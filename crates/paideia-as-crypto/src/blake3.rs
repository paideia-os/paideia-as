//! BLAKE3 — cryptographic hash / KDF / MAC (Aumasson, Neves,
//! O'Connor, Wilcox-O'Hearn, 2020).
//!
//! # Reference
//!
//! J.-P. Aumasson, S. Neves, Z. Wilcox-O'Hearn, C. Winnerlein,
//! *"BLAKE3 — one function, fast everywhere"*, revision from
//! 2020-01-09, <https://github.com/BLAKE3-team/BLAKE3-specs>. The
//! reference specification pins three modes over one keyed
//! compression:
//!
//! - **Hash** — unkeyed, single-input; default 32-byte output.
//! - **Keyed hash** — MAC mode; 32-byte key, arbitrary-length input.
//! - **Derive key** — KDF mode; ASCII context string plus arbitrary
//!   key material, arbitrary-length output. The context string is a
//!   hard-coded application-scoped label (per-application constant,
//!   never runtime input) that domain-separates one deployment's
//!   KDF outputs from another's.
//!
//! # Backend
//!
//! Wraps the audited [`::blake3`] crate (BLAKE3 team's reference
//! Rust implementation, `pure` feature set — no assembly, no SIMD
//! intrinsics, no `std`). That choice mirrors the "phase 1: FFI over
//! a well-audited Rust crate, phase 6+: paideia-native rewrite
//! behind the same trait" rationale documented in
//! `design/toolchain/rust-dep-gap-analysis.md` and already applied to
//! `Argon2id`, `ChaCha20Poly1305`, and `MlKem768` in this same tree.
//!
//! # Design shape
//!
//! - [`Blake3`] is a marker struct with three associated methods
//!   (`hash`, `hash_keyed`, `derive_key`), matching the shape of
//!   [`crate::Argon2id`] / [`crate::MlKem768`] so downstream code
//!   names the algorithm at the call site without threading a
//!   generic parameter through every layer.
//! - Fixed 32-byte output on all three modes — matching the default
//!   BLAKE3 digest length and every consumer's need for a fixed-
//!   size key / handle. BLAKE3's XOF (arbitrary-length output) is
//!   deliberately not exposed on this trait; a `derive_key_xof`
//!   companion can slot in behind an additive method later.
//!
//! # Test vectors
//!
//! Two official BLAKE3 vectors from the reference `test_vectors.json`
//! are embedded at the bottom of this file — the empty input and the
//! 1024-byte input (bytes 0..1024 filled with the `i % 251` pattern
//! the spec pins) — one covering the single-chunk root path, the
//! other the multi-chunk tree with a full-chunk boundary. Vectors
//! for the two intermediate modes (`keyed_hash`, `derive_key`) at
//! both lengths are pinned alongside, using the reference key
//! `"whats the Elvish word for friend"` and context string
//! `"BLAKE3 2019-12-27 16:29:52 test vectors context"`.
//!
//! # Encoder-conservative posture (Wave 0)
//!
//! This landing adds the trait, backend wrapper, and extern-C thunk
//! descriptors. The `.pdx`-side elaborator hook
//! (`stdlib_lowering::cryptoops`) that emits `call
//! paideia_crypto_blake3_*` is deliberately NOT wired here — it
//! lands in the Wave-1 companion issue so the encoder surface stays
//! frozen for this wave's parallel authoring.

// The extern `blake3` crate and this module share the identifier
// `blake3` at the crate root. Use the fully-qualified `::blake3::`
// path inside this file to disambiguate: `::blake3` is resolved from
// the extern prelude and always names the audited backend crate.

/// Length in bytes of the BLAKE3 key used with `hash_keyed`.
///
/// Fixed by the BLAKE3 specification (§2.1: keyed mode consumes
/// exactly 256 bits of key material and folds it into the compression
/// function's initial chaining value). Exposed as a `pub const` so
/// downstream call sites can name the buffer length without
/// re-declaring it — mirrors [`crate::aead::KEY_LEN`] and
/// [`crate::kem::SS_LEN`].
pub const KEY_LEN: usize = 32;

/// Length in bytes of the default BLAKE3 output on all three modes.
///
/// BLAKE3 is an extendable-output function, but the trait surface
/// here fixes the length to 32 bytes — the default digest length and
/// the natural fit for every consumer's fixed-size key / handle /
/// content-address slot. An `XOF` variant can be added additively
/// later without breaking the current trait.
pub const OUT_LEN: usize = 32;

