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

use crate::aead::{Aead, AeadError, ChaCha20Poly1305, ChaCha20Poly1305Params, KEY_LEN, NONCE_LEN, TAG_LEN};
use crate::kdf::{Argon2id, Argon2idParams, Kdf, KdfError};
use crate::kem::{
    CT_LEN as KEM_CT_LEN, DK_LEN as KEM_DK_LEN, EK_LEN as KEM_EK_LEN, KemError, MlKem768,
    SEED_LEN as KEM_SEED_LEN, SS_LEN as KEM_SS_LEN,
};

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
// Argon2id
// ---------------------------------------------------------------------

/// C-ABI parameter bundle for [`paideia_crypto_argon2id_derive`].
///
/// A NULL `secret_ptr` (with any `secret_len`) or a NULL `ad_ptr`
/// (with any `ad_len`) is interpreted as "field absent" — this
/// mirrors the `Option<&[u8]>` shape on [`Argon2idParams`].
///
/// The struct is `#[repr(C)]` so its layout is stable across
/// compilation units: the `.pdx` side lays out a matching struct in
/// static data (or on the stack) and hands its address in RDI.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Argon2idParamsC {
    /// Pointer to `password` bytes.
    pub password_ptr: *const u8,
    /// Length of `password`, in bytes.
    pub password_len: usize,
    /// Pointer to `salt` bytes.
    pub salt_ptr: *const u8,
    /// Length of `salt`, in bytes.
    pub salt_len: usize,
    /// Pointer to `secret` bytes, or NULL for "no secret".
    pub secret_ptr: *const u8,
    /// Length of `secret`, in bytes. Ignored when `secret_ptr` is NULL.
    pub secret_len: usize,
    /// Pointer to associated-data bytes, or NULL for "no AD".
    pub ad_ptr: *const u8,
    /// Length of associated data, in bytes. Ignored when `ad_ptr` is NULL.
    pub ad_len: usize,
    /// Memory cost, KiB (RFC 9106 `m`).
    pub m_cost_kib: u32,
    /// Time cost / iterations (RFC 9106 `t`).
    pub t_cost: u32,
    /// Parallelism (RFC 9106 `p`).
    pub p_cost: u32,
    /// Reserved for future use; MUST be zero.
    pub _reserved: u32,
}

/// Derive Argon2id key material into `out_ptr[..out_len]`.
///
/// SysV register mapping (as emitted by `stdlib_lowering::cryptoops`):
///
/// | Register | Meaning                              |
/// |----------|--------------------------------------|
/// | RDI      | `params` — pointer to a valid `Argon2idParamsC` |
/// | RSI      | `out_ptr` — pointer to writable buffer of `out_len` bytes |
/// | RDX      | `out_len` — buffer length in bytes    |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)     |
///
/// # Safety
///
/// * `params` must point to a live, correctly-initialised
///   [`Argon2idParamsC`] whose pointer fields either satisfy the slice
///   invariants (aligned, valid for reads of the paired length) or are
///   NULL where the field is documented as optional.
/// * `out_ptr` must be non-NULL and valid for writes of `out_len`
///   bytes.
///
/// Returns `PDX_CRYPTO_OK` on success, one of the negative error
/// codes on failure. On failure the output buffer is not written.
///
/// # Panics
///
/// Never panics under valid inputs. UB (not a panic) on precondition
/// violation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_argon2id_derive(
    params: *const Argon2idParamsC,
    out_ptr: *mut u8,
    out_len: usize,
) -> i64 {
    if params.is_null() || out_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    // SAFETY: caller-asserted precondition.
    let p = unsafe { &*params };
    if p.password_ptr.is_null() || p.salt_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }

    // SAFETY: password/salt/secret/ad pointer + length pairs are the
    // caller's responsibility per the doc comment above.
    let password = unsafe { slice::from_raw_parts(p.password_ptr, p.password_len) };
    let salt = unsafe { slice::from_raw_parts(p.salt_ptr, p.salt_len) };
    let secret = if p.secret_ptr.is_null() {
        None
    } else {
        Some(unsafe { slice::from_raw_parts(p.secret_ptr, p.secret_len) })
    };
    let ad = if p.ad_ptr.is_null() {
        None
    } else {
        Some(unsafe { slice::from_raw_parts(p.ad_ptr, p.ad_len) })
    };
    let out = unsafe { slice::from_raw_parts_mut(out_ptr, out_len) };

    let params_typed = Argon2idParams {
        password,
        salt,
        secret,
        associated_data: ad,
        m_cost_kib: p.m_cost_kib,
        t_cost: p.t_cost,
        p_cost: p.p_cost,
    };

    match Argon2id::derive(&params_typed, out) {
        Ok(()) => PDX_CRYPTO_OK,
        Err(KdfError::InvalidParams(_)) => PDX_CRYPTO_ERR_INVALID_PARAM,
        Err(KdfError::InvalidOutputLen(_)) => PDX_CRYPTO_ERR_INVALID_LENGTH,
        Err(KdfError::Primitive(_)) => PDX_CRYPTO_ERR_PRIMITIVE,
    }
}

