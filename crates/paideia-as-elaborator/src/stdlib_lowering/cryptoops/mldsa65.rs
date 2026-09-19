//! Lowering recipes for `MlDsa65C::{sign, verify}` — Wave υ
//! (paideia-as υ-01 / υ-02).
//!
//! Routes the source-level `MlDsa65C::…` calls to the compact
//! extern-C thunks `mldsa65_{sign, verify}` in
//! `paideia-as-crypto::ffi::ml_dsa_65`. The trait name deliberately
//! carries the `C` suffix ("Compact" — 5-argument SysV-clean surface,
//! `u32` / `u64` returns) to disambiguate from the pre-existing
//! `MlDsa65` trait, whose dispatch in `stdlib_lowering::mldsaops`
//! routes to the std-linked `paideia-pq-sign::ffi::
//! mldsa65_{sign,verify}_runtime_entry` thunks with the richer
//! `i64` return contract.
//!
//! # Why two traits
//!
//! The pre-existing `MlDsa65` trait is consumed by tools built on
//! the offline signer path (`libpdx-volume` `pdxb_sign_superblock`,
//! `umount.pdxfs` `unmount_op_sign_locked`, and the `.pdxpkg` /
//! `.pdxtrust` verify chains once those repos exist) at std-linked
//! CLI-side call sites. Retiring those symbols out from under those
//! call sites in this wave is out of scope; the migration path is a
//! future wave that flips the `MlDsa65` dispatch to point at these
//! thunks and drops the pq-sign runtime-entry surface once every
//! consumer is on the compact ABI.
//!
//! # Calling convention — sign
//!
//! ```text
//! MlDsa65C::sign(msg_ptr, msg_len, sk_ptr,
//!                out_sig_ptr, out_sig_max) -> u64
//!   RDI  msg_ptr       *const u8 (NULL only when msg_len == 0)
//!   RSI  msg_len        usize
//!   RDX  sk_ptr        *const u8 (32-byte ML-DSA-65 seed)
//!   RCX  out_sig_ptr   *mut u8   (>= out_sig_max bytes)
//!   R8   out_sig_max    usize    (MUST be >= 3309)
//!   RAX  bytes written on success (always 3309), or 0 on failure
//! ```
//!
//! # Calling convention — verify
//!
//! ```text
//! MlDsa65C::verify(msg_ptr, msg_len, sig_ptr, sig_len, pk_ptr) -> u32
//!   RDI  msg_ptr    *const u8 (NULL only when msg_len == 0)
//!   RSI  msg_len     usize
//!   RDX  sig_ptr    *const u8 (must be non-NULL)
//!   RCX  sig_len     usize    (MUST equal 3309)
//!   R8   pk_ptr     *const u8 (must be non-NULL; length fixed at 1952)
//!   RAX  return: 0 = verify OK, 1 = verify FAIL
//! ```
//!
//! # Effect + capability discipline
//!
//! `!{crypto, mem} @{paideia.crypto}` — same effect row and
//! capability as `MlDsa65::sign` / `MlDsa65::verify` (see
//! `stdlib_lowering::mldsaops` for the rationale). ML-DSA-65 read /
//! write the caller's buffers, which is the same "crypto primitive
//! touching caller memory" shape those already cover.
//!
//! The `.pdx` trait declaration for `MlDsa65C` lives at
//! `crates/paideia-as-stdlib/pdx/crypto/mldsa_65_c.pdx` (Wave υ).

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `MlDsa65C::sign` — must match the
/// `#[unsafe(no_mangle)]` name in `paideia-as-crypto::ffi::ml_dsa_65`.
const SYM_MLDSA65_SIGN: &str = "mldsa65_sign";

/// Extern-C symbol for `MlDsa65C::verify` — must match the
/// `#[unsafe(no_mangle)]` name in `paideia-as-crypto::ffi::ml_dsa_65`.
const SYM_MLDSA65_VERIFY: &str = "mldsa65_verify";

/// Dispatch an `MlDsa65C::<method_name>` call to its lowering recipe.
///
/// Unknown methods return `None` so an accidental typo (e.g.
/// `MlDsa65C::signn`) falls through to normal call emission and
/// diagnoses T0553 rather than silently emitting a call to an
/// unresolved extern symbol.
pub(super) fn try_lower(
    method_name: &str,
    _mode: InstrMode,
    _arg_ids: &[IrNodeId],
    _arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "sign" => Some(Ok(extern_recipe(SYM_MLDSA65_SIGN))),
        "verify" => Some(Ok(extern_recipe(SYM_MLDSA65_VERIFY))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdlib_lowering::ArgConvention;
    use paideia_as_ir::IrArena;

    #[test]
    fn mldsa65_c_sign_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = try_lower("sign", InstrMode::Mode64, &[], &arena)
            .expect("MlDsa65C::sign recipe should exist")
            .expect("MlDsa65C::sign lowering should succeed");
        assert!(recipe.instructions.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert!(recipe.labels.is_empty());
        assert_eq!(recipe.extern_target.as_deref(), Some(SYM_MLDSA65_SIGN));
    }

    #[test]
    fn mldsa65_c_verify_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = try_lower("verify", InstrMode::Mode64, &[], &arena)
            .expect("MlDsa65C::verify recipe should exist")
            .expect("MlDsa65C::verify lowering should succeed");
        assert!(recipe.instructions.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert!(recipe.labels.is_empty());
        assert_eq!(recipe.extern_target.as_deref(), Some(SYM_MLDSA65_VERIFY));
    }

    #[test]
    fn unknown_mldsa65_c_method_returns_none() {
        let arena = IrArena::new();
        assert!(try_lower("no_such_method", InstrMode::Mode64, &[], &arena).is_none());
    }
}