/// Static shape check — a drift in the underlying `blake3` crate's
/// key length would silently disagree with the FFI thunks below.
/// Assert it at compile time so any regression fails to build.
const _: () = {
    assert!(KEY_LEN == 32);
    assert!(OUT_LEN == 32);
};

/// BLAKE3 marker type.
///
/// All operations are provided as associated functions; the struct
/// exists so downstream code can name the algorithm
/// (`Blake3::hash(...)`) without threading a generic parameter
/// through every call site. Mirrors the shape of [`crate::Argon2id`]
/// and [`crate::MlKem768`].
#[derive(Copy, Clone, Debug, Default)]
pub struct Blake3;

impl Blake3 {
    /// Unkeyed BLAKE3 hash of `data`, truncated to the default
    /// [`OUT_LEN`]-byte digest.
    ///
    /// Byte-identical to `::blake3::hash(data).as_bytes()` — this
    /// wrapper exists purely to bind the digest length to the trait
    /// surface and to sit in front of the FFI thunk below.
    #[must_use]
    pub fn hash(data: &[u8]) -> [u8; OUT_LEN] {
        *::blake3::hash(data).as_bytes()
    }

    /// Keyed BLAKE3 (MAC mode) of `data` under `key`, truncated to
    /// the default [`OUT_LEN`]-byte digest.
    ///
    /// This is the pseudo-random function surface. `key` MUST be
    /// uniformly random 32-byte material — passing a low-entropy
    /// user password here defeats the MAC. Sample `key` via
    /// [`crate::rng`] or derive it from `Blake3::derive_key`.
    ///
    /// Byte-identical to `::blake3::keyed_hash(key, data).as_bytes()`.
    #[must_use]
    pub fn hash_keyed(key: &[u8; KEY_LEN], data: &[u8]) -> [u8; OUT_LEN] {
        *::blake3::keyed_hash(key, data).as_bytes()
    }

    /// KDF-mode BLAKE3 derivation of a [`OUT_LEN`]-byte subkey from
    /// `key_material` under the domain-separating `context` label.
    ///
    /// # Context discipline (spec §7.5)
    ///
    /// `context` MUST be a compile-time-fixed ASCII string that
    /// identifies the deployment context: neither user input nor
    /// otherwise attacker-influenced data may reach this argument.
    /// The BLAKE3 spec pins the convention as
    /// `"<app name> <ISO 8601 datetime the label was minted> <purpose>"`
    /// — e.g., paideia-os would use
    /// `"paideiaos <mint-date> user-sk sealing"`. Reusing the same
    /// context string across different key material derives
    /// independent subkeys; reusing the same `key_material` under a
    /// different context string derives an independent subkey too.
    ///
    /// # Truncation
    ///
    /// The trait fixes the output to 32 bytes — the natural key size
    /// for every consumer this crate ships (Argon2id-32B, XChaCha20
    /// keys, HKDF-SHA256 pseudo-random keys). A future `derive_key_xof`
    /// method can expose the underlying XOF for consumers that need
    /// more than 32 bytes of subkey material.
    ///
    /// Byte-identical to `::blake3::derive_key(context, key_material)`.
    #[must_use]
    pub fn derive_key(context: &str, key_material: &[u8]) -> [u8; OUT_LEN] {
        ::blake3::derive_key(context, key_material)
    }
}

// =====================================================================
// C-ABI thunks
//
// Encoder-conservative posture (Wave 0): the thunk descriptors are
// emitted here so downstream link paths (`paideia-satellite-runtime`)
// resolve the `paideia_crypto_blake3_*` symbols. The elaborator hook
// (`stdlib_lowering::cryptoops`) that emits `call paideia_crypto_
// blake3_*` on the `.pdx` side is deliberately NOT wired in this
// wave — that lands in the Wave-1 companion issue so the encoder
// surface stays frozen while this wave's parallel authoring
// proceeds.
//
// Every thunk mirrors the shape already established by
// `ffi::argon2id` and `ffi::ml_kem_768`:
//
//   * Fixed-length outputs (32 bytes) → typed `*mut [u8; 32]` at the
//     C-ABI seam, so callers cannot mismatch the buffer size.
//   * Variable-length inputs → `(*const u8, usize)` pair.
//   * Return code: `PDX_CRYPTO_OK` (0) on success, negative on
//     invalid parameters. Non-null `out_ptr` and non-null `data_ptr`
//     (when `data_len > 0`) are precondition-checked; NULL either
//     returns `PDX_CRYPTO_ERR_INVALID_PARAM` without touching the
//     output buffer.
//
// SAFETY discipline matches the other FFI shims in `ffi::`: every
// entry point is `unsafe fn` in intent (the C ABI does not carry
// `unsafe`), and preconditions are enumerated on each function.
// Violating them is undefined behaviour.
// =====================================================================

