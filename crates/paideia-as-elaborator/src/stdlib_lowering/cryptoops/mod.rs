//! `Argon2id` / `ChaCha20Poly1305` / `MlKem768` stdlib-lowering
//! recipes (paideia-as#1305, #1352).
//!
//! # Design
//!
//! These recipes route trait-method calls in `.pdx` source to the
//! extern-C thunks in `crates/paideia-as-crypto/src/ffi/`. The
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
//! `.pdx` code calls these as `Argon2id::derive(...)`,
//! `ChaCha20Poly1305::seal(...)` / `open(...)`, and
//! `MlKem768::{keygen, encaps, decaps}(...)` — the trait names on
//! the source-language side deliberately match the Rust type names
//! in `paideia-as-crypto`. This keeps the two sides of the FFI
//! boundary spelt the same way, so a reader tracing a call site
//! from `.pdx` to Rust never has to memorise a translation table.
//!
//! # Module layout (paideia-as#1354)
//!
//! Per-primitive dispatchers live in sibling files so parallel
//! authoring of the v0.25-v0.32 crypto waves never collides inside
//! a shared file:
//!
//! - [`argon2id`] — `Argon2id::derive`.
//! - [`chacha20_poly1305`] — `ChaCha20Poly1305::{seal, open}`.
//! - [`ml_kem_768`] — `MlKem768::{keygen, encaps, decaps}`.
//!
//! Each sub-module exposes a `try_lower(method_name, mode, arg_ids,
//! arena)` fn scoped `pub(super)` and is fronted at this module by
//! `try_lower_argon2id`, `try_lower_chacha20_poly1305`, and
//! `try_lower_ml_kem_768` — the names `stdlib_lowering::mod.rs`
//! dispatches to.
//!
//! The `.pdx` trait declarations at
//! `crates/paideia-as-stdlib/pdx/crypto/*.pdx` pin the source-level
//! signatures; the recipes below are what route each spelling to its
//! extern symbol.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

mod argon2id;
mod chacha20_poly1305;
mod ml_kem_768;

/// Dispatch an `Argon2id::<method_name>` call to its lowering recipe.
///
/// Delegates to [`argon2id::try_lower`]; kept as a named entry point
/// on the parent module so `stdlib_lowering::mod.rs`'s dispatch table
/// stays a flat `match trait_name` shape.
pub(super) fn try_lower_argon2id(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    argon2id::try_lower(method_name, mode, arg_ids, arena)
}

/// Dispatch a `ChaCha20Poly1305::<method_name>` call to its lowering
/// recipe. Delegates to [`chacha20_poly1305::try_lower`].
pub(super) fn try_lower_chacha20_poly1305(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    chacha20_poly1305::try_lower(method_name, mode, arg_ids, arena)
}

/// Dispatch a `MlKem768::<method_name>` call to its lowering recipe.
/// Delegates to [`ml_kem_768::try_lower`].
pub(super) fn try_lower_ml_kem_768(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    ml_kem_768::try_lower(method_name, mode, arg_ids, arena)
}

/// Build a SysVRegs recipe with no preamble instructions whose CALL
/// target is `sym`. emit_call marshals SysV args, splices the (empty)
/// preamble, then emits `call sym` and restores caller-save scratch.
///
/// Shared helper: every per-primitive sub-module in this directory
/// calls it. Kept at the parent module so the SysVRegs / no-preamble
/// / no-labels shape is authoritative in one place — a future wave
/// adding a new AEAD or KEM re-uses the same helper rather than
/// re-inlining the LoweringRecipe literal.
pub(super) fn extern_recipe(sym: &str) -> LoweringRecipe {
    LoweringRecipe {
        instructions: vec![],
        arg_convention: ArgConvention::SysVRegs,
        labels: vec![],
        extern_target: Some(sym.to_string()),
    }
}
