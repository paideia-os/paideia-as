//! C-ABI thunk over [`crate::curve::ed25519::ed25519_verify`] (RFC 8032
//! §5.1.7).
//!
//! One thunk (`paideia_crypto_ed25519_verify`), no parameter struct —
//! the whole call fits in four SysV integer registers. Added for
//! paideia-as Wave γ (γ-02): `libpdx-net`'s TLS 1.3 `CertificateVerify`
//! transcript check (γ-04) calls straight through this symbol against a
//! `KIND_TLS_TRUST`-pinned Ed25519 public key.
//!
//! Split into its own file (alongside `argon2id`, `chacha20_poly1305`,
//! `ml_kem_768`, `hkdf`) per the paideia-as#1354 one-primitive-per-file
//! discipline.

#![allow(unsafe_code)]

use core::slice;

use crate::curve::ed25519::ed25519_verify;

/// Signature verified successfully.
pub const PDX_ED25519_VALID: i32 = 1;
/// Signature did not verify (structurally valid inputs, algebraic
/// reject — RFC 8032 §5.1.7's `[S]B == R + [k]A` check failed, or the
/// public key / `R` component failed to decompress).
pub const PDX_ED25519_INVALID: i32 = 0;
/// A required pointer was NULL, or `msg_len > 0` with a NULL `msg_ptr`.
pub const PDX_ED25519_ERR_INVALID_PARAM: i32 = -1;

/// Verify an Ed25519 signature (RFC 8032 §5.1.7).
///
/// SysV register mapping (4 args, all in registers):
///
/// | Register | Meaning                                          |
/// |----------|---------------------------------------------------|
/// | RDI      | `pk_ptr` — pointer to a 32-byte public key        |
/// | RSI      | `sig_ptr` — pointer to a 64-byte signature (R‖S)  |
/// | RDX      | `msg_ptr` — pointer to the signed message         |
/// | RCX      | `msg_len` — length of the signed message          |
/// | **EAX**  | `PDX_ED25519_VALID` / `_INVALID` / `_ERR_*`       |
///
/// Never panics: every structural or algebraic reject collapses to
/// [`PDX_ED25519_INVALID`] (or [`PDX_ED25519_ERR_INVALID_PARAM`] for a
/// malformed call), matching [`ed25519_verify`]'s own no-panic
/// contract.
///
/// # Safety
///
/// * `pk_ptr` must be non-NULL and valid for reads of 32 bytes.
/// * `sig_ptr` must be non-NULL and valid for reads of 64 bytes.
/// * `msg_ptr` must be non-NULL and valid for reads of `msg_len` bytes
///   whenever `msg_len > 0` (a NULL `msg_ptr` with `msg_len == 0` is
///   accepted — the empty message is a legal Ed25519 input, RFC 8032
///   §7.1 TEST 1).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_ed25519_verify(
    pk_ptr: *const u8,
    sig_ptr: *const u8,
    msg_ptr: *const u8,
    msg_len: usize,
) -> i32 {
    if pk_ptr.is_null() || sig_ptr.is_null() {
        return PDX_ED25519_ERR_INVALID_PARAM;
    }
    if msg_len > 0 && msg_ptr.is_null() {
        return PDX_ED25519_ERR_INVALID_PARAM;
    }

    // SAFETY: caller-asserted; `pk_ptr` valid for 32 bytes.
    let pk: &[u8; 32] = unsafe { &*(pk_ptr as *const [u8; 32]) };
    // SAFETY: caller-asserted; `sig_ptr` valid for 64 bytes.
    let sig: &[u8; 64] = unsafe { &*(sig_ptr as *const [u8; 64]) };
    let msg: &[u8] = if msg_len == 0 {
        &[]
    } else {
        // SAFETY: caller-asserted; `msg_ptr` non-NULL and valid for
        // `msg_len` bytes (checked above).
        unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
    };

    if ed25519_verify(pk, msg, sig) {
        PDX_ED25519_VALID
    } else {
        PDX_ED25519_INVALID
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::ed25519::{ed25519_public_from_secret, ed25519_sign};

    fn decode_hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // RFC 8032 §7.1 TEST 1 — empty message, through the FFI thunk.
    #[test]
    fn ffi_ed25519_verify_rfc_8032_test_1() {
        let sk_bytes = decode_hex(
            "9d61b19deffd5a60ba844af492ec2cc4\
             4449c5697b326919703bac031cae7f60",
        );
        let mut sk = [0u8; 32];
        sk.copy_from_slice(&sk_bytes);

        let pk = ed25519_public_from_secret(&sk);
        let sig = ed25519_sign(&sk, b"");

        let rc = unsafe {
            paideia_crypto_ed25519_verify(
                pk.as_ptr(),
                sig.as_ptr(),
                core::ptr::null(),
                0,
            )
        };
        assert_eq!(rc, PDX_ED25519_VALID);
    }

    #[test]
    fn ffi_ed25519_verify_rejects_mutated_signature() {
        let sk_bytes = decode_hex(
            "9d61b19deffd5a60ba844af492ec2cc4\
             4449c5697b326919703bac031cae7f60",
        );
        let mut sk = [0u8; 32];
        sk.copy_from_slice(&sk_bytes);

        let pk = ed25519_public_from_secret(&sk);
        let msg = b"transcript bytes";
        let mut sig = ed25519_sign(&sk, msg);
        sig[0] ^= 0x01;

        let rc = unsafe {
            paideia_crypto_ed25519_verify(pk.as_ptr(), sig.as_ptr(), msg.as_ptr(), msg.len())
        };
        assert_eq!(rc, PDX_ED25519_INVALID);
    }

    #[test]
    fn ffi_ed25519_verify_null_pk_rejected() {
        let sig = [0u8; 64];
        let msg = b"x";
        let rc = unsafe {
            paideia_crypto_ed25519_verify(
                core::ptr::null(),
                sig.as_ptr(),
                msg.as_ptr(),
                msg.len(),
            )
        };
        assert_eq!(rc, PDX_ED25519_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_ed25519_verify_null_msg_with_nonzero_len_rejected() {
        let pk = [0u8; 32];
        let sig = [0u8; 64];
        let rc = unsafe {
            paideia_crypto_ed25519_verify(pk.as_ptr(), sig.as_ptr(), core::ptr::null(), 4)
        };
        assert_eq!(rc, PDX_ED25519_ERR_INVALID_PARAM);
    }
}
