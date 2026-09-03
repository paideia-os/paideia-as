//! C-ABI thunks over the [`crate::kdf::Kdf`] and [`crate::aead::Aead`]
//! trait implementations.
//!
//! ## Why a separate ffi module
//!
//! The trait API in `kdf` / `aead` uses GAT-bearing parameter bundles
//! and `Vec<u8>` returns — shapes that cannot cross a `.pdx` call site,
//! since `.pdx` code marshals args through the SysV integer-register
//! convention (RDI/RSI/RDX/RCX/R8/R9) and cannot express a Rust GAT
//! lifetime. This module flattens those APIs into plain
//! `pub extern "C" fn` symbols whose arguments are pointer + length
//! pairs packed into fixed-shape C structs.
//!
//! Consumers on the `.pdx` side reach these via the
//! `stdlib_lowering::cryptoops` recipes (issue paideia-as#1305), which
//! emit `call paideia_crypto_argon2id_derive` (etc.) after marshalling
//! the SysV argument registers per the signatures below. The paideia-as
//! elaborator does not need to know about this module directly; it only
//! emits the CALL relocation. Downstream consumers (paideia-os, host
//! tooling) satisfy the symbol at link time by depending on this crate.
//!
//! ## Module layout (paideia-as#1354)
//!
//! Per-primitive FFI thunks live in sibling files so parallel authoring
//! of the v0.25-v0.32 crypto waves never collides inside a shared
//! monolithic module:
//!
//! - [`argon2id`] — RFC 9106 password-based KDF.
//! - [`chacha20_poly1305`] — RFC 8439 AEAD.
//! - [`ml_kem_768`] — FIPS 203 KEM (paideia-as#1352).
//!
//! Every thunk and its `#[repr(C)]` parameter bundle is re-exported at
//! the `ffi::` root, so `paideia_as_crypto::ffi::paideia_crypto_*` (the
//! path the satellite runtime, external `nm` audits, and doc comments
//! all use) keeps resolving byte-identically to before the split.
//!
//! Shared items live in this file so they need not be duplicated (or
//! diverge) across primitives:
//!
//! - Error-code band (`PDX_CRYPTO_*`).
//! - Trait-error → FFI-code translators (`aead_err_code`,
//!   `kem_err_code`).
//! - `AeadParamsC` → typed-borrow converter (`params_from_c`).
//!
//! ## Error-code discipline
//!
//! Every entry point returns a small negative integer for failure and
//! a non-negative integer for success. Callers MUST NOT dereference
//! output pointers when the return is negative — the failure paths do
//! not touch the output buffer beyond what the underlying trait method
//! itself would touch (which is nothing on failure).
//!
//! Codes match across primitives so a caller can share diagnostic
//! handlers:
//!
//! | Code   | Meaning                                                 |
//! |--------|---------------------------------------------------------|
//! |  0     | success                                                 |
//! | -1     | invalid parameter (`KdfError::InvalidParams` etc.)      |
//! | -2     | invalid input length (key / nonce / output)             |
//! | -3     | authentication failed (AEAD open only)                  |
//! | -4     | primitive-internal failure (crate-level error)          |
//! | -5     | output buffer too small                                 |
//!
//! ## Safety
//!
//! Every entry point is `unsafe fn` in intent even though the signature
//! is `extern "C" fn` (the C ABI does not carry `unsafe`). The
//! preconditions are enumerated on each function. Violating them is
//! undefined behaviour.
//!
//! This is the only module in the crate that touches raw pointers.
//! The crate-level `deny(unsafe_code)` guards every other module; the
//! `#![allow(unsafe_code)]` below lifts it here for the C-shim path
//! only. `#[unsafe(no_mangle)]` is a Rust 2024 requirement for
//! exported symbols.

#![allow(unsafe_code)]

use core::slice;

use crate::aead::{AeadError, ChaCha20Poly1305Params, KEY_LEN, NONCE_LEN};
use crate::kem::KemError;

pub mod argon2id;
pub mod chacha20_poly1305;
pub mod ml_kem_768;

// ---------------------------------------------------------------------
// Public re-exports — the pre-split `ffi::…` paths continue to resolve
// byte-identically. Every symbol on a satellite `nm` dump or a
// downstream `use paideia_as_crypto::ffi::…` line resolves to the same
// item as before the paideia-as#1354 split; only the file that hosts
// the item changed.
// ---------------------------------------------------------------------

pub use argon2id::Argon2idParamsC;
pub use argon2id::paideia_crypto_argon2id_derive;

pub use chacha20_poly1305::AeadParamsC;
pub use chacha20_poly1305::paideia_crypto_chacha20_poly1305_open;
pub use chacha20_poly1305::paideia_crypto_chacha20_poly1305_seal;

