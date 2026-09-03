//! Lowering recipe for `Argon2id::derive` — paideia-as#1305.
//!
//! Routes the source-level `Argon2id::derive(params, out, out_len)`
//! call to the extern-C thunk `paideia_crypto_argon2id_derive` in
//! `paideia-as-crypto::ffi::argon2id`. Split out of the monolithic
//! `cryptoops.rs` per paideia-as#1354 so parallel authoring of the
//! v0.25-v0.32 crypto waves never collides inside one file.
//!
//! # Register contract (from `emit_call`'s SysV marshaller)
//!
//! * `Argon2id::derive(params_ptr, out_ptr, out_len) -> i64`
//!   * RDI = `params_ptr` (`*const Argon2idParamsC`)
//!   * RSI = `out_ptr`
//!   * RDX = `out_len`
//!   * RAX = return code (0 OK, negative = error)
//!
//! The `.pdx` trait declaration lives in
//! `crates/paideia-stdlib/pdx/crypto/argon2id.pdx`.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `Argon2id::derive` — must match the
/// `#[unsafe(no_mangle)]` name in `paideia-as-crypto::ffi::argon2id`.
const SYM_ARGON2ID_DERIVE: &str = "paideia_crypto_argon2id_derive";

/// Dispatch an `Argon2id::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission, which then fails T0553 — deliberate: an unknown
/// crypto method must not silently emit a call to a non-existent
/// symbol).
pub(super) fn try_lower(
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
