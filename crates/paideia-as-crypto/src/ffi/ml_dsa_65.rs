//! C-ABI thunks over [`crate::sig::MlDsa65`] (FIPS 204) —
//! Wave υ (paideia-as υ-01 / υ-02).
//!
//! Two thunks — `mldsa65_verify` and `mldsa65_sign` — the compact
//! `no_std + alloc` sibling of the std-linked
//! `mldsa65_{sign,verify}_runtime_entry` thunks in
//! `paideia-pq-sign::ffi`. Both surfaces coexist: the pq-sign runtime
//! entries carry richer error codes (`-1/-2/-3` sentinels) and route
//! through the offline signer's expanded-key path; the thunks in this
//! file carry the compact task-spec surface (`u32` / `u64` returns,
//! no `pk_len` argument) that the kernel + satellite `.pdx` runtime
//! prefers because it fits the SysV register file without spilling.
//!
//! # `mldsa65_verify` — return surface
//!
//! ```text
//! mldsa65_verify(msg_ptr,  msg_len,
//!                sig_ptr,  sig_len,
//!                pk_ptr)                                        -> u32
//!
//!   RDI  msg_ptr    *const u8   (may be NULL iff msg_len == 0)
//!   RSI  msg_len     usize
//!   RDX  sig_ptr    *const u8   (must be non-NULL)
//!   RCX  sig_len     usize      (MUST == 3309)
//!   R8   pk_ptr     *const u8   (must be non-NULL; length is
//!                                  fixed at 1952 — no pk_len arg
//!                                  per the task spec)
//!   RAX  return:  0 = verify OK
//!                 1 = verify FAIL (bad signature, bad shape,
//!                     NULL where required, or malformed pk)
//! ```
//!
//! # `mldsa65_sign` — return surface
//!
//! ```text
//! mldsa65_sign(msg_ptr, msg_len,
//!              sk_ptr,
//!              out_sig_ptr, out_sig_max)                        -> u64
//!
//!   RDI  msg_ptr        *const u8 (may be NULL iff msg_len == 0)
//!   RSI  msg_len         usize
//!   RDX  sk_ptr         *const u8 (must be non-NULL; length is
//!                                    fixed at 32 — compact seed
//!                                    form, per the task spec)
//!   RCX  out_sig_ptr    *mut u8   (must be non-NULL and valid
//!                                    for writes of >= out_sig_max)
//!   R8   out_sig_max     usize    (MUST be >= 3309 — the fixed
//!                                    ML-DSA-65 signature length)
//!   RAX  return:  bytes written on success (always 3309), or
//!                 0 on failure (NULL where required, out_sig_max
//!                 too small, or primitive-internal error)
//! ```
//!
//! The task-spec return contract makes 0 == fail even though 0 is a
//! plausible byte-count in a general API. In practice the ML-DSA-65
//! signature is always [`MLDSA65_SIG_LEN`] bytes on success, so a
//! zero return is unambiguous. Callers on the `.pdx` side check
//! `rc != 0` for success.
//!
//! # Safety
//!
//! Every entry point is `unsafe fn` in intent even though the
//! signature is `extern "C" fn` (the C ABI does not carry `unsafe`).
//! The preconditions are enumerated per function.
//!
//! # Cross-repo alias
//!
//! There is intentionally no `#[link_name = …]` aliasing to the
//! `paideia-pq-sign::ffi::mldsa65_{sign,verify}_runtime_entry`
//! symbols — those thunks live in a DIFFERENT staticlib
//! (`paideia-pq-sign`, std-linked) with a DIFFERENT signature (`i64`
//! return, includes `pk_len`). Duplicating the object code across
//! two archives is safe because each archive links into a different
//! final binary (offline signer vs. satellite runtime), so `ld` never
//! sees both at once.

#![allow(unsafe_code)]

use core::slice;

use crate::sig::{MLDSA65_PK_LEN, MLDSA65_SEED_LEN, MLDSA65_SIG_LEN, MlDsa65, SigError};

