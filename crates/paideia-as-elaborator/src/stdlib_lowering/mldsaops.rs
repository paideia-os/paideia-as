//! `MlDsa65::sign` / `MlDsa65::verify` stdlib-lowering recipes
//! (paideia-as#1330 sign, paideia-as#1347 verify).
//!
//! Mirrors `cryptoops` (#1305): ML-DSA-65 sign + verify are each
//! thousands of lines of Rust (key expansion + rejection-sampling
//! loop; NTT + hint decoding + polynomial-multiply chain), so both
//! MUST lower to a `call` into an extern-C thunk rather than inline
//! mnemonics — unlike `cpuid_leaf_ad`/`cpuid_leaf_bc`, which splice a
//! handful of instructions directly.
//!
//! # Calling convention — sign (choice A: caller-allocated output buffer)
//!
//! A 3309-byte signature does not fit in RAX (or even RAX:RDX). Two
//! shapes were possible: (A) the caller passes a pointer to a
//! caller-allocated `MLDSA65_SIG_BYTES`-byte buffer and the thunk
//! writes into it, returning a status code in RAX; (B) the callee
//! returns a `{ bytes: [u8; 3309] }` record via an sret slot. (A) is
//! chosen because it matches how every other extern-C crypto thunk in
//! this codebase already works (`paideia_crypto_argon2id_derive`,
//! `paideia_crypto_chacha20_poly1305_seal`/`open` — see `cryptoops.rs`)
//! and does not require the record-return / sret marshalling that
//! `cpuidops.rs` explicitly defers to a separate design pass.
//!
//! ```text
//! MlDsa65::sign(seed_ptr, msg_ptr, msg_len, sig_out_ptr) -> i64
//!   RDI  seed_ptr     *const u8 (32-byte ML-DSA-65 seed)
//!   RSI  msg_ptr      *const u8
//!   RDX  msg_len      u64
//!   RCX  sig_out_ptr  *mut u8 (>= 3309 bytes, MLDSA65_SIG_LEN)
//!   RAX  return code: 0 = success, non-zero = error
//! ```
//!
//! # Calling convention — verify (all 6 SysV regs, boolean via RAX)
//!
//! Verify has no output buffer; the six SysV integer registers are
//! all used for `(msg, sig, pubkey)` `(ptr, len)` pairs, and the
//! boolean is projected onto the same `0 = OK / negative = error`
//! shape sign uses so both share a diagnostic surface.
//!
//! ```text
//! MlDsa65::verify(msg_ptr, msg_len, sig_ptr, sig_len,
//!                 pubkey_ptr, pubkey_len) -> i64
//!   RDI  msg_ptr      *const u8
//!   RSI  msg_len      u64
//!   RDX  sig_ptr      *const u8 (== 3309 bytes)
//!   RCX  sig_len      u64 (== 3309)
//!   R8   pubkey_ptr   *const u8 (== 1952 bytes)
//!   R9   pubkey_len   u64 (== 1952)
//!   RAX  return code: 0 = valid, negative = invalid or bad shape
//! ```
//!
//! # Effect + capability discipline
//!
//! `!{crypto, mem} @{paideia.crypto}` — the same effect row and
//! capability `crypto.pdx` already declares for `Argon2id::derive` /
//! `ChaCha20Poly1305::seal`/`open`. Both ML-DSA-65 sign and verify
//! read caller buffers (verify writes none), which is the same
//! "crypto primitive touching caller memory" shape those already
//! cover; reusing the capability avoids fragmenting the crypto cap
//! surface across near-identical primitives ahead of a dedicated
//! per-key-material capability design (tracked as a follow-up, not
//! required to land these intrinsics).

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

/// Extern-C symbol for `MlDsa65::sign` — must match the
/// `#[unsafe(no_mangle)]` name in `paideia-pq-sign::ffi`.
const SYM_MLDSA65_SIGN: &str = "mldsa65_sign_runtime_entry";

/// Extern-C symbol for `MlDsa65::verify` — must match the
/// `#[unsafe(no_mangle)]` name in `paideia-pq-sign::ffi`.
const SYM_MLDSA65_VERIFY: &str = "mldsa65_verify_runtime_entry";

/// Dispatch an `MlDsa65::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission, which then fails T0553 — deliberate: an unknown
/// ML-DSA method must not silently emit a call to a non-existent
/// symbol).
pub(super) fn try_lower(
    method_name: &str,
    _mode: InstrMode,
    _arg_ids: &[IrNodeId],
    _arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "sign" => Some(Ok(LoweringRecipe {
            instructions: vec![],
            arg_convention: ArgConvention::SysVRegs,
            labels: vec![],
            extern_target: Some(SYM_MLDSA65_SIGN.to_string()),
        })),
        "verify" => Some(Ok(LoweringRecipe {
            instructions: vec![],
            arg_convention: ArgConvention::SysVRegs,
            labels: vec![],
            extern_target: Some(SYM_MLDSA65_VERIFY.to_string()),
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::IrArena;

    #[test]
    fn mldsa65_sign_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = try_lower("sign", InstrMode::Mode64, &[], &arena)
            .expect("MlDsa65::sign recipe should exist")
            .expect("MlDsa65::sign lowering should succeed");

        assert!(
            recipe.instructions.is_empty(),
            "extern-C recipes carry no preamble instructions"
        );
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert!(recipe.labels.is_empty());
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("mldsa65_sign_runtime_entry")
        );
    }

    #[test]
    fn mldsa65_verify_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = try_lower("verify", InstrMode::Mode64, &[], &arena)
            .expect("MlDsa65::verify recipe should exist")
            .expect("MlDsa65::verify lowering should succeed");

        assert!(
            recipe.instructions.is_empty(),
            "extern-C recipes carry no preamble instructions"
        );
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert!(recipe.labels.is_empty());
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("mldsa65_verify_runtime_entry")
        );
    }

    #[test]
    fn unknown_mldsa65_method_returns_none() {
        let arena = IrArena::new();
        assert!(try_lower("no_such_method", InstrMode::Mode64, &[], &arena).is_none());
    }
}