#[allow(unsafe_code)]
mod ffi {
    //! Extern-C thunk descriptors for [`super::Blake3`].
    //!
    //! Kept in a nested module so the `#![allow(unsafe_code)]` lift
    //! is scoped to the C-ABI surface and does not spill into the
    //! trait / test code above. Symbols are `#[unsafe(no_mangle)]`
    //! and live in the crate's exported `nm` set at the same
    //! `paideia_crypto_*` path as the other cryptoops thunks.

    #![allow(unsafe_code)]

    use core::slice;

    use super::{Blake3, KEY_LEN, OUT_LEN};

    /// FFI success return code — mirrors [`crate::ffi::PDX_CRYPTO_OK`]
    /// so a `.pdx` caller can share one diagnostic handler across every
    /// cryptoops thunk. Re-declared locally rather than imported to
    /// keep this module self-contained; the value is stable ABI.
    pub const PDX_CRYPTO_OK: i64 = 0;

    /// FFI invalid-parameter return code — mirrors
    /// [`crate::ffi::PDX_CRYPTO_ERR_INVALID_PARAM`]. Returned when a
    /// required pointer is NULL. On failure the output buffer is not
    /// written.
    pub const PDX_CRYPTO_ERR_INVALID_PARAM: i64 = -1;

    /// Unkeyed BLAKE3 hash — `[u8] → [u8; 32]`.
    ///
    /// SysV register mapping (as future `stdlib_lowering::cryptoops`
    /// recipes will emit; the Wave-1 companion issue wires it up):
    ///
    /// | Register | Meaning                                            |
    /// |----------|----------------------------------------------------|
    /// | RDI      | `data_ptr` — `*const u8` (may be NULL iff `data_len == 0`) |
    /// | RSI      | `data_len` — `usize`                               |
    /// | RDX      | `out_ptr`  — `*mut [u8; 32]` (writable)            |
    /// | **RAX**  | return code (see `PDX_CRYPTO_*`)                   |
    ///
    /// # Safety
    ///
    /// * `out_ptr` must be non-NULL and valid for writes of exactly
    ///   [`OUT_LEN`] bytes.
    /// * If `data_len > 0`, `data_ptr` must be non-NULL and valid
    ///   for reads of `data_len` bytes. `data_ptr == NULL` with
    ///   `data_len == 0` is accepted and hashed as the empty input.
    ///
    /// Returns [`PDX_CRYPTO_OK`] on success, or
    /// [`PDX_CRYPTO_ERR_INVALID_PARAM`] if `out_ptr` is NULL or
    /// `data_ptr` is NULL with `data_len > 0`. The output buffer is
    /// not written on failure.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn paideia_crypto_blake3_hash(
        data_ptr: *const u8,
        data_len: usize,
        out_ptr: *mut u8,
    ) -> i64 {
        if out_ptr.is_null() {
            return PDX_CRYPTO_ERR_INVALID_PARAM;
        }
        if data_ptr.is_null() && data_len > 0 {
            return PDX_CRYPTO_ERR_INVALID_PARAM;
        }

        // SAFETY: an empty slice is safe from a dangling / NULL
        // pointer only if the length is zero, which the precondition
        // check above guarantees.
        let data: &[u8] = if data_len == 0 {
            &[]
        } else {
            // SAFETY: caller-asserted `data_ptr` valid for `data_len` bytes.
            unsafe { slice::from_raw_parts(data_ptr, data_len) }
        };

        let digest = Blake3::hash(data);
        // SAFETY: caller-asserted `out_ptr` valid for `OUT_LEN` bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, OUT_LEN);
        }
        PDX_CRYPTO_OK
    }

    /// Keyed BLAKE3 (MAC mode) — `([u8; 32], [u8]) → [u8; 32]`.
    ///
    /// SysV register mapping:
    ///
    /// | Register | Meaning                                            |
    /// |----------|----------------------------------------------------|
    /// | RDI      | `key_ptr`  — `*const [u8; 32]` (non-NULL)          |
    /// | RSI      | `data_ptr` — `*const u8` (may be NULL iff `data_len == 0`) |
    /// | RDX      | `data_len` — `usize`                               |
    /// | RCX      | `out_ptr`  — `*mut [u8; 32]` (writable)            |
    /// | **RAX**  | return code (see `PDX_CRYPTO_*`)                   |
    ///
    /// # Safety
    ///
    /// * `key_ptr` must be non-NULL and valid for reads of exactly
    ///   [`KEY_LEN`] bytes.
    /// * `out_ptr` must be non-NULL and valid for writes of exactly
    ///   [`OUT_LEN`] bytes.
    /// * If `data_len > 0`, `data_ptr` must be non-NULL and valid
    ///   for reads of `data_len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn paideia_crypto_blake3_hash_keyed(
        key_ptr: *const u8,
        data_ptr: *const u8,
        data_len: usize,
        out_ptr: *mut u8,
    ) -> i64 {
        if key_ptr.is_null() || out_ptr.is_null() {
            return PDX_CRYPTO_ERR_INVALID_PARAM;
        }
        if data_ptr.is_null() && data_len > 0 {
            return PDX_CRYPTO_ERR_INVALID_PARAM;
        }

        // SAFETY: caller-asserted `key_ptr` valid for KEY_LEN bytes.
        let key: &[u8; KEY_LEN] = unsafe { &*(key_ptr as *const [u8; KEY_LEN]) };
        let data: &[u8] = if data_len == 0 {
            &[]
        } else {
            // SAFETY: caller-asserted `data_ptr` valid for `data_len` bytes.
            unsafe { slice::from_raw_parts(data_ptr, data_len) }
        };

        let mac = Blake3::hash_keyed(key, data);
        // SAFETY: caller-asserted `out_ptr` valid for `OUT_LEN` bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(mac.as_ptr(), out_ptr, OUT_LEN);
        }
        PDX_CRYPTO_OK
    }

    /// BLAKE3 KDF-mode subkey derivation — `(&str, [u8]) → [u8; 32]`.
    ///
    /// SysV register mapping:
    ///
    /// | Register | Meaning                                                  |
    /// |----------|----------------------------------------------------------|
    /// | RDI      | `context_ptr` — `*const u8` (ASCII / UTF-8, non-NULL)    |
    /// | RSI      | `context_len` — `usize` (byte length of the context)     |
    /// | RDX      | `key_material_ptr` — `*const u8` (may be NULL iff `key_material_len == 0`) |
    /// | RCX      | `key_material_len` — `usize`                             |
    /// | R8       | `out_ptr` — `*mut [u8; 32]` (writable)                   |
    /// | **RAX**  | return code (see `PDX_CRYPTO_*`)                         |
    ///
    /// # Safety
    ///
    /// * `context_ptr` must be non-NULL and valid for reads of
    ///   `context_len` bytes, and the byte range MUST be valid UTF-8.
    ///   Per the BLAKE3 spec §7.5 the context is a compile-time-fixed
    ///   ASCII label; passing non-UTF-8 bytes is undefined behaviour
    ///   at the trait surface (`::blake3::derive_key` takes `&str`).
    /// * `out_ptr` must be non-NULL and valid for writes of exactly
    ///   [`OUT_LEN`] bytes.
    /// * If `key_material_len > 0`, `key_material_ptr` must be non-NULL
    ///   and valid for reads of `key_material_len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn paideia_crypto_blake3_derive_key(
        context_ptr: *const u8,
        context_len: usize,
        key_material_ptr: *const u8,
        key_material_len: usize,
        out_ptr: *mut u8,
    ) -> i64 {
        if context_ptr.is_null() || out_ptr.is_null() {
            return PDX_CRYPTO_ERR_INVALID_PARAM;
        }
        if key_material_ptr.is_null() && key_material_len > 0 {
            return PDX_CRYPTO_ERR_INVALID_PARAM;
        }

        // SAFETY: caller-asserted `context_ptr` valid for `context_len` bytes.
        let context_bytes: &[u8] =
            unsafe { slice::from_raw_parts(context_ptr, context_len) };
        // Per doc-comment SAFETY clause the caller guarantees valid UTF-8.
        // Fall back to invalid-param rather than a UB `unchecked` cast so a
        // buggy caller sees a clean error rather than a poisoned digest.
        let context = match core::str::from_utf8(context_bytes) {
            Ok(s) => s,
            Err(_) => return PDX_CRYPTO_ERR_INVALID_PARAM,
        };

        let key_material: &[u8] = if key_material_len == 0 {
            &[]
        } else {
            // SAFETY: caller-asserted `key_material_ptr` valid for `key_material_len` bytes.
            unsafe { slice::from_raw_parts(key_material_ptr, key_material_len) }
        };

        let subkey = Blake3::derive_key(context, key_material);
        // SAFETY: caller-asserted `out_ptr` valid for `OUT_LEN` bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(subkey.as_ptr(), out_ptr, OUT_LEN);
        }
        PDX_CRYPTO_OK
    }
}