/// `mldsa65_verify` — task-spec return contract.
///
/// - `0` = verify OK.
/// - `1` = verify FAIL (bad signature, bad shape, NULL where required).
///
/// See the module-level doc comment for the SysV register mapping.
///
/// # Safety
///
/// - `sig_ptr` must be non-NULL and valid for reads of `sig_len` bytes;
///   `sig_len` must equal [`MLDSA65_SIG_LEN`].
/// - `pk_ptr` must be non-NULL and valid for reads of [`MLDSA65_PK_LEN`]
///   bytes (no `pk_len` argument — the length is fixed at 1952).
/// - `msg_ptr` may be NULL only when `msg_len == 0`; otherwise it must
///   be valid for reads of `msg_len` bytes.
///
/// Violating any of the above is undefined behaviour on the caller
/// side; the thunk performs the NULL checks it can (all pointers +
/// `sig_len` shape) and returns `1` on any failure that does not
/// require dereferencing the offending pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mldsa65_verify(
    msg_ptr: *const u8,
    msg_len: usize,
    sig_ptr: *const u8,
    sig_len: usize,
    pk_ptr: *const u8,
) -> u32 {
    if sig_ptr.is_null() || pk_ptr.is_null() {
        return 1;
    }
    if sig_len != MLDSA65_SIG_LEN {
        return 1;
    }
    if msg_ptr.is_null() && msg_len != 0 {
        return 1;
    }

    // SAFETY: caller-asserted precondition — sig_ptr/sig_len,
    // pk_ptr/1952, and msg_ptr/msg_len are all valid borrow spans.
    let msg: &[u8] = if msg_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
    };
    let sig: &[u8] = unsafe { slice::from_raw_parts(sig_ptr, sig_len) };
    let pk: &[u8] = unsafe { slice::from_raw_parts(pk_ptr, MLDSA65_PK_LEN) };

    match MlDsa65::verify(msg, sig, pk) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => 1,
    }
}

/// `mldsa65_sign` — task-spec return contract.
///
/// Returns the number of bytes written to `out_sig_ptr` on success
/// (always [`MLDSA65_SIG_LEN`] = 3309), or `0` on any failure.
///
/// # Safety
///
/// - `sk_ptr` must be non-NULL and valid for reads of
///   [`MLDSA65_SEED_LEN`] bytes.
/// - `out_sig_ptr` must be non-NULL and valid for writes of
///   `out_sig_max` bytes; `out_sig_max` must be at least
///   [`MLDSA65_SIG_LEN`].
/// - `msg_ptr` may be NULL only when `msg_len == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mldsa65_sign(
    msg_ptr: *const u8,
    msg_len: usize,
    sk_ptr: *const u8,
    out_sig_ptr: *mut u8,
    out_sig_max: usize,
) -> u64 {
    if sk_ptr.is_null() || out_sig_ptr.is_null() {
        return 0;
    }
    if out_sig_max < MLDSA65_SIG_LEN {
        return 0;
    }
    if msg_ptr.is_null() && msg_len != 0 {
        return 0;
    }

    // SAFETY: caller-asserted precondition on sk_ptr / msg_ptr /
    // msg_len (borrow spans of their declared lengths).
    let msg: &[u8] = if msg_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
    };
    let sk: &[u8] = unsafe { slice::from_raw_parts(sk_ptr, MLDSA65_SEED_LEN) };

    match MlDsa65::sign(msg, sk) {
        Ok(sig_bytes) => {
            debug_assert_eq!(sig_bytes.len(), MLDSA65_SIG_LEN);
            // SAFETY: caller-asserted precondition — out_sig_ptr is
            // valid for writes of out_sig_max >= MLDSA65_SIG_LEN.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    sig_bytes.as_ptr(),
                    out_sig_ptr,
                    MLDSA65_SIG_LEN,
                );
            }
            MLDSA65_SIG_LEN as u64
        }
        Err(SigError::InvalidInput) | Err(SigError::Primitive) => 0,
    }
}

#[cfg(test)]
mod tests {
    //! FFI-level ML-DSA-65 tests. Round-trip sign→verify through the
    //! extern-C thunks to prove the SysV register mapping and
    //! length-checked pointer casts match the trait-level tests in
    //! `sig::ml_dsa_65`.

    use super::*;
    use crate::sig::MLDSA65_SEED_LEN;
    use alloc::vec;
    use ml_dsa::{MlDsa65 as MlDsa65Core, SigningKey, VerifyingKey};

    const TEST_SEED: [u8; MLDSA65_SEED_LEN] = [
        0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8,
        0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb0,
        0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8,
        0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0,
    ];

    fn test_pk() -> Vec<u8> {
        let sk = SigningKey::<MlDsa65Core>::from_seed(&TEST_SEED.into());
        let vk: &VerifyingKey<MlDsa65Core> = sk.as_ref();
        vk.encode().to_vec()
    }

