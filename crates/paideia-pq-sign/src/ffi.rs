//! C-ABI thunks over [`crate::mldsa::MlDsa65Marker`], the ML-DSA-65
//! (FIPS 204) signer.
//!
//! ## Why a separate ffi module
//!
//! `MlDsa65Marker::sign` / `::verify` (the [`crate::Signer`] impl) take
//! `Vec<u8>`-backed key/signature wrappers and — for sign — return a
//! heap-allocated `Signature`; both are shapes that cannot cross a
//! `.pdx` call site, which marshals arguments through the SysV
//! integer-register convention (RDI/RSI/RDX/RCX/R8/R9) and has no
//! notion of `Vec<u8>`. This module flattens those APIs into plain
//! `pub extern "C" fn` symbols whose signature/output is a
//! caller-allocated buffer (sign) or a boolean status code (verify).
//!
//! Consumers on the `.pdx` side reach these via the
//! `stdlib_lowering::mldsaops` recipe (paideia-as#1330 for sign;
//! paideia-as#1347 for verify), which emits `call
//! mldsa65_sign_runtime_entry` / `call mldsa65_verify_runtime_entry`
//! after marshalling the SysV argument registers per the tables below.
//! The paideia-as elaborator does not need to know about this module
//! directly; it only emits the CALL relocation. Downstream consumers
//! (paideia-os, host tooling) satisfy the symbols at link time by
//! depending on this crate.
//!
//! ## Calling convention — sign (choice A: caller-allocated output buffer)
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
//! ## Calling convention — verify (paideia-as#1347)
//!
//! Verify has no output buffer; the six SysV integer registers are all
//! used for `(msg, sig, pubkey)` `(ptr, len)` pairs, and the boolean
//! result is projected onto the same negative-error / zero-success
//! shape sign uses so both entry points share a diagnostic surface.
//!
//! | Register | Meaning                                                |
//! |----------|--------------------------------------------------------|
//! | RDI      | `msg_ptr`     — `*const u8`                            |
//! | RSI      | `msg_len`     — `usize`                                |
//! | RDX      | `sig_ptr`     — `*const u8`, `MLDSA65_SIG_LEN` bytes   |
//! | RCX      | `sig_len`     — `usize` (MUST equal 3309)              |
//! | R8       | `pubkey_ptr`  — `*const u8`, `MLDSA65_PK_LEN` bytes    |
//! | R9       | `pubkey_len`  — `usize` (MUST equal 1952)              |
//! | **RAX**  | return code (0 = valid, negative = invalid or error)   |
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
use crate::{MLDSA65_PK_LEN, MLDSA65_SIG_LEN, MLDSA65_SK_LEN, Signer};

/// Success return code (sign: buffer written; verify: signature valid).
pub const PDX_MLDSA_OK: i64 = 0;
/// A required pointer was NULL.
pub const PDX_MLDSA_ERR_INVALID_PARAM: i64 = -1;
/// A length mismatch: sign — the produced signature was not
/// `MLDSA65_SIG_LEN` bytes (should be unreachable given the underlying
/// library's fixed-size encoding, guarded defensively rather than
/// trusted blindly across an FFI boundary); verify — the caller-passed
/// `pubkey_len` was not `MLDSA65_PK_LEN` (1952) or `sig_len` was not
/// `MLDSA65_SIG_LEN` (3309).
pub const PDX_MLDSA_ERR_LENGTH: i64 = -2;
/// Verify only: the signature did not authenticate under the given
/// public key + message. Shares the sentinel with
/// `paideia-as-crypto::ffi::PDX_CRYPTO_ERR_AUTHENTICATION` so callers
/// can share a diagnostic handler across AEAD open and PQ verify.
pub const PDX_MLDSA_ERR_AUTHENTICATION: i64 = -3;

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

