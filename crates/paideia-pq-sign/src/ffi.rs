//! C-ABI thunk over [`crate::mldsa::MlDsa65Marker`], the ML-DSA-65
//! (FIPS 204) signer.
//!
//! ## Why a separate ffi module
//!
//! `MlDsa65Marker::sign` (the [`crate::Signer`] impl) takes a `&[u8]`
//! secret key and message and returns a heap-allocated `Signature` —
//! a shape that cannot cross a `.pdx` call site, which marshals
//! arguments through the SysV integer-register convention (RDI/RSI/
//! RDX/RCX/R8/R9) and has no notion of `Vec<u8>`. This module flattens
//! that API into a single `pub extern "C" fn` whose signature/output
//! is a caller-allocated buffer plus a status code.
//!
//! Consumers on the `.pdx` side reach this via the
//! `stdlib_lowering::mldsaops` recipe (paideia-as#1330), which emits
//! `call mldsa65_sign_runtime_entry` after marshalling the SysV
//! argument registers per the table below. The paideia-as elaborator
//! does not need to know about this module directly; it only emits
//! the CALL relocation. Downstream consumers (paideia-os, host
//! tooling) satisfy the symbol at link time by depending on this
//! crate.
//!
//! ## Calling convention (choice A: caller-allocated output buffer)
//!
//! A 3309-byte ML-DSA-65 signature does not fit in a register, so the
//! caller passes a pointer to a pre-allocated `MLDSA65_SIG_LEN`-byte
//! buffer and the thunk writes into it, returning a status code in
//! RAX — the same shape `paideia-as-crypto::ffi` already uses for
//! Argon2id / ChaCha20-Poly1305, rather than an sret record-return
//! convention the encoder does not yet support (see
//! `stdlib_lowering::cpuidops` for why record-return is deferred).
//!
//! | Register | Meaning                                          |
//! |----------|---------------------------------------------------|
//! | RDI      | `seed_ptr` — `*const u8`, 32-byte ML-DSA-65 seed  |
//! | RSI      | `msg_ptr` — `*const u8`                           |
//! | RDX      | `msg_len` — `usize`                               |
//! | RCX      | `sig_out_ptr` — `*mut u8`, >= 3309 bytes           |
//! | **RAX**  | return code (0 = OK, negative = error)            |
//!
//! ## Safety
//!
//! This is the only module in the crate that touches raw pointers.
//! The crate-level `#![deny(unsafe_code)]` guards every other module;
//! `#![allow(unsafe_code)]` below lifts it here for the C-shim path
//! only.

#![allow(unsafe_code)]

use core::slice;

use crate::mldsa::MlDsa65Marker;
use crate::{MLDSA65_SIG_LEN, MLDSA65_SK_LEN, Signer};

/// Success return code.
pub const PDX_MLDSA_OK: i64 = 0;
/// A required pointer was NULL.
pub const PDX_MLDSA_ERR_INVALID_PARAM: i64 = -1;
/// The produced signature was not `MLDSA65_SIG_LEN` bytes — should be
/// unreachable given the underlying library's fixed-size encoding,
/// guarded defensively rather than trusted blindly across an FFI
/// boundary.
pub const PDX_MLDSA_ERR_LENGTH: i64 = -2;

