//! `Argon2id` / `ChaCha20Poly1305` stdlib-lowering recipes
//! (paideia-as#1305).
//!
//! # Design
//!
//! These recipes route trait-method calls in `.pdx` source to the
//! extern-C thunks in `crates/paideia-as-crypto/src/ffi/mod.rs`. The
//! recipes are deliberately minimal — an empty instruction sequence
//! plus an `extern_target` symbol name — because emit_call already
//! does the interesting work:
//!
//! 1. Marshals the caller's SysV argument registers (RDI, RSI, RDX,
//!    RCX, R8, R9) with the exact operand shape the FFI thunks
//!    require (see the register table on each thunk's doc comment).
//! 2. Saves any live caller-save scratch registers (RCX, RDX, R8, R9)
//!    across the call — the FFI thunks are ordinary Rust code and
//!    clobber all caller-save registers per SysV ABI, so the caller
//!    MUST have those regs saved beforehand.
//! 3. Emits `call <extern_target>` with a symbol relocation.
//! 4. Restores caller-save scratch and the SysV / MS postlude.
//!
//! # Why extern-C rather than paideia-native
//!
//! Argon2id and ChaCha20-Poly1305 together are ~5k LoC of constant-
//! time crypto (RFC 9106 and RFC 8439) — see
//! `design/toolchain/rust-dep-gap-analysis.md` (i) which classifies
//! a full `.pdx` port as **Phase 6+** work. Meanwhile paideia-os R48
//! (user management, sealed `user_sk`) needs the primitives now
//! (`design/user/model.md` §2.1, §9.2). The bridge is a Rust static
//! rlib (`paideia-as-crypto`) whose `ffi` module exposes SysV C
//! entry points; consumers link against it. When the paideia-native
//! rewrite lands, it can register itself under the same symbol names
//! and no `.pdx` caller changes.
//!
//! # Trait names — matching the Rust type names
//!
//! `.pdx` code calls these as `Argon2id::derive(...)` and
//! `ChaCha20Poly1305::seal(...)` / `open(...)` — the trait names on
//! the source-language side deliberately match the Rust type names in
//! `paideia-as-crypto`. This keeps the two sides of the FFI boundary
//! spelt the same way, so a reader tracing a call site from `.pdx` to
//! Rust never has to memorise a translation table.
//!
//! # Register contract (exhaustive)
//!
//! * `Argon2id::derive(params_ptr, out_ptr, out_len) -> i64`
//!   * RDI = `params_ptr` (`*const Argon2idParamsC`)
//!   * RSI = `out_ptr`
//!   * RDX = `out_len`
//!   * RAX = return code (0 OK, negative = error)
//! * `ChaCha20Poly1305::seal(params, pt_ptr, pt_len, out_ptr, out_cap, written) -> i64`
//!   * RDI = `params` / RSI = `pt_ptr` / RDX = `pt_len` /
//!     RCX = `out_ptr` / R8 = `out_cap` / R9 = `written` (`*mut usize`)
//!   * RAX = return code
//! * `ChaCha20Poly1305::open(...)` — same shape as `seal`.
//!
//! The `.pdx` trait declarations at
//! `crates/paideia-stdlib/pdx/crypto.pdx` pin the source-level
//! signatures; the recipe below is what routes each spelling to its
//! extern symbol.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

/// Extern-C symbol for `Argon2id::derive` — must match the
/// `#[unsafe(no_mangle)]` name in `paideia-as-crypto::ffi`.
const SYM_ARGON2ID_DERIVE: &str = "paideia_crypto_argon2id_derive";
/// Extern-C symbol for `ChaCha20Poly1305::seal`.
const SYM_CHACHA_SEAL: &str = "paideia_crypto_chacha20_poly1305_seal";
/// Extern-C symbol for `ChaCha20Poly1305::open`.
const SYM_CHACHA_OPEN: &str = "paideia_crypto_chacha20_poly1305_open";
/// Extern-C symbol for `MlKem768::keygen` — paideia-as#1352.
const SYM_ML_KEM_768_KEYGEN: &str = "paideia_crypto_ml_kem_768_keygen";
/// Extern-C symbol for `MlKem768::encaps`.
const SYM_ML_KEM_768_ENCAPS: &str = "paideia_crypto_ml_kem_768_encaps";
/// Extern-C symbol for `MlKem768::decaps`.
const SYM_ML_KEM_768_DECAPS: &str = "paideia_crypto_ml_kem_768_decaps";

/// Dispatch an `Argon2id::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission, which then fails T0553 — deliberate: an unknown
/// crypto method must not silently emit a call to a non-existent
/// symbol).
pub(super) fn try_lower_argon2id(
    method_name: &str,
    _mode: InstrMode,
    _arg_ids: &[IrNodeId],
    _arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "derive" => Some(Ok(extern_recipe(SYM_ARGON2ID_DERIVE))),
        _ => None,
    }
}

/// Dispatch a `ChaCha20Poly1305::<method_name>` call to its lowering
/// recipe.
pub(super) fn try_lower_chacha20_poly1305(
    method_name: &str,
    _mode: InstrMode,
    _arg_ids: &[IrNodeId],
    _arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "seal" => Some(Ok(extern_recipe(SYM_CHACHA_SEAL))),
        "open" => Some(Ok(extern_recipe(SYM_CHACHA_OPEN))),
        _ => None,
    }
}

/// Dispatch a `MlKem768::<method_name>` call to its lowering recipe.
///
/// Three FIPS 203 primitives are wired: `keygen(d, z, ek_out,
/// dk_out) -> i64`, `encaps(ek, m, ct_out, ss_out) -> i64`, and
/// `decaps(dk, ct, ss_out) -> i64`. Every buffer's length is a
/// compile-time constant of the ML-KEM-768 parameter set (FIPS 203
/// §7); the FFI thunks in `paideia-as-crypto::ffi` cast the raw
/// pointers to the corresponding fixed-size array references. The
/// SysV register mappings are documented on each thunk.
///
/// Unknown methods return `None` so an accidental typo (e.g.
/// `MlKem768::keygenn`) falls through to normal call emission and
/// diagnoses T0553 rather than silently emitting a call to an
/// unresolved extern symbol.
pub(super) fn try_lower_ml_kem_768(
    method_name: &str,
    _mode: InstrMode,
    _arg_ids: &[IrNodeId],
    _arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "keygen" => Some(Ok(extern_recipe(SYM_ML_KEM_768_KEYGEN))),
        "encaps" => Some(Ok(extern_recipe(SYM_ML_KEM_768_ENCAPS))),
        "decaps" => Some(Ok(extern_recipe(SYM_ML_KEM_768_DECAPS))),
        _ => None,
    }
}

/// Build a SysVRegs recipe with no preamble instructions whose CALL
/// target is `sym`. emit_call marshals SysV args, splices the (empty)
/// preamble, then emits `call sym` and restores caller-save scratch.
fn extern_recipe(sym: &str) -> LoweringRecipe {
    LoweringRecipe {
        instructions: vec![],
        arg_convention: ArgConvention::SysVRegs,
        labels: vec![],
        extern_target: Some(sym.to_string()),
    }
}
