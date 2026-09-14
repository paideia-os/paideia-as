//! C-ABI thunk over [`crate::kdf::hkdf`] (RFC 5869 HKDF-SHA256).
//!
//! One thunk (`paideia_crypto_hkdf_sha256`) + one `#[repr(C)]` parameter
//! bundle (`HkdfParamsC`) covering all three shapes a TLS 1.3 key
//! schedule needs from a single exported symbol (paideia-as Wave γ,
//! γ-01):
//!
//! - **Extract-only** (`HKDF_MODE_EXTRACT`) — `PRK = HKDF-Extract(salt,
//!   IKM)`. This is what RFC 8446 §7.1 calls directly at each rung of
//!   the Early / Handshake / Master secret ladder; the 32-byte PRK
//!   becomes either a `Derive-Secret` input or the salt for the next
//!   `HKDF-Extract`.
//! - **Expand-only** (`HKDF_MODE_EXPAND`) — `OKM = HKDF-Expand(PRK,
//!   info, L)`. `libpdx-net` builds the RFC 8446 §7.1
//!   `HkdfLabel` structure (length + `"tls13 " ‖ label` + context) into
//!   `info` itself and calls this mode for every `HKDF-Expand-Label` /
//!   `Derive-Secret` step; the thunk performs no `HkdfLabel` framing of
//!   its own.
//! - **Extract-then-Expand** (`HKDF_MODE_EXTRACT_AND_EXPAND`) — the
//!   full RFC 5869 §2 construction in one call, for non-TLS callers
//!   that just want `HKDF(salt, IKM, info, L)`.
//!
//! Split into its own file (alongside `argon2id`, `chacha20_poly1305`,
//! `ml_kem_768`) per the paideia-as#1354 one-primitive-per-file
//! discipline. Shared helpers (`PDX_CRYPTO_*` codes) live in `super`.

#![allow(unsafe_code)]

use core::slice;

use alloc::vec::Vec;

use crate::kdf::hkdf::{hkdf_expand, hkdf_extract, HkdfExpandError};

use super::{
    PDX_CRYPTO_ERR_BUFFER_TOO_SMALL, PDX_CRYPTO_ERR_INVALID_LENGTH, PDX_CRYPTO_ERR_INVALID_PARAM,
    PDX_CRYPTO_OK,
};

/// SHA-256 output length (bytes) — also the exact `out_len` a
/// [`HKDF_MODE_EXTRACT`] call must supply.
pub const HKDF_SHA256_PRK_LEN: usize = 32;

/// Extract-only mode: `out` receives the 32-byte PRK. `info_ptr` /
/// `info_len` are ignored.
pub const HKDF_MODE_EXTRACT: i64 = 0;
/// Expand-only mode: `ikm_ptr` / `ikm_len` are reinterpreted as the PRK
/// input (the extract stage is skipped); `salt_ptr` / `salt_len` are
/// ignored.
pub const HKDF_MODE_EXPAND: i64 = 1;
/// Full RFC 5869 §2 HKDF: extract `PRK = HKDF-Extract(salt, ikm)`, then
/// `out = HKDF-Expand(PRK, info, out_len)`.
pub const HKDF_MODE_EXTRACT_AND_EXPAND: i64 = 2;

/// C-ABI parameter bundle for [`paideia_crypto_hkdf_sha256`].
///
/// `#[repr(C)]` for stable layout. Every `_ptr` field may be NULL only
/// when its paired `_len` field is `0` — a non-NULL-required pointer
/// with a NULL value is an invalid-parameter error, not a
/// zero-length input, so callers with a genuinely empty salt/IKM/info
/// (RFC 5869 explicitly permits an empty salt) MUST still pass a
/// non-NULL, zero-length-valid pointer (e.g. a `[0u8; 0]` slice's
/// pointer, which is always non-NULL in Rust and safe to pass through
/// even without dereferencing) or NULL with length 0 — both are
/// accepted; only NULL-with-nonzero-length is rejected.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HkdfParamsC {
    /// Salt pointer (mode 0 / 2 only; ignored in mode 1).
    pub salt_ptr: *const u8,
    /// Salt length in bytes.
    pub salt_len: usize,
    /// IKM pointer (mode 0 / 2), or PRK pointer (mode 1).
    pub ikm_ptr: *const u8,
    /// Length of the buffer `ikm_ptr` references.
    pub ikm_len: usize,
    /// `info` pointer (mode 1 / 2 only; ignored in mode 0).
    pub info_ptr: *const u8,
    /// Length of `info`.
    pub info_len: usize,
}

/// Read an optional (NULL-when-empty) byte slice out of a raw
/// pointer/length pair.
///
/// # Safety
///
/// If `ptr` is non-NULL, it must be valid for reads of `len` bytes.
unsafe fn optional_slice<'a>(ptr: *const u8, len: usize) -> Result<&'a [u8], i64> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(PDX_CRYPTO_ERR_INVALID_PARAM);
    }
    // SAFETY: caller-asserted precondition; `len > 0` here.
    Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

