//! Lowering recipes for `ChaCha20Poly1305::seal` / `open` —
//! paideia-as#1305.
//!
//! Routes the source-level `ChaCha20Poly1305::seal(...)` /
//! `open(...)` calls to the extern-C thunks
//! `paideia_crypto_chacha20_poly1305_{seal, open}` in
//! `paideia-as-crypto::ffi::chacha20_poly1305`. Split out of the
//! monolithic `cryptoops.rs` per paideia-as#1354.
//!
//! # Register contract (from `emit_call`'s SysV marshaller)
//!
//! * `ChaCha20Poly1305::seal(params, pt_ptr, pt_len, out_ptr, out_cap, written) -> i64`
//!   * RDI = `params` / RSI = `pt_ptr` / RDX = `pt_len` /
//!     RCX = `out_ptr` / R8 = `out_cap` / R9 = `written` (`*mut usize`)
//!   * RAX = return code
//! * `ChaCha20Poly1305::open(...)` — same shape as `seal`.
//!
//! The `.pdx` trait declaration lives in
//! `crates/paideia-as-stdlib/pdx/crypto/chacha20_poly1305.pdx`.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `ChaCha20Poly1305::seal`.
const SYM_CHACHA_SEAL: &str = "paideia_crypto_chacha20_poly1305_seal";
/// Extern-C symbol for `ChaCha20Poly1305::open`.
const SYM_CHACHA_OPEN: &str = "paideia_crypto_chacha20_poly1305_open";

/// Dispatch a `ChaCha20Poly1305::<method_name>` call to its lowering
/// recipe. Returns `None` for unknown methods — see the
/// [module-level rationale](super) on why unknown-method fall-through
/// is deliberate.
pub(super) fn try_lower(
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