/// Sign `msg` with the ML-DSA-65 seed at `seed_ptr`, writing the
/// encoded 3309-byte signature into `sig_out_ptr`.
///
/// SysV register mapping (as emitted by `stdlib_lowering::mldsaops`):
/// see the module-level doc table.
///
/// # Safety
///
/// * `seed_ptr` must be non-NULL and valid for reads of
///   `MLDSA65_SK_LEN` (32) bytes.
/// * `msg_ptr` must be non-NULL (unless `msg_len == 0`) and valid for
///   reads of `msg_len` bytes.
/// * `sig_out_ptr` must be non-NULL and valid for writes of
///   `MLDSA65_SIG_LEN` (3309) bytes.
///
/// Returns `PDX_MLDSA_OK` on success. On any error the output buffer
/// is not written.
///
/// # Panics
///
/// Never panics under valid inputs. UB (not a panic) on precondition
/// violation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mldsa65_sign_runtime_entry(
    seed_ptr: *const u8,
    msg_ptr: *const u8,
    msg_len: usize,
    sig_out_ptr: *mut u8,
) -> i64 {
    if seed_ptr.is_null() || sig_out_ptr.is_null() || (msg_ptr.is_null() && msg_len != 0) {
        return PDX_MLDSA_ERR_INVALID_PARAM;
    }

    // SAFETY: `seed_ptr` non-null, caller-asserted valid for
    // `MLDSA65_SK_LEN` reads per the precondition above.
    let seed = unsafe { slice::from_raw_parts(seed_ptr, MLDSA65_SK_LEN) }.to_vec();
    // `slice::from_raw_parts` requires a non-null, well-aligned
    // pointer even for a zero-length slice — a null `msg_ptr` with
    // `msg_len == 0` (the empty-message case) must not reach it.
    let msg: &[u8] = if msg_len == 0 {
        &[]
    } else {
        // SAFETY: `msg_ptr` is non-null (checked above) and
        // caller-asserted valid for `msg_len` reads.
        unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
    };

    let sig = MlDsa65Marker::sign(&crate::mldsa::SecretKey(seed), msg);

    if sig.0.len() != MLDSA65_SIG_LEN {
        return PDX_MLDSA_ERR_LENGTH;
    }

    // SAFETY: `sig_out_ptr` non-null, caller-asserted valid for
    // `MLDSA65_SIG_LEN` writes per the precondition above.
    let out = unsafe { slice::from_raw_parts_mut(sig_out_ptr, MLDSA65_SIG_LEN) };
    out.copy_from_slice(&sig.0);

    PDX_MLDSA_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mldsa::MlDsa65Marker;
    use rand_core::OsRng;

    #[test]
    fn ffi_sign_round_trips_through_verify() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg = b"paideia-as#1330 ffi round-trip";
        let mut sig_out = [0u8; MLDSA65_SIG_LEN];

        let rc = unsafe {
            mldsa65_sign_runtime_entry(
                sk.0.as_ptr(),
                msg.as_ptr(),
                msg.len(),
                sig_out.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_MLDSA_OK);

        assert!(MlDsa65Marker::verify(
            &pk,
            msg,
            &crate::mldsa::Signature(sig_out.to_vec())
        ));
    }

    #[test]
    fn ffi_sign_matches_direct_signer_call() {
        let (_, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg = b"determinism cross-check";
        let mut sig_out = [0u8; MLDSA65_SIG_LEN];

        let rc = unsafe {
            mldsa65_sign_runtime_entry(
                sk.0.as_ptr(),
                msg.as_ptr(),
                msg.len(),
                sig_out.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_MLDSA_OK);

        let direct = MlDsa65Marker::sign(&sk, msg);
        assert_eq!(direct.0, sig_out.to_vec());
    }

    #[test]
    fn ffi_sign_rejects_null_seed() {
        let msg = b"x";
        let mut sig_out = [0u8; MLDSA65_SIG_LEN];
        let rc = unsafe {
            mldsa65_sign_runtime_entry(
                core::ptr::null(),
                msg.as_ptr(),
                msg.len(),
                sig_out.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_MLDSA_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_sign_rejects_null_sig_out() {
        let (_, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg = b"x";
        let rc = unsafe {
            mldsa65_sign_runtime_entry(sk.0.as_ptr(), msg.as_ptr(), msg.len(), core::ptr::null_mut())
        };
        assert_eq!(rc, PDX_MLDSA_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_sign_accepts_empty_message_with_null_msg_ptr() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let mut sig_out = [0u8; MLDSA65_SIG_LEN];
        let rc = unsafe {
            mldsa65_sign_runtime_entry(sk.0.as_ptr(), core::ptr::null(), 0, sig_out.as_mut_ptr())
        };
        assert_eq!(rc, PDX_MLDSA_OK);
        assert!(MlDsa65Marker::verify(
            &pk,
            b"",
            &crate::mldsa::Signature(sig_out.to_vec())
        ));
    }
}