/// HKDF-SHA256 (RFC 5869), C-ABI thunk over
/// [`crate::kdf::hkdf::hkdf_extract`] / [`crate::kdf::hkdf::hkdf_expand`].
///
/// SysV register mapping (4 args, all in registers):
///
/// | Register | Meaning                                          |
/// |----------|---------------------------------------------------|
/// | RDI      | `params` — pointer to [`HkdfParamsC`]             |
/// | RSI      | `mode` — one of `HKDF_MODE_*`                     |
/// | RDX      | `out_ptr` — writable output buffer                |
/// | RCX      | `out_len` — bytes to write into `out_ptr`         |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)                  |
///
/// `HKDF_MODE_EXTRACT` requires `out_len == HKDF_SHA256_PRK_LEN` (32);
/// any other value is rejected as `PDX_CRYPTO_ERR_INVALID_LENGTH`.
/// `HKDF_MODE_EXPAND` and `HKDF_MODE_EXTRACT_AND_EXPAND` accept any
/// `out_len <= 255 * 32` (RFC 5869 §2.3's hard cap); larger requests
/// return `PDX_CRYPTO_ERR_BUFFER_TOO_SMALL`.
///
/// # Safety
///
/// * `params` must be non-NULL and point to a live, fully-initialized
///   [`HkdfParamsC`].
/// * Every non-NULL pointer field of `*params` must be valid for reads
///   of its paired length.
/// * `out_ptr` must be non-NULL and valid for writes of `out_len`
///   bytes (unless `out_len == 0`, which is only ever valid — and a
///   no-op — for the expand-family modes).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_hkdf_sha256(
    params: *const HkdfParamsC,
    mode: i64,
    out_ptr: *mut u8,
    out_len: usize,
) -> i64 {
    if params.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    if out_len > 0 && out_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    // SAFETY: caller-asserted non-NULL + live `HkdfParamsC`.
    let p = unsafe { &*params };

    match mode {
        m if m == HKDF_MODE_EXTRACT => {
            if out_len != HKDF_SHA256_PRK_LEN {
                return PDX_CRYPTO_ERR_INVALID_LENGTH;
            }
            let salt = match unsafe { optional_slice(p.salt_ptr, p.salt_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            let ikm = match unsafe { optional_slice(p.ikm_ptr, p.ikm_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            let prk = hkdf_extract(salt, ikm);
            // SAFETY: `out_len == HKDF_SHA256_PRK_LEN == prk.len()`;
            // `out_ptr` non-NULL (checked above since out_len > 0).
            unsafe { core::ptr::copy_nonoverlapping(prk.as_ptr(), out_ptr, prk.len()) };
            PDX_CRYPTO_OK
        }
        m if m == HKDF_MODE_EXPAND => {
            let prk = match unsafe { optional_slice(p.ikm_ptr, p.ikm_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            let info = match unsafe { optional_slice(p.info_ptr, p.info_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            // SAFETY: `out_ptr`/`out_len` are the caller-asserted
            // preconditions of `paideia_crypto_hkdf_sha256` itself.
            unsafe { expand_into(prk, info, out_ptr, out_len) }
        }
        m if m == HKDF_MODE_EXTRACT_AND_EXPAND => {
            let salt = match unsafe { optional_slice(p.salt_ptr, p.salt_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            let ikm = match unsafe { optional_slice(p.ikm_ptr, p.ikm_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            let info = match unsafe { optional_slice(p.info_ptr, p.info_len) } {
                Ok(s) => s,
                Err(code) => return code,
            };
            let prk = hkdf_extract(salt, ikm);
            // SAFETY: see the `HKDF_MODE_EXPAND` arm above.
            unsafe { expand_into(&prk, info, out_ptr, out_len) }
        }
        _ => PDX_CRYPTO_ERR_INVALID_PARAM,
    }
}

/// Shared expand tail for [`HKDF_MODE_EXPAND`] / [`HKDF_MODE_EXTRACT_AND_EXPAND`].
///
/// # Safety
///
/// `out_ptr` must be non-NULL and valid for writes of `out_len` bytes
/// whenever `out_len > 0`.
unsafe fn expand_into(prk: &[u8], info: &[u8], out_ptr: *mut u8, out_len: usize) -> i64 {
    if out_len == 0 {
        return PDX_CRYPTO_OK;
    }
    // hkdf_expand writes into a caller-shaped `&mut [u8]`; stage it in a
    // heap buffer sized exactly to `out_len` (RFC 5869 §2.3 caps this at
    // `255 * 32 = 8160` bytes, and TLS 1.3 traffic secrets/keys are in
    // practice <= 64 bytes) before copying out to `out_ptr`.
    let mut buf: Vec<u8> = Vec::with_capacity(out_len);
    buf.resize(out_len, 0u8);
    match hkdf_expand(prk, info, &mut buf) {
        Ok(()) => {
            // SAFETY: `out_ptr` valid for `out_len` writes (caller-asserted,
            // `out_len > 0` here); `buf.len() == out_len`.
            unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), out_ptr, buf.len()) };
            PDX_CRYPTO_OK
        }
        Err(HkdfExpandError::OutputTooLong) => PDX_CRYPTO_ERR_BUFFER_TOO_SMALL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(s: &str) -> Vec<u8> {
        let s = s.replace(' ', "");
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(s.len() / 2);
        let mut i = 0;
        while i < bytes.len() {
            let hi = char::from(bytes[i]).to_digit(16).expect("hex digit") as u8;
            let lo = char::from(bytes[i + 1]).to_digit(16).expect("hex digit") as u8;
            out.push((hi << 4) | lo);
            i += 2;
        }
        out
    }

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    // RFC 5869 §A.1 test case 1, exercised through all three modes.
    #[test]
    fn ffi_hkdf_extract_matches_rfc_5869_a1() {
        let ikm = decode_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = decode_hex("000102030405060708090a0b0c");

        let params = HkdfParamsC {
            salt_ptr: salt.as_ptr(),
            salt_len: salt.len(),
            ikm_ptr: ikm.as_ptr(),
            ikm_len: ikm.len(),
            info_ptr: core::ptr::null(),
            info_len: 0,
        };
        let mut prk = [0u8; HKDF_SHA256_PRK_LEN];
        let rc = unsafe {
            paideia_crypto_hkdf_sha256(
                &params as *const _,
                HKDF_MODE_EXTRACT,
                prk.as_mut_ptr(),
                prk.len(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(
            hex(&prk),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );
    }

    #[test]
    fn ffi_hkdf_expand_matches_rfc_5869_a1() {
        let prk = decode_hex("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
        let info = decode_hex("f0f1f2f3f4f5f6f7f8f9");

        let params = HkdfParamsC {
            salt_ptr: core::ptr::null(),
            salt_len: 0,
            ikm_ptr: prk.as_ptr(),
            ikm_len: prk.len(),
            info_ptr: info.as_ptr(),
            info_len: info.len(),
        };
        let mut okm = vec![0u8; 42];
        let rc = unsafe {
            paideia_crypto_hkdf_sha256(
                &params as *const _,
                HKDF_MODE_EXPAND,
                okm.as_mut_ptr(),
                okm.len(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(
            hex(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    #[test]
    fn ffi_hkdf_extract_and_expand_matches_rfc_5869_a3_empty_salt_info() {
        let ikm = decode_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");

        let params = HkdfParamsC {
            salt_ptr: core::ptr::null(),
            salt_len: 0,
            ikm_ptr: ikm.as_ptr(),
            ikm_len: ikm.len(),
            info_ptr: core::ptr::null(),
            info_len: 0,
        };
        let mut okm = vec![0u8; 42];
        let rc = unsafe {
            paideia_crypto_hkdf_sha256(
                &params as *const _,
                HKDF_MODE_EXTRACT_AND_EXPAND,
                okm.as_mut_ptr(),
                okm.len(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(
            hex(&okm),
            "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"
        );
    }

    #[test]
    fn ffi_hkdf_extract_wrong_out_len_rejected() {
        let ikm = [0x0bu8; 22];
        let params = HkdfParamsC {
            salt_ptr: core::ptr::null(),
            salt_len: 0,
            ikm_ptr: ikm.as_ptr(),
            ikm_len: ikm.len(),
            info_ptr: core::ptr::null(),
            info_len: 0,
        };
        let mut out = [0u8; 16]; // wrong: must be 32 for extract mode.
        let rc = unsafe {
            paideia_crypto_hkdf_sha256(
                &params as *const _,
                HKDF_MODE_EXTRACT,
                out.as_mut_ptr(),
                out.len(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_LENGTH);
    }

    #[test]
    fn ffi_hkdf_null_params_rejected() {
        let mut out = [0u8; 32];
        let rc = unsafe {
            paideia_crypto_hkdf_sha256(
                core::ptr::null(),
                HKDF_MODE_EXTRACT,
                out.as_mut_ptr(),
                out.len(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_hkdf_unknown_mode_rejected() {
        let ikm = [0x0bu8; 22];
        let params = HkdfParamsC {
            salt_ptr: core::ptr::null(),
            salt_len: 0,
            ikm_ptr: ikm.as_ptr(),
            ikm_len: ikm.len(),
            info_ptr: core::ptr::null(),
            info_len: 0,
        };
        let mut out = [0u8; 32];
        let rc = unsafe {
            paideia_crypto_hkdf_sha256(&params as *const _, 99, out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }
}