// ---------------------------------------------------------------------
// ChaCha20-Poly1305
// ---------------------------------------------------------------------

/// C-ABI parameter bundle for [`paideia_crypto_chacha20_poly1305_seal`]
/// and [`paideia_crypto_chacha20_poly1305_open`].
///
/// The struct is `#[repr(C)]` for stable layout. The `key` and `nonce`
/// pointers reference fixed-size byte arrays (32 and 12 bytes
/// respectively); passing shorter or misaligned buffers is undefined
/// behaviour.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AeadParamsC {
    /// Pointer to a `[u8; 32]` key.
    pub key_ptr: *const u8,
    /// Pointer to a `[u8; 12]` nonce.
    pub nonce_ptr: *const u8,
    /// Pointer to associated-data bytes. May be NULL when `aad_len == 0`.
    pub aad_ptr: *const u8,
    /// Length of associated data, in bytes. Zero is legal.
    pub aad_len: usize,
}

/// Seal `plaintext[..plaintext_len]` under `params`, writing
/// `ciphertext || tag` into `out_ptr[..out_cap]` and the actual
/// number of bytes written into `*written`.
///
/// SysV register mapping (6 args, all in registers):
///
/// | Register | Meaning                                     |
/// |----------|---------------------------------------------|
/// | RDI      | `params` — pointer to [`AeadParamsC`]       |
/// | RSI      | `plaintext_ptr`                             |
/// | RDX      | `plaintext_len`                             |
/// | RCX      | `out_ptr` — writable buffer                 |
/// | R8       | `out_cap`                                   |
/// | R9       | `written` — pointer to `usize` (out param)  |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)            |
///
/// Sealed output length is `plaintext_len + TAG_LEN`. Callers MUST
/// supply `out_cap >= plaintext_len + TAG_LEN`; otherwise the call
/// returns `PDX_CRYPTO_ERR_BUFFER_TOO_SMALL` and no bytes are
/// written.
///
/// # Safety
///
/// * `params` must point to a live [`AeadParamsC`] whose `key_ptr` and
///   `nonce_ptr` reference 32-byte and 12-byte arrays respectively.
/// * `plaintext_ptr` must be non-NULL when `plaintext_len > 0` and
///   valid for reads of `plaintext_len` bytes.
/// * `out_ptr` must be non-NULL and valid for writes of `out_cap`
///   bytes.
/// * `written` must be non-NULL and valid for a `usize` write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_chacha20_poly1305_seal(
    params: *const AeadParamsC,
    plaintext_ptr: *const u8,
    plaintext_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    written: *mut usize,
) -> i64 {
    if params.is_null() || out_ptr.is_null() || written.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    let needed = plaintext_len.saturating_add(TAG_LEN);
    if out_cap < needed {
        return PDX_CRYPTO_ERR_BUFFER_TOO_SMALL;
    }
    let typed_params = match unsafe { params_from_c(params) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    let plaintext = if plaintext_len == 0 {
        &[][..]
    } else if plaintext_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    } else {
        // SAFETY: caller-asserted precondition.
        unsafe { slice::from_raw_parts(plaintext_ptr, plaintext_len) }
    };

    match ChaCha20Poly1305::seal(&typed_params, plaintext) {
        Ok(sealed) => {
            // Length is always `plaintext_len + TAG_LEN` for CC20-P1305; assert.
            debug_assert_eq!(sealed.len(), needed);
            // SAFETY: `out_cap >= needed` (checked above); `sealed.len() == needed`.
            unsafe {
                core::ptr::copy_nonoverlapping(sealed.as_ptr(), out_ptr, sealed.len());
                *written = sealed.len();
            }
            PDX_CRYPTO_OK
        }
        Err(e) => aead_err_code(&e),
    }
}

