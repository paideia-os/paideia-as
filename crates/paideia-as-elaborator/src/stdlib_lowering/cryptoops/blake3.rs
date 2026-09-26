//! Lowering recipes for `Blake3::{hash, hash_keyed, derive_key}` —
//! paideia-as#1545 (Wave-1 companion to the Wave-0 landing in
//! `paideia-as-crypto::blake3`).
//!
//! Routes the source-level `Blake3::hash(...)`,
//! `Blake3::hash_keyed(...)`, and `Blake3::derive_key(...)` calls to
//! the extern-C thunks `paideia_crypto_blake3_{hash, hash_keyed,
//! derive_key}` in `paideia-as-crypto::blake3::ffi`. Split out per the
//! paideia-as#1354 one-primitive-per-file discipline, same shape as
//! [`super::hkdf`] / [`super::ed25519`].
//!
//! # Reference
//!
//! J.-P. Aumasson, S. Neves, Z. Wilcox-O'Hearn, C. Winnerlein,
//! *"BLAKE3 — one function, fast everywhere"* (2020-01-09), pinning
//! keyed hash (§2.1, 32-byte key) and KDF context (§7.5, ASCII
//! domain-separating label). The FFI-thunk register mappings below
//! mirror the tables at
//! `paideia-as-crypto/src/blake3.rs` on each `#[unsafe(no_mangle)]`
//! entry point.
//!
//! # Register contract (from `emit_call`'s SysV marshaller)
//!
//! * `Blake3::hash(data_ptr, data_len, out_ptr) -> i64`
//!   * RDI = `data_ptr` / RSI = `data_len` / RDX = `out_ptr`
//!   * RAX = return code (`PDX_CRYPTO_OK` / `_ERR_INVALID_PARAM`)
//! * `Blake3::hash_keyed(key_ptr, data_ptr, data_len, out_ptr) -> i64`
//!   * RDI = `key_ptr` (`*const [u8; 32]`, non-NULL) /
//!     RSI = `data_ptr` / RDX = `data_len` / RCX = `out_ptr`
//!   * RAX = return code
//! * `Blake3::derive_key(context_ptr, context_len, key_material_ptr,
//!    key_material_len, out_ptr) -> i64`
//!   * RDI = `context_ptr` / RSI = `context_len` /
//!     RDX = `key_material_ptr` / RCX = `key_material_len` /
//!     R8 = `out_ptr`
//!   * RAX = return code
//!
//! The `.pdx` trait declaration is redeclared module-locally by each
//! consumer (same convention as [`super::hkdf`] / [`super::ed25519`]).

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `Blake3::hash`.
const SYM_BLAKE3_HASH: &str = "paideia_crypto_blake3_hash";
/// Extern-C symbol for `Blake3::hash_keyed`.
const SYM_BLAKE3_HASH_KEYED: &str = "paideia_crypto_blake3_hash_keyed";
/// Extern-C symbol for `Blake3::derive_key`.
const SYM_BLAKE3_DERIVE_KEY: &str = "paideia_crypto_blake3_derive_key";

/// Dispatch a `Blake3::<method_name>` call to its lowering recipe.
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
        "hash" => Some(Ok(extern_recipe(SYM_BLAKE3_HASH))),
        "hash_keyed" => Some(Ok(extern_recipe(SYM_BLAKE3_HASH_KEYED))),
        "derive_key" => Some(Ok(extern_recipe(SYM_BLAKE3_DERIVE_KEY))),
        _ => None,
    }
}