/// Verify an ML-DSA-65 (FIPS 204 §7.3) signature `sig[..sig_len]` over
/// `msg[..msg_len]` under `pubkey[..pubkey_len]`.
///
/// SysV register mapping (as emitted by `stdlib_lowering::mldsaops`):
/// see the module-level doc table.
///
/// Returns `PDX_MLDSA_OK` (0) when the signature verifies,
/// `PDX_MLDSA_ERR_AUTHENTICATION` (-3) when it does not,
/// `PDX_MLDSA_ERR_INVALID_PARAM` (-1) when a required pointer is NULL
/// (msg_ptr may be NULL iff msg_len == 0), or
/// `PDX_MLDSA_ERR_LENGTH` (-2) when `pubkey_len != MLDSA65_PK_LEN` or
/// `sig_len != MLDSA65_SIG_LEN`.
///
/// Consumers on the `.pdx` side (libpdx-volume#16 pdxb_verify_superblock,
/// libpdx-volume pdxb_verify_inode_tail, etc.) MUST test `rc == 0` for
/// a valid signature; any negative value means "not verified" and the
/// signature is unusable regardless of whether the failure was a bad
/// parameter or a genuine authentication mismatch.
///
/// # Safety
///
/// * `msg_ptr` must be non-NULL when `msg_len > 0` and valid for reads
///   of `msg_len` bytes; may be NULL when `msg_len == 0`.
/// * `sig_ptr` must be non-NULL and valid for reads of `sig_len` bytes.
/// * `pubkey_ptr` must be non-NULL and valid for reads of `pubkey_len`
///   bytes.
///
/// # Panics
///
/// Never panics under valid inputs. UB (not a panic) on precondition
/// violation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mldsa65_verify_runtime_entry(
    msg_ptr: *const u8,
    msg_len: usize,
    sig_ptr: *const u8,
    sig_len: usize,
    pubkey_ptr: *const u8,
    pubkey_len: usize,
) -> i64 {
    if sig_ptr.is_null() || pubkey_ptr.is_null() || (msg_ptr.is_null() && msg_len != 0) {
        return PDX_MLDSA_ERR_INVALID_PARAM;
    }
    // Length gate is upstream of the borrow so we never construct a
    // slice under a spurious length: `MlDsa65Marker::verify` itself
    // returns `false` on mis-sized pubkey/sig, but that would collapse
    // "bad length" into "bad signature" for the caller. Splitting the
    // sentinels here gives libpdx-volume#16 the diagnostic separation
    // it needs (a malformed superblock header is a repair path; a
    // legitimate authentication failure is an eviction path).
    if pubkey_len != MLDSA65_PK_LEN || sig_len != MLDSA65_SIG_LEN {
        return PDX_MLDSA_ERR_LENGTH;
    }

    // `slice::from_raw_parts` requires a non-null well-aligned pointer
    // even for a zero-length slice — see the sign path above for the
    // same guard.
    let msg: &[u8] = if msg_len == 0 {
        &[]
    } else {
        // SAFETY: `msg_ptr` non-null (checked above), caller-asserted
        // valid for `msg_len` reads.
        unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
    };
    // SAFETY: `sig_ptr` / `pubkey_ptr` non-null (checked above),
    // caller-asserted valid for the exact fixed-size reads.
    let sig_bytes = unsafe { slice::from_raw_parts(sig_ptr, MLDSA65_SIG_LEN) }.to_vec();
    let pk_bytes = unsafe { slice::from_raw_parts(pubkey_ptr, MLDSA65_PK_LEN) }.to_vec();

    let pk = crate::mldsa::PublicKey(pk_bytes);
    let sig = crate::mldsa::Signature(sig_bytes);

    if MlDsa65Marker::verify(&pk, msg, &sig) {
        PDX_MLDSA_OK
    } else {
        PDX_MLDSA_ERR_AUTHENTICATION
    }
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

    // ---------- paideia-as#1347 — mldsa65_verify_runtime_entry ----------

    /// Sign via the sign thunk, then verify via the verify thunk with
    /// the matching pk / msg. This is the primary happy-path pin the
    /// verify wrapper landed to serve libpdx-volume#16
    /// pdxb_verify_superblock — a real pdx caller will marshal exactly
    /// this shape: fixed-size sig/pk buffers plus a variable message.
    #[test]
    fn ffi_verify_accepts_signature_from_sign_thunk() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg = b"paideia-as#1347 ffi verify round-trip";

        let mut sig_out = [0u8; MLDSA65_SIG_LEN];
        let sign_rc = unsafe {
            mldsa65_sign_runtime_entry(
                sk.0.as_ptr(),
                msg.as_ptr(),
                msg.len(),
                sig_out.as_mut_ptr(),
            )
        };
        assert_eq!(sign_rc, PDX_MLDSA_OK);

        let verify_rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg.as_ptr(),
                msg.len(),
                sig_out.as_ptr(),
                sig_out.len(),
                pk.0.as_ptr(),
                pk.0.len(),
            )
        };
        assert_eq!(verify_rc, PDX_MLDSA_OK);
    }

    /// Flip a single bit of the signature and confirm verify rejects
    /// with the authentication sentinel (not the length sentinel — the
    /// length stayed correct, only a payload bit changed).
    #[test]
    fn ffi_verify_rejects_bit_flipped_signature() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg = b"paideia-as#1347 bit-flip guard";

        let mut sig_out = [0u8; MLDSA65_SIG_LEN];
        let sign_rc = unsafe {
            mldsa65_sign_runtime_entry(
                sk.0.as_ptr(),
                msg.as_ptr(),
                msg.len(),
                sig_out.as_mut_ptr(),
            )
        };
        assert_eq!(sign_rc, PDX_MLDSA_OK);

        // Corrupt one bit deep enough into the signature to hit the
        // `z` polynomial (byte 100 is well past the `c_tilde` prefix,
        // avoiding the tiny probability that a c_tilde perturbation
        // happens to hash-match).
        sig_out[100] ^= 0x01;

        let verify_rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg.as_ptr(),
                msg.len(),
                sig_out.as_ptr(),
                sig_out.len(),
                pk.0.as_ptr(),
                pk.0.len(),
            )
        };
        assert_eq!(verify_rc, PDX_MLDSA_ERR_AUTHENTICATION);
    }

    #[test]
    fn ffi_verify_rejects_null_sig_ptr() {
        let (pk, _sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg = b"x";
        let rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg.as_ptr(),
                msg.len(),
                core::ptr::null(),
                MLDSA65_SIG_LEN,
                pk.0.as_ptr(),
                pk.0.len(),
            )
        };
        assert_eq!(rc, PDX_MLDSA_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_verify_rejects_null_pubkey_ptr() {
        let sig = [0u8; MLDSA65_SIG_LEN];
        let msg = b"x";
        let rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg.as_ptr(),
                msg.len(),
                sig.as_ptr(),
                sig.len(),
                core::ptr::null(),
                MLDSA65_PK_LEN,
            )
        };
        assert_eq!(rc, PDX_MLDSA_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_verify_rejects_wrong_pubkey_length() {
        let (pk, _sk) = MlDsa65Marker::keygen(&mut OsRng);
        let sig = [0u8; MLDSA65_SIG_LEN];
        let msg = b"x";
        // pubkey_len off by one — must be rejected with the length
        // sentinel BEFORE reaching the underlying verify (which would
        // otherwise fold it into a false / authentication error).
        let rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg.as_ptr(),
                msg.len(),
                sig.as_ptr(),
                sig.len(),
                pk.0.as_ptr(),
                pk.0.len() - 1,
            )
        };
        assert_eq!(rc, PDX_MLDSA_ERR_LENGTH);
    }

    #[test]
    fn ffi_verify_rejects_wrong_sig_length() {
        let (pk, _sk) = MlDsa65Marker::keygen(&mut OsRng);
        let sig = [0u8; MLDSA65_SIG_LEN];
        let msg = b"x";
        let rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg.as_ptr(),
                msg.len(),
                sig.as_ptr(),
                sig.len() - 1,
                pk.0.as_ptr(),
                pk.0.len(),
            )
        };
        assert_eq!(rc, PDX_MLDSA_ERR_LENGTH);
    }

    /// Empty-message path: pdx callers pass `msg_len == 0` with a NULL
    /// or dangling `msg_ptr` when the payload was zero-byte (empty pax
    /// blobs, unit-only capability quotes). Mirrors the empty-message
    /// pin on the sign thunk.
    #[test]
    fn ffi_verify_accepts_empty_message_signed_with_null_msg_ptr() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let mut sig_out = [0u8; MLDSA65_SIG_LEN];
        let sign_rc = unsafe {
            mldsa65_sign_runtime_entry(sk.0.as_ptr(), core::ptr::null(), 0, sig_out.as_mut_ptr())
        };
        assert_eq!(sign_rc, PDX_MLDSA_OK);

        let verify_rc = unsafe {
            mldsa65_verify_runtime_entry(
                core::ptr::null(),
                0,
                sig_out.as_ptr(),
                sig_out.len(),
                pk.0.as_ptr(),
                pk.0.len(),
            )
        };
        assert_eq!(verify_rc, PDX_MLDSA_OK);
    }

    /// A valid signature over `msg_a` MUST NOT verify against `msg_b`.
    /// Guards against a stale caller pipeline swapping message pointers
    /// and getting back a spurious "ok" — the same class of bug the
    /// AEAD-open tag-mismatch pin in `paideia-as-crypto::ffi` catches.
    #[test]
    fn ffi_verify_rejects_signature_over_different_message() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let msg_a = b"paideia-as#1347 message A";
        let msg_b = b"paideia-as#1347 message B";

        let mut sig_out = [0u8; MLDSA65_SIG_LEN];
        let sign_rc = unsafe {
            mldsa65_sign_runtime_entry(
                sk.0.as_ptr(),
                msg_a.as_ptr(),
                msg_a.len(),
                sig_out.as_mut_ptr(),
            )
        };
        assert_eq!(sign_rc, PDX_MLDSA_OK);

        let verify_rc = unsafe {
            mldsa65_verify_runtime_entry(
                msg_b.as_ptr(),
                msg_b.len(),
                sig_out.as_ptr(),
                sig_out.len(),
                pk.0.as_ptr(),
                pk.0.len(),
            )
        };
        assert_eq!(verify_rc, PDX_MLDSA_ERR_AUTHENTICATION);
    }
}
