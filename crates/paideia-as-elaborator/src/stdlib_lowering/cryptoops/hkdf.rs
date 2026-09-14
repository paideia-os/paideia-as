//! Lowering recipe for `Hkdf::sha256` — paideia-as Wave γ (γ-01).
//!
//! Routes the source-level `Hkdf::sha256(...)` call to the extern-C
//! thunk `paideia_crypto_hkdf_sha256` in
//! `paideia-as-crypto::ffi::hkdf`. Split out per the paideia-as#1354
//! one-primitive-per-file discipline, same shape as
//! [`super::chacha20_poly1305`].
//!
//! # Register contract (from `emit_call`'s SysV marshaller)
//!
//! * `Hkdf::sha256(params, mode, out_ptr, out_len) -> i64`
//!   * RDI = `params` (`*const HkdfParamsC`) / RSI = `mode`
//!     (`HKDF_MODE_EXTRACT` / `_EXPAND` / `_EXTRACT_AND_EXPAND`) /
//!     RDX = `out_ptr` / RCX = `out_len`
//!   * RAX = return code (`PDX_CRYPTO_*`)
//!
//! The `.pdx` trait declaration is redeclared module-locally by each
//! consumer (e.g. `libpdx-net/src/net_tls_key_schedule.pdx`), mirroring
//! the established convention documented on
//! [`super::chacha20_poly1305`] and on `pdxb_crypto.pdx` in
//! `tools/user/libpdx-volume` — the elaborator dispatches on the
//! literal `(trait_name, method_name)` pair at the call site, not on a
//! cross-crate import, so there is no single canonical `.pdx` trait
//! declaration to point at.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `Hkdf::sha256`.
const SYM_HKDF_SHA256: &str = "paideia_crypto_hkdf_sha256";

/// Dispatch an `Hkdf::<method_name>` call to its lowering recipe.
/// Returns `None` for unknown methods — see the
/// [module-level rationale](super) on why unknown-method fall-through
/// is deliberate.
pub(super) fn try_lower(
    method_name: &str,
    _mode: InstrMode,
    _arg_ids: &[IrNodeId],
    _arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "sha256" => Some(Ok(extern_recipe(SYM_HKDF_SHA256))),
        _ => None,
    }
}
