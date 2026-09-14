//! Lowering recipe for `Ed25519::verify` — paideia-as Wave γ (γ-02).
//!
//! Routes the source-level `Ed25519::verify(...)` call to the
//! extern-C thunk `paideia_crypto_ed25519_verify` in
//! `paideia-as-crypto::ffi::ed25519`. Split out per the
//! paideia-as#1354 one-primitive-per-file discipline, same shape as
//! [`super::chacha20_poly1305`].
//!
//! # Register contract (from `emit_call`'s SysV marshaller)
//!
//! * `Ed25519::verify(pk_ptr, sig_ptr, msg_ptr, msg_len) -> i32`
//!   * RDI = `pk_ptr` (`*const u8`, 32 bytes) / RSI = `sig_ptr`
//!     (`*const u8`, 64 bytes) / RDX = `msg_ptr` / RCX = `msg_len`
//!   * EAX = `PDX_ED25519_VALID` (1) / `_INVALID` (0) /
//!     `_ERR_INVALID_PARAM` (-1)
//!
//! Note the narrower-than-usual `i32` return width (every other
//! crypto thunk in this crate returns `i64`) — `emit_call`'s
//! extern-target path does not inspect or reinterpret a callee's
//! return value at all (the recipe below carries zero preamble/
//! postamble instructions, same as every other primitive here), so
//! this is purely a `.pdx`-caller-side contract: callers MUST treat
//! only the low 32 bits of RAX as meaningful, since the SysV ABI does
//! not guarantee the upper 32 bits of RAX are zeroed after a callee
//! that returns `i32`.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `Ed25519::verify`.
const SYM_ED25519_VERIFY: &str = "paideia_crypto_ed25519_verify";

/// Dispatch an `Ed25519::<method_name>` call to its lowering recipe.
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
        "verify" => Some(Ok(extern_recipe(SYM_ED25519_VERIFY))),
        _ => None,
    }
}