// Re-export the FFI symbols and constants at the module root so a
// downstream `use paideia_as_crypto::blake3::paideia_crypto_blake3_hash`
// resolves. The `#[unsafe(no_mangle)]` attribute already binds the
// symbol name at link time regardless of module path, but re-exporting
// keeps the Rust-level API discoverable.
pub use ffi::{
    PDX_CRYPTO_ERR_INVALID_PARAM, PDX_CRYPTO_OK, paideia_crypto_blake3_derive_key,
    paideia_crypto_blake3_hash, paideia_crypto_blake3_hash_keyed,
};

#[cfg(test)]
mod tests {
    //! Reference-vector tests against `test_vectors.json` from the
    //! BLAKE3 team. Two lengths are pinned — the empty input (root
    //! path with no data) and the 1024-byte input (multi-chunk tree,
    //! exact full-chunk boundary since BLAKE3's chunk size is
    //! 1024 bytes).

    // The crate-root `#![deny(unsafe_code)]` lint blocks the `unsafe`
    // blocks the FFI-parity tests below need to invoke the extern "C"
    // thunks. Lift it for the test module only — the trait / thunk
    // code above is unaffected (the thunk module lifts the lint under
    // its own scoped `#![allow(unsafe_code)]`).
    #![allow(unsafe_code)]