pub use ml_kem_768::PDX_ML_KEM_768_CT_LEN;
pub use ml_kem_768::PDX_ML_KEM_768_DK_LEN;
pub use ml_kem_768::PDX_ML_KEM_768_EK_LEN;
pub use ml_kem_768::PDX_ML_KEM_768_SEED_LEN;
pub use ml_kem_768::PDX_ML_KEM_768_SS_LEN;
pub use ml_kem_768::paideia_crypto_ml_kem_768_decaps;
pub use ml_kem_768::paideia_crypto_ml_kem_768_encaps;
pub use ml_kem_768::paideia_crypto_ml_kem_768_keygen;

// ---------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------

/// Success return code.
pub const PDX_CRYPTO_OK: i64 = 0;
/// A parameter fell outside the range the primitive accepts.
pub const PDX_CRYPTO_ERR_INVALID_PARAM: i64 = -1;
/// An input length (key / nonce / output) is wrong for the primitive.
pub const PDX_CRYPTO_ERR_INVALID_LENGTH: i64 = -2;
/// AEAD open: the tag did not authenticate the ciphertext + AAD.
pub const PDX_CRYPTO_ERR_AUTHENTICATION: i64 = -3;
/// Primitive-internal failure. Callers should treat as fatal.
pub const PDX_CRYPTO_ERR_PRIMITIVE: i64 = -4;
/// The output buffer is too small for the primitive's output.
pub const PDX_CRYPTO_ERR_BUFFER_TOO_SMALL: i64 = -5;

// ---------------------------------------------------------------------
// Shared helpers — usable from every primitive sub-module.
// ---------------------------------------------------------------------

/// Convert an `AeadError` into the FFI error-code contract.
///
/// Kept at the `ffi::` root so a future AEAD (XChaCha20-Poly1305,
/// AES-256-GCM) added in a v0.25-v0.32 wave shares one translator
/// rather than duplicating the match arms into its sub-module.
pub(super) fn aead_err_code(err: &AeadError) -> i64 {
    match err {
        AeadError::InvalidKeyLen { .. } | AeadError::InvalidNonceLen { .. } => {
            PDX_CRYPTO_ERR_INVALID_LENGTH
        }
        AeadError::CiphertextTooShort { .. } => PDX_CRYPTO_ERR_INVALID_LENGTH,
        AeadError::AuthenticationFailed => PDX_CRYPTO_ERR_AUTHENTICATION,
        AeadError::Primitive(_) => PDX_CRYPTO_ERR_PRIMITIVE,
    }
}

/// Convert a `KemError` into the FFI error-code contract shared with
/// the KDF and AEAD paths.
///
/// Kept at the `ffi::` root for the same reason as `aead_err_code`:
/// a second KEM (ML-KEM-512, ML-KEM-1024, HQC) added later shares
/// this translator rather than re-emitting the match.
pub(super) fn kem_err_code(err: &KemError) -> i64 {
    match err {
        KemError::InvalidParams(_) => PDX_CRYPTO_ERR_INVALID_PARAM,
        KemError::Primitive(_) => PDX_CRYPTO_ERR_PRIMITIVE,
    }
}

/// Convert an `AeadParamsC` (raw pointer form) into a typed borrow.
///
/// Currently used only by the ChaCha20-Poly1305 thunks; kept at the
/// `ffi::` root so a future AEAD sub-module (XChaCha20-Poly1305) can
/// re-use the pointer-unpack path unchanged.
///
/// # Safety
///
/// Caller must uphold the [`AeadParamsC`] pointer preconditions:
/// `key_ptr` references at least 32 bytes; `nonce_ptr` at least 12.
pub(super) unsafe fn params_from_c<'a>(
    p: *const AeadParamsC,
) -> Result<ChaCha20Poly1305Params<'a>, i64> {
    // SAFETY: caller-asserted.
    let raw = unsafe { &*p };
    if raw.key_ptr.is_null() || raw.nonce_ptr.is_null() {
        return Err(PDX_CRYPTO_ERR_INVALID_PARAM);
    }
    // SAFETY: key_ptr valid for 32 bytes, nonce_ptr valid for 12; caller-asserted.
    let key = unsafe { &*(raw.key_ptr as *const [u8; KEY_LEN]) };
    let nonce = unsafe { &*(raw.nonce_ptr as *const [u8; NONCE_LEN]) };
    let aad: &'a [u8] = if raw.aad_ptr.is_null() {
        &[]
    } else {
        // SAFETY: caller-asserted precondition on aad_ptr / aad_len pair.
        unsafe { slice::from_raw_parts(raw.aad_ptr, raw.aad_len) }
    };
    Ok(ChaCha20Poly1305Params { key, nonce, aad })
}
