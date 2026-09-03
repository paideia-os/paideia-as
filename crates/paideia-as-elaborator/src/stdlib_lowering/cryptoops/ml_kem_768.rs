//! Lowering recipes for `MlKem768::{keygen, encaps, decaps}` —
//! paideia-as#1352.
//!
//! Routes the source-level `MlKem768::…` calls to the extern-C thunks
//! `paideia_crypto_ml_kem_768_{keygen, encaps, decaps}` in
//! `paideia-as-crypto::ffi::ml_kem_768`. Split out of the monolithic
//! `cryptoops.rs` per paideia-as#1354.
//!
//! Three FIPS 203 primitives are wired: `keygen(d, z, ek_out,
//! dk_out) -> i64`, `encaps(ek, m, ct_out, ss_out) -> i64`, and
//! `decaps(dk, ct, ss_out) -> i64`. Every buffer's length is a
//! compile-time constant of the ML-KEM-768 parameter set (FIPS 203
//! §7); the FFI thunks in `paideia-as-crypto::ffi::ml_kem_768` cast
//! the raw pointers to the corresponding fixed-size array references.
//! The SysV register mappings are documented on each thunk.
//!
//! Unknown methods return `None` so an accidental typo (e.g.
//! `MlKem768::keygenn`) falls through to normal call emission and
//! diagnoses T0553 rather than silently emitting a call to an
//! unresolved extern symbol.
//!
//! The `.pdx` trait declaration lives in
//! `crates/paideia-as-stdlib/pdx/crypto/ml_kem_768.pdx`.

use paideia_as_ir::{IrArena, IrNodeId, instruction::InstrMode};

use super::super::{LoweringRecipe, StdlibLoweringError};
use super::extern_recipe;

/// Extern-C symbol for `MlKem768::keygen`.
const SYM_ML_KEM_768_KEYGEN: &str = "paideia_crypto_ml_kem_768_keygen";
/// Extern-C symbol for `MlKem768::encaps`.
const SYM_ML_KEM_768_ENCAPS: &str = "paideia_crypto_ml_kem_768_encaps";
/// Extern-C symbol for `MlKem768::decaps`.
const SYM_ML_KEM_768_DECAPS: &str = "paideia_crypto_ml_kem_768_decaps";

/// Dispatch a `MlKem768::<method_name>` call to its lowering recipe.
pub(super) fn try_lower(
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