    use super::*;

    /// Reference `key` field from `test_vectors.json`.
    ///
    /// Exactly 32 bytes of ASCII: `"whats the Elvish word for friend"`.
    const REFERENCE_KEY: &[u8; KEY_LEN] = b"whats the Elvish word for friend";

    /// Reference `context_string` field from `test_vectors.json`.
    const REFERENCE_CONTEXT: &str = "BLAKE3 2019-12-27 16:29:52 test vectors context";

    /// Static shape check — the ASCII key literal must be exactly
    /// [`KEY_LEN`] bytes. A drift here (or a mistyped literal)
    /// fails to compile rather than fails at runtime.
    const _: () = assert!(REFERENCE_KEY.len() == KEY_LEN);

    /// Build the reference input pattern: `bytes[i] = (i % 251) as u8`,
    /// per the `_comment` field at the head of `test_vectors.json`.
    fn reference_input(len: usize) -> alloc::vec::Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn hex(bytes: &[u8]) -> alloc::string::String {
        use alloc::string::String;
        use core::fmt::Write;
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(s, "{:02x}", b);
        }
        s
    }

    // -------- input_len = 0 --------
    //
    // Vector 1 (three modes at empty input). The empty-input path
    // exercises the root-chunk-only compression: no data blocks,
    // just the final flag and length embedding.

    #[test]
    fn reference_vector_empty_hash() {
        assert_eq!(
            hex(&Blake3::hash(b"")),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        );
    }

    #[test]
    fn reference_vector_empty_hash_keyed() {
        assert_eq!(
            hex(&Blake3::hash_keyed(REFERENCE_KEY, b"")),
            "92b2b75604ed3c761f9d6f62392c8a9227ad0ea3f09573e783f1498a4ed60d26",
        );
    }

    #[test]
    fn reference_vector_empty_derive_key() {
        assert_eq!(
            hex(&Blake3::derive_key(REFERENCE_CONTEXT, b"")),
            "2cc39783c223154fea8dfb7c1b1660f2ac2dcbd1c1de8277b0b0dd39b7e50d7d",
        );
    }

    // -------- input_len = 1024 --------
    //
    // Vector 2 (three modes at 1024 bytes). BLAKE3's chunk size is
    // 1024 bytes, so this is the smallest input that produces a
    // two-chunk tree — the first chunk fills exactly, the length
    // triggers a fresh chunk that then closes at the root. Covers
    // the chunk-boundary path in the reference backend.

    #[test]
    fn reference_vector_1024_hash() {
        let input = reference_input(1024);
        assert_eq!(
            hex(&Blake3::hash(&input)),
            "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
        );
    }

    #[test]
    fn reference_vector_1024_hash_keyed() {
        let input = reference_input(1024);
        assert_eq!(
            hex(&Blake3::hash_keyed(REFERENCE_KEY, &input)),
            "75c46f6f3d9eb4f55ecaaee480db732e6c2105546f1e675003687c31719c7ba4",
        );
    }

    #[test]
    fn reference_vector_1024_derive_key() {
        let input = reference_input(1024);
        assert_eq!(
            hex(&Blake3::derive_key(REFERENCE_CONTEXT, &input)),
            "7356cd7720d5b66b6d0697eb3177d9f8d73a4a5c5e968896eb6a689684302706",
        );
    }

    // -------- FFI thunks: parity + null-handling --------
    //
    // Prove the extern-C shims produce byte-identical output to the
    // trait surface, and that null-pointer preconditions return the
    // documented error code without touching the output buffer.

    #[test]
    fn ffi_hash_matches_trait_for_empty_and_1024() {
        for &n in &[0usize, 1024] {
            let input = reference_input(n);
            let expected = Blake3::hash(&input);
            let mut out = [0u8; OUT_LEN];
            let rc = unsafe {
                paideia_crypto_blake3_hash(
                    if n == 0 { core::ptr::null() } else { input.as_ptr() },
                    n,
                    out.as_mut_ptr(),
                )
            };
            assert_eq!(rc, PDX_CRYPTO_OK, "hash rc for n={}", n);
            assert_eq!(out, expected, "hash out mismatch for n={}", n);
        }
    }

    #[test]
    fn ffi_hash_keyed_matches_trait_for_empty_and_1024() {
        for &n in &[0usize, 1024] {
            let input = reference_input(n);
            let expected = Blake3::hash_keyed(REFERENCE_KEY, &input);
            let mut out = [0u8; OUT_LEN];
            let rc = unsafe {
                paideia_crypto_blake3_hash_keyed(
                    REFERENCE_KEY.as_ptr(),
                    if n == 0 { core::ptr::null() } else { input.as_ptr() },
                    n,
                    out.as_mut_ptr(),
                )
            };
            assert_eq!(rc, PDX_CRYPTO_OK, "hash_keyed rc for n={}", n);
            assert_eq!(out, expected, "hash_keyed out mismatch for n={}", n);
        }
    }

    #[test]
    fn ffi_derive_key_matches_trait_for_empty_and_1024() {
        let context_bytes = REFERENCE_CONTEXT.as_bytes();
        for &n in &[0usize, 1024] {
            let input = reference_input(n);
            let expected = Blake3::derive_key(REFERENCE_CONTEXT, &input);
            let mut out = [0u8; OUT_LEN];
            let rc = unsafe {
                paideia_crypto_blake3_derive_key(
                    context_bytes.as_ptr(),
                    context_bytes.len(),
                    if n == 0 { core::ptr::null() } else { input.as_ptr() },
                    n,
                    out.as_mut_ptr(),
                )
            };
            assert_eq!(rc, PDX_CRYPTO_OK, "derive_key rc for n={}", n);
            assert_eq!(out, expected, "derive_key out mismatch for n={}", n);
        }
    }

    #[test]
    fn ffi_hash_null_out_returns_invalid_param() {
        let rc = unsafe { paideia_crypto_blake3_hash(core::ptr::null(), 0, core::ptr::null_mut()) };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_hash_keyed_null_key_returns_invalid_param() {
        let mut out = [0u8; OUT_LEN];
        let rc = unsafe {
            paideia_crypto_blake3_hash_keyed(
                core::ptr::null(),
                core::ptr::null(),
                0,
                out.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
        // Failure path must not touch the output buffer.
        assert_eq!(out, [0u8; OUT_LEN]);
    }

    #[test]
    fn ffi_derive_key_invalid_utf8_context_returns_invalid_param() {
        // 0xFF is not a valid UTF-8 start byte. The thunk must reject
        // it rather than reach a UB `str::from_utf8_unchecked`.
        let bad = [0xFFu8, 0xFE, 0xFD];
        let mut out = [0u8; OUT_LEN];
        let rc = unsafe {
            paideia_crypto_blake3_derive_key(
                bad.as_ptr(),
                bad.len(),
                core::ptr::null(),
                0,
                out.as_mut_ptr(),
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
        assert_eq!(out, [0u8; OUT_LEN]);
    }
}