/// Open (decrypt + authenticate) `sealed[..sealed_len]` under `params`
/// and write the recovered plaintext into `out_ptr[..out_cap]` with
/// the byte count landed in `*written`.
///
/// Register mapping mirrors [`paideia_crypto_chacha20_poly1305_seal`].
///
/// The recovered-plaintext length is `sealed_len - TAG_LEN` on
/// success. Callers MUST supply `out_cap >= sealed_len - TAG_LEN`
/// (and `sealed_len >= TAG_LEN`); undersized inputs return
/// `PDX_CRYPTO_ERR_INVALID_LENGTH`, undersized outputs return
/// `PDX_CRYPTO_ERR_BUFFER_TOO_SMALL`.
///
/// On tag mismatch the call returns
/// `PDX_CRYPTO_ERR_AUTHENTICATION` and does not write to `out_ptr`.
///
/// # Safety
///
/// See [`paideia_crypto_chacha20_poly1305_seal`]. Additionally,
/// `sealed_ptr` must be non-NULL and valid for reads of `sealed_len`
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_chacha20_poly1305_open(
    params: *const AeadParamsC,
    sealed_ptr: *const u8,
    sealed_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    written: *mut usize,
) -> i64 {
    if params.is_null() || out_ptr.is_null() || written.is_null() || sealed_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    if sealed_len < TAG_LEN {
        return PDX_CRYPTO_ERR_INVALID_LENGTH;
    }
    let plaintext_needed = sealed_len - TAG_LEN;
    if out_cap < plaintext_needed {
        return PDX_CRYPTO_ERR_BUFFER_TOO_SMALL;
    }
    let typed_params = match unsafe { params_from_c(params) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    // SAFETY: caller-asserted precondition; sealed_len is > 0 here.
    let sealed = unsafe { slice::from_raw_parts(sealed_ptr, sealed_len) };

    match ChaCha20Poly1305::open(&typed_params, sealed) {
        Ok(plaintext) => {
            debug_assert_eq!(plaintext.len(), plaintext_needed);
            // SAFETY: `out_cap >= plaintext.len()` (checked above).
            unsafe {
                core::ptr::copy_nonoverlapping(plaintext.as_ptr(), out_ptr, plaintext.len());
                *written = plaintext.len();
            }
            PDX_CRYPTO_OK
        }
        Err(e) => aead_err_code(&e),
    }
}

/// Convert an `AeadError` into the FFI error-code contract.
fn aead_err_code(err: &AeadError) -> i64 {
    match err {
        AeadError::InvalidKeyLen { .. } | AeadError::InvalidNonceLen { .. } => {
            PDX_CRYPTO_ERR_INVALID_LENGTH
        }
        AeadError::CiphertextTooShort { .. } => PDX_CRYPTO_ERR_INVALID_LENGTH,
        AeadError::AuthenticationFailed => PDX_CRYPTO_ERR_AUTHENTICATION,
        AeadError::Primitive(_) => PDX_CRYPTO_ERR_PRIMITIVE,
    }
}

// ---------------------------------------------------------------------
// ML-KEM-768 (FIPS 203) — paideia-as#1352
// ---------------------------------------------------------------------
//
// The three ML-KEM operations expose fixed-size byte buffers on both
// sides of the FFI:
//
//   * KeyGen: (d, z: 32 B each) -> (ek: 1184 B, dk: 2400 B)
//   * Encaps: (ek: 1184 B, m: 32 B) -> (ct: 1088 B, ss: 32 B)
//   * Decaps: (dk: 2400 B, ct: 1088 B) -> (ss: 32 B)
//
// Every buffer's length is a compile-time constant of the ML-KEM-768
// parameter set (FIPS 203 §7), so the FFI thunks accept raw pointers
// only — no length arguments, no `written` out-params. Callers on the
// `.pdx` side allocate fixed-size arrays and pass their addresses;
// undersized buffers are the caller's own UB, exactly as with any
// `#[repr(C)]` array crossing a C ABI. This matches the shape of the
// paideia-pq-sign ML-DSA thunks, whose sig / pk / sk sizes are also
// fixed at the FIPS-204 parameter level.

/// KEM shared-secret length: kept as a stable FFI-visible name so
/// callers on the `.pdx` side can spell the buffer size without
/// re-declaring it. Currently equals [`KEM_SS_LEN`] (32 bytes for
/// ML-KEM-768; ML-KEM-512/1024 also use a 32-byte shared secret).
pub const PDX_ML_KEM_768_SEED_LEN: usize = KEM_SEED_LEN;
/// Encapsulation-key length in bytes for ML-KEM-768. See
/// [`PDX_ML_KEM_768_SEED_LEN`] for the rationale on the constant.
pub const PDX_ML_KEM_768_EK_LEN: usize = KEM_EK_LEN;
/// Decapsulation-key length in bytes for ML-KEM-768.
pub const PDX_ML_KEM_768_DK_LEN: usize = KEM_DK_LEN;
/// Ciphertext length in bytes for ML-KEM-768.
pub const PDX_ML_KEM_768_CT_LEN: usize = KEM_CT_LEN;
/// Shared-secret length in bytes.
pub const PDX_ML_KEM_768_SS_LEN: usize = KEM_SS_LEN;

/// Deterministic ML-KEM-768 KeyGen (FIPS 203 §7.1, Algorithm 15).
///
/// SysV register mapping (as emitted by `stdlib_lowering::cryptoops`):
///
/// | Register | Meaning                                                          |
/// |----------|------------------------------------------------------------------|
/// | RDI      | `seed_d_ptr`  — `*const [u8; 32]`                                |
/// | RSI      | `seed_z_ptr`  — `*const [u8; 32]`                                |
/// | RDX      | `ek_out_ptr`  — `*mut [u8; 1184]` (writable)                     |
/// | RCX      | `dk_out_ptr`  — `*mut [u8; 2400]` (writable)                     |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)                                 |
///
/// # Safety
///
/// * `seed_d_ptr` and `seed_z_ptr` must be non-NULL and valid for
///   reads of 32 bytes each.
/// * `ek_out_ptr` and `dk_out_ptr` must be non-NULL and valid for
///   writes of 1184 and 2400 bytes respectively.
///
/// Returns `PDX_CRYPTO_OK` on success. On failure the output buffers
/// are not written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_ml_kem_768_keygen(
    seed_d_ptr: *const u8,
    seed_z_ptr: *const u8,
    ek_out_ptr: *mut u8,
    dk_out_ptr: *mut u8,
) -> i64 {
    if seed_d_ptr.is_null()
        || seed_z_ptr.is_null()
        || ek_out_ptr.is_null()
        || dk_out_ptr.is_null()
    {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    // SAFETY: caller-asserted precondition — pointers reference
    // 32-byte arrays at rest.
    let seed_d: &[u8; KEM_SEED_LEN] = unsafe { &*(seed_d_ptr as *const [u8; KEM_SEED_LEN]) };
    let seed_z: &[u8; KEM_SEED_LEN] = unsafe { &*(seed_z_ptr as *const [u8; KEM_SEED_LEN]) };

    match MlKem768::keygen(seed_d, seed_z) {
        Ok((ek, dk)) => {
            // SAFETY: caller-asserted precondition — the two output
            // buffers are valid for writes of their declared lengths.
            unsafe {
                core::ptr::copy_nonoverlapping(ek.as_ptr(), ek_out_ptr, KEM_EK_LEN);
                core::ptr::copy_nonoverlapping(dk.as_ptr(), dk_out_ptr, KEM_DK_LEN);
            }
            PDX_CRYPTO_OK
        }
        Err(e) => kem_err_code(&e),
    }
}

/// Deterministic ML-KEM-768 Encaps (FIPS 203 §6.2, Algorithm 16).
///
/// SysV register mapping:
///
/// | Register | Meaning                                                          |
/// |----------|------------------------------------------------------------------|
/// | RDI      | `ek_ptr`      — `*const [u8; 1184]`                              |
/// | RSI      | `seed_m_ptr`  — `*const [u8; 32]`                                |
/// | RDX      | `ct_out_ptr`  — `*mut [u8; 1088]` (writable)                     |
/// | RCX      | `ss_out_ptr`  — `*mut [u8; 32]` (writable)                       |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)                                 |
///
/// # Safety
///
/// Every pointer must be non-NULL and valid for reads / writes of its
/// declared length.
///
/// Returns `PDX_CRYPTO_OK` on success; on failure the output buffers
/// are not written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_ml_kem_768_encaps(
    ek_ptr: *const u8,
    seed_m_ptr: *const u8,
    ct_out_ptr: *mut u8,
    ss_out_ptr: *mut u8,
) -> i64 {
    if ek_ptr.is_null() || seed_m_ptr.is_null() || ct_out_ptr.is_null() || ss_out_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    // SAFETY: caller-asserted precondition.
    let ek: &[u8; KEM_EK_LEN] = unsafe { &*(ek_ptr as *const [u8; KEM_EK_LEN]) };
    let seed_m: &[u8; KEM_SEED_LEN] = unsafe { &*(seed_m_ptr as *const [u8; KEM_SEED_LEN]) };

    match MlKem768::encaps(ek, seed_m) {
        Ok((ct, ss)) => {
            // SAFETY: caller-asserted precondition.
            unsafe {
                core::ptr::copy_nonoverlapping(ct.as_ptr(), ct_out_ptr, KEM_CT_LEN);
                core::ptr::copy_nonoverlapping(ss.as_ptr(), ss_out_ptr, KEM_SS_LEN);
            }
            PDX_CRYPTO_OK
        }
        Err(e) => kem_err_code(&e),
    }
}

