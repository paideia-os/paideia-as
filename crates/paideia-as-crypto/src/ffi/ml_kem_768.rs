//! C-ABI thunks over [`crate::kem::MlKem768`] (FIPS 203) —
//! paideia-as#1352.
//!
//! Three thunks (`paideia_crypto_ml_kem_768_{keygen, encaps, decaps}`)
//! plus the FFI-stable buffer-length constants (`PDX_ML_KEM_768_*`)
//! and the ACVP known-answer vector tests.
//!
//! Split out of `ffi/mod.rs` per paideia-as#1354 so parallel authoring
//! of the v0.25-v0.32 crypto waves never collides inside the shared
//! module. Behaviour is byte-identical to the pre-split thunks; only
//! the file location changed. Shared error codes and the KEM →
//! FFI-code translator (`kem_err_code`) live in `super`.
//!
//! The three ML-KEM operations expose fixed-size byte buffers on both
//! sides of the FFI:
//!
//!   * KeyGen: (d, z: 32 B each) -> (ek: 1184 B, dk: 2400 B)
//!   * Encaps: (ek: 1184 B, m: 32 B) -> (ct: 1088 B, ss: 32 B)
//!   * Decaps: (dk: 2400 B, ct: 1088 B) -> (ss: 32 B)
//!
//! Every buffer's length is a compile-time constant of the ML-KEM-768
//! parameter set (FIPS 203 §7), so the FFI thunks accept raw pointers
//! only — no length arguments, no `written` out-params. Callers on the
//! `.pdx` side allocate fixed-size arrays and pass their addresses;
//! undersized buffers are the caller's own UB, exactly as with any
//! `#[repr(C)]` array crossing a C ABI. This matches the shape of the
//! paideia-pq-sign ML-DSA thunks, whose sig / pk / sk sizes are also
//! fixed at the FIPS-204 parameter level.

// The crate-root `#![deny(unsafe_code)]` lint is lifted here so this
// FFI shim can cast caller-supplied raw pointers to fixed-size array
// references.
#![allow(unsafe_code)]

use crate::kem::{
    CT_LEN as KEM_CT_LEN, DK_LEN as KEM_DK_LEN, EK_LEN as KEM_EK_LEN, MlKem768,
    SEED_LEN as KEM_SEED_LEN, SS_LEN as KEM_SS_LEN,
};

use super::{PDX_CRYPTO_ERR_INVALID_PARAM, PDX_CRYPTO_OK, kem_err_code};

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

#[cfg(test)]
mod tests {
    //! FFI-level ML-KEM-768 tests. Re-run the NIST ACVP known-answer
    //! vectors through the extern-C thunks to prove the SysV register
    //! mapping and length-checked pointer casts match the trait-level
    //! tests in `kem::ml_kem_768`.

    use super::*;

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
}