    #[test]
    fn ffi_mldsa65_round_trip_sign_verify() {
        let msg = b"ffi round-trip";
        let mut sig = vec![0u8; MLDSA65_SIG_LEN];
        let n = unsafe {
            mldsa65_sign(
                msg.as_ptr(),
                msg.len(),
                TEST_SEED.as_ptr(),
                sig.as_mut_ptr(),
                MLDSA65_SIG_LEN,
            )
        };
        assert_eq!(n as usize, MLDSA65_SIG_LEN);

        let pk = test_pk();
        let rc = unsafe {
            mldsa65_verify(
                msg.as_ptr(),
                msg.len(),
                sig.as_ptr(),
                MLDSA65_SIG_LEN,
                pk.as_ptr(),
            )
        };
        assert_eq!(rc, 0, "genuine signature must verify OK");
    }

    #[test]
    fn ffi_mldsa65_verify_rejects_flipped_signature() {
        let msg = b"ffi flip";
        let mut sig = vec![0u8; MLDSA65_SIG_LEN];
        unsafe {
            mldsa65_sign(
                msg.as_ptr(),
                msg.len(),
                TEST_SEED.as_ptr(),
                sig.as_mut_ptr(),
                MLDSA65_SIG_LEN,
            )
        };
        sig[0] ^= 0x01;

        let pk = test_pk();
        let rc = unsafe {
            mldsa65_verify(
                msg.as_ptr(),
                msg.len(),
                sig.as_ptr(),
                MLDSA65_SIG_LEN,
                pk.as_ptr(),
            )
        };
        assert_eq!(rc, 1, "tampered signature must fail with 1");
    }

    #[test]
    fn ffi_mldsa65_verify_rejects_wrong_sig_len() {
        let pk = test_pk();
        let sig = vec![0u8; MLDSA65_SIG_LEN - 1];
        let rc = unsafe {
            mldsa65_verify(
                core::ptr::null(),
                0,
                sig.as_ptr(),
                MLDSA65_SIG_LEN - 1,
                pk.as_ptr(),
            )
        };
        assert_eq!(rc, 1);
    }

    #[test]
    fn ffi_mldsa65_verify_rejects_null_sig() {
        let pk = test_pk();
        let rc = unsafe {
            mldsa65_verify(
                core::ptr::null(),
                0,
                core::ptr::null(),
                MLDSA65_SIG_LEN,
                pk.as_ptr(),
            )
        };
        assert_eq!(rc, 1);
    }

    #[test]
    fn ffi_mldsa65_verify_rejects_null_pk() {
        let sig = vec![0u8; MLDSA65_SIG_LEN];
        let rc = unsafe {
            mldsa65_verify(
                core::ptr::null(),
                0,
                sig.as_ptr(),
                MLDSA65_SIG_LEN,
                core::ptr::null(),
            )
        };
        assert_eq!(rc, 1);
    }

    #[test]
    fn ffi_mldsa65_sign_rejects_out_buffer_too_small() {
        let mut sig = vec![0u8; MLDSA65_SIG_LEN - 1];
        let n = unsafe {
            mldsa65_sign(
                core::ptr::null(),
                0,
                TEST_SEED.as_ptr(),
                sig.as_mut_ptr(),
                MLDSA65_SIG_LEN - 1,
            )
        };
        assert_eq!(n, 0, "undersized output buffer must return 0");
    }

    #[test]
    fn ffi_mldsa65_sign_rejects_null_sk() {
        let mut sig = vec![0u8; MLDSA65_SIG_LEN];
        let n = unsafe {
            mldsa65_sign(
                core::ptr::null(),
                0,
                core::ptr::null(),
                sig.as_mut_ptr(),
                MLDSA65_SIG_LEN,
            )
        };
        assert_eq!(n, 0);
    }

    #[test]
    fn ffi_mldsa65_sign_rejects_null_output() {
        let n = unsafe {
            mldsa65_sign(
                core::ptr::null(),
                0,
                TEST_SEED.as_ptr(),
                core::ptr::null_mut(),
                MLDSA65_SIG_LEN,
            )
        };
        assert_eq!(n, 0);
    }

    #[test]
    fn ffi_mldsa65_empty_message_round_trip() {
        // FIPS 204 permits msg_len == 0 with msg_ptr NULL. Verify the
        // FFI thunk honours that shape.
        let mut sig = vec![0u8; MLDSA65_SIG_LEN];
        let n = unsafe {
            mldsa65_sign(
                core::ptr::null(),
                0,
                TEST_SEED.as_ptr(),
                sig.as_mut_ptr(),
                MLDSA65_SIG_LEN,
            )
        };
        assert_eq!(n as usize, MLDSA65_SIG_LEN);

        let pk = test_pk();
        let rc = unsafe {
            mldsa65_verify(
                core::ptr::null(),
                0,
                sig.as_ptr(),
                MLDSA65_SIG_LEN,
                pk.as_ptr(),
            )
        };
        assert_eq!(rc, 0);
    }
}