/// ML-KEM-768 Decaps (FIPS 203 §6.3, Algorithm 17).
///
/// SysV register mapping (5 args, all in registers):
///
/// | Register | Meaning                                                          |
/// |----------|------------------------------------------------------------------|
/// | RDI      | `dk_ptr`      — `*const [u8; 2400]`                              |
/// | RSI      | `ct_ptr`      — `*const [u8; 1088]`                              |
/// | RDX      | `ss_out_ptr`  — `*mut [u8; 32]` (writable)                       |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)                                 |
///
/// # Implicit rejection
///
/// A tampered ciphertext decapsulates to a pseudo-random shared
/// secret rather than an error (FIPS 203 §6.3). The thunk therefore
/// returns `PDX_CRYPTO_OK` on any well-formed input; callers detect
/// mismatch by wrapping the shared secret in an authenticated
/// transform. See the `MlKem768::decaps` docs.
///
/// # Safety
///
/// Every pointer must be non-NULL and valid for reads / writes of
/// its declared length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_ml_kem_768_decaps(
    dk_ptr: *const u8,
    ct_ptr: *const u8,
    ss_out_ptr: *mut u8,
) -> i64 {
    if dk_ptr.is_null() || ct_ptr.is_null() || ss_out_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    // SAFETY: caller-asserted precondition.
    let dk: &[u8; KEM_DK_LEN] = unsafe { &*(dk_ptr as *const [u8; KEM_DK_LEN]) };
    let ct: &[u8; KEM_CT_LEN] = unsafe { &*(ct_ptr as *const [u8; KEM_CT_LEN]) };

    match MlKem768::decaps(dk, ct) {
        Ok(ss) => {
            // SAFETY: caller-asserted precondition.
            unsafe {
                core::ptr::copy_nonoverlapping(ss.as_ptr(), ss_out_ptr, KEM_SS_LEN);
            }
            PDX_CRYPTO_OK
        }
        Err(e) => kem_err_code(&e),
    }
}

/// Convert a `KemError` into the FFI error-code contract shared with
/// the KDF and AEAD paths.
fn kem_err_code(err: &KemError) -> i64 {
    match err {
        KemError::InvalidParams(_) => PDX_CRYPTO_ERR_INVALID_PARAM,
        KemError::Primitive(_) => PDX_CRYPTO_ERR_PRIMITIVE,
    }
}

/// Convert an `AeadParamsC` (raw pointer form) into a typed borrow.
///
/// # Safety
///
/// Caller must uphold the [`AeadParamsC`] pointer preconditions:
/// `key_ptr` references at least 32 bytes; `nonce_ptr` at least 12.
unsafe fn params_from_c<'a>(p: *const AeadParamsC) -> Result<ChaCha20Poly1305Params<'a>, i64> {
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

#[cfg(test)]
mod tests {
    //! FFI-level tests. These re-derive / re-seal the canonical RFC
    //! vectors through the extern-C thunks — verifying that the C
    //! shim reproduces the same bytes as the trait-level tests in
    //! `kdf::argon2id` and `aead::chacha20_poly1305`, and therefore
    //! that the extern-C surface is a faithful projection of the
    //! trait API. Any regression on either the vector or the FFI
    //! layout will fail here.

    use super::*;
    use crate::aead::{
        RFC_8439_SEC_2_8_2_AAD, RFC_8439_SEC_2_8_2_CIPHERTEXT, RFC_8439_SEC_2_8_2_KEY,
        RFC_8439_SEC_2_8_2_NONCE, RFC_8439_SEC_2_8_2_PLAINTEXT, RFC_8439_SEC_2_8_2_TAG,
    };
    use crate::kdf::RFC_9106_ARGON2ID_TAG;

    // ---------- Argon2id: RFC 9106 §5.3 vector via the FFI thunk ----------

    #[test]
    fn ffi_argon2id_derive_reproduces_rfc_9106_section_5_3() {
        let password = [0x01u8; 32];
        let salt = [0x02u8; 16];
        let secret = [0x03u8; 8];
        let ad = [0x04u8; 12];

        let p = Argon2idParamsC {
            password_ptr: password.as_ptr(),
            password_len: password.len(),
            salt_ptr: salt.as_ptr(),
            salt_len: salt.len(),
            secret_ptr: secret.as_ptr(),
            secret_len: secret.len(),
            ad_ptr: ad.as_ptr(),
            ad_len: ad.len(),
            m_cost_kib: 32,
            t_cost: 3,
            p_cost: 4,
            _reserved: 0,
        };

        let mut out = [0u8; 32];
        let rc = unsafe {
            paideia_crypto_argon2id_derive(&p as *const _, out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(out, RFC_9106_ARGON2ID_TAG);
    }

    #[test]
    fn ffi_argon2id_null_params_returns_invalid_param() {
        let mut out = [0u8; 32];
        let rc = unsafe {
            paideia_crypto_argon2id_derive(core::ptr::null(), out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_argon2id_short_output_returns_invalid_length() {
        // T < 4 rejected by the trait; the FFI must forward the code.
        let password = [0u8; 8];
        let salt = [0u8; 16];
        let p = Argon2idParamsC {
            password_ptr: password.as_ptr(),
            password_len: password.len(),
            salt_ptr: salt.as_ptr(),
            salt_len: salt.len(),
            secret_ptr: core::ptr::null(),
            secret_len: 0,
            ad_ptr: core::ptr::null(),
            ad_len: 0,
            m_cost_kib: 1 << 16,
            t_cost: 3,
            p_cost: 4,
            _reserved: 0,
        };
        let mut out = [0u8; 3];
        let rc = unsafe {
            paideia_crypto_argon2id_derive(&p as *const _, out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_LENGTH);
    }

    // ---------- ChaCha20-Poly1305: RFC 8439 §2.8.2 vector via the FFI thunk ----------

    #[test]
    fn ffi_chacha20_poly1305_seal_reproduces_rfc_8439_section_2_8_2() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let aad = RFC_8439_SEC_2_8_2_AAD;

        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: aad.as_ptr(),
            aad_len: aad.len(),
        };

        let plaintext = RFC_8439_SEC_2_8_2_PLAINTEXT;
        let mut out = vec![0u8; plaintext.len() + TAG_LEN];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_seal(
                &params as *const _,
                plaintext.as_ptr(),
                plaintext.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(written, plaintext.len() + TAG_LEN);

        // Byte-exact match against the RFC ciphertext || tag.
        let (ct, tag) = out[..written].split_at(plaintext.len());
        assert_eq!(ct, &RFC_8439_SEC_2_8_2_CIPHERTEXT[..]);
        assert_eq!(tag, &RFC_8439_SEC_2_8_2_TAG[..]);
    }

    #[test]
    fn ffi_chacha20_poly1305_open_round_trips_rfc_8439_section_2_8_2() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let aad = RFC_8439_SEC_2_8_2_AAD;

        // Reconstruct sealed = ciphertext || tag.
        let mut sealed = Vec::with_capacity(RFC_8439_SEC_2_8_2_CIPHERTEXT.len() + TAG_LEN);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);

        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: aad.as_ptr(),
            aad_len: aad.len(),
        };

        let mut out = vec![0u8; RFC_8439_SEC_2_8_2_PLAINTEXT.len()];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_open(
                &params as *const _,
                sealed.as_ptr(),
                sealed.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(written, RFC_8439_SEC_2_8_2_PLAINTEXT.len());
        assert_eq!(&out[..written], RFC_8439_SEC_2_8_2_PLAINTEXT);
    }

    #[test]
    fn ffi_chacha20_poly1305_open_tag_mismatch_returns_authentication_error() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let aad = RFC_8439_SEC_2_8_2_AAD;

        let mut sealed = Vec::with_capacity(RFC_8439_SEC_2_8_2_CIPHERTEXT.len() + TAG_LEN);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);
        // Flip one bit in the tag.
        *sealed.last_mut().unwrap() ^= 0x01;

        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: aad.as_ptr(),
            aad_len: aad.len(),
        };

        let mut out = vec![0u8; RFC_8439_SEC_2_8_2_PLAINTEXT.len()];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_open(
                &params as *const _,
                sealed.as_ptr(),
                sealed.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_AUTHENTICATION);
    }

    #[test]
    fn ffi_chacha20_poly1305_seal_undersized_output_returns_buffer_too_small() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: core::ptr::null(),
            aad_len: 0,
        };
        let plaintext = [0u8; 16];
        // out_cap < plaintext_len + TAG_LEN (16) should be rejected.
        let mut out = [0u8; 8];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_seal(
                &params as *const _,
                plaintext.as_ptr(),
                plaintext.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_BUFFER_TOO_SMALL);
        assert_eq!(written, 0);
    }

    // ---------- ML-KEM-768: NIST ACVP vectors via the FFI thunks ----------

    /// Same NIST ACVP tcId-26 KeyGen vector as the trait-level test in
    /// `kem::ml_kem_768`, reproduced through the extern-C thunk. Any
    /// mismatch on the SysV register mapping or the length-checked
    /// pointer casts would fail here even if the trait test passed.
    #[test]
    fn ffi_ml_kem_768_keygen_reproduces_acvp_tc26() {
        use crate::kem::{ACVP_KG_D, ACVP_KG_DK, ACVP_KG_EK, ACVP_KG_Z};
        let mut ek = vec![0u8; PDX_ML_KEM_768_EK_LEN];
        let mut dk = vec![0u8; PDX_ML_KEM_768_DK_LEN];
        let rc = unsafe {
            paideia_crypto_ml_kem_768_keygen(
                ACVP_KG_D.as_ptr(),
                ACVP_KG_Z.as_ptr(),
                ek.as_mut_ptr(),
                dk.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(ek.as_slice(), &ACVP_KG_EK[..]);
        assert_eq!(dk.as_slice(), &ACVP_KG_DK[..]);
    }

    /// Same NIST ACVP tcId-26 Encaps vector reproduced through the
    /// extern-C thunk.
    #[test]
    fn ffi_ml_kem_768_encaps_reproduces_acvp_tc26() {
        use crate::kem::{ACVP_EN_C, ACVP_EN_EK, ACVP_EN_K, ACVP_EN_M};
        let mut ct = vec![0u8; PDX_ML_KEM_768_CT_LEN];
        let mut ss = vec![0u8; PDX_ML_KEM_768_SS_LEN];
        let rc = unsafe {
            paideia_crypto_ml_kem_768_encaps(
                ACVP_EN_EK.as_ptr(),
                ACVP_EN_M.as_ptr(),
                ct.as_mut_ptr(),
                ss.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(ct.as_slice(), &ACVP_EN_C[..]);
        assert_eq!(ss.as_slice(), &ACVP_EN_K[..]);
    }

    /// Same NIST ACVP tcId-88 "no modification" Decaps vector reproduced
    /// through the extern-C thunk.
    #[test]
    fn ffi_ml_kem_768_decaps_reproduces_acvp_tc88() {
        use crate::kem::{ACVP_DE_C, ACVP_DE_DK, ACVP_DE_K};
        let mut ss = vec![0u8; PDX_ML_KEM_768_SS_LEN];
        let rc = unsafe {
            paideia_crypto_ml_kem_768_decaps(
                ACVP_DE_DK.as_ptr(),
                ACVP_DE_C.as_ptr(),
                ss.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(ss.as_slice(), &ACVP_DE_K[..]);
    }

    /// NULL input to any KEM thunk must fail with the standard
    /// `PDX_CRYPTO_ERR_INVALID_PARAM` code and MUST NOT touch the
    /// output buffer. Pinning one representative NULL per thunk is
    /// sufficient to exercise the null-guard path — the internal
    /// arg checks are all uniform.
    #[test]
    fn ffi_ml_kem_768_keygen_rejects_null_seed() {
        let mut ek = vec![0u8; PDX_ML_KEM_768_EK_LEN];
        let mut dk = vec![0u8; PDX_ML_KEM_768_DK_LEN];
        let z = [0u8; PDX_ML_KEM_768_SEED_LEN];
        let rc = unsafe {
            paideia_crypto_ml_kem_768_keygen(
                core::ptr::null(),
                z.as_ptr(),
                ek.as_mut_ptr(),
                dk.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_ml_kem_768_encaps_rejects_null_output() {
        let ek = [0u8; PDX_ML_KEM_768_EK_LEN];
        let m = [0u8; PDX_ML_KEM_768_SEED_LEN];
        let mut ss = vec![0u8; PDX_ML_KEM_768_SS_LEN];
        let rc = unsafe {
            paideia_crypto_ml_kem_768_encaps(
                ek.as_ptr(),
                m.as_ptr(),
                core::ptr::null_mut(),
                ss.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_ml_kem_768_decaps_rejects_null_dk() {
        let ct = [0u8; PDX_ML_KEM_768_CT_LEN];
        let mut ss = vec![0u8; PDX_ML_KEM_768_SS_LEN];
        let rc = unsafe {
            paideia_crypto_ml_kem_768_decaps(
                core::ptr::null(),
                ct.as_ptr(),
                ss.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_chacha20_poly1305_open_short_sealed_returns_invalid_length() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: core::ptr::null(),
            aad_len: 0,
        };
        // sealed shorter than TAG_LEN (16) is malformed.
        let sealed = [0u8; 8];
        let mut out = [0u8; 8];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_open(
                &params as *const _,
                sealed.as_ptr(),
                sealed.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_LENGTH);
    }
}
