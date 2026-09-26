//! crypto_shim — satellite crypto FFI re-exports + ML-DSA-65 fail-closed stubs.
//!
//! Split out of `lib.rs` per `design/paideia-as-debt-catalog.md` §7
//! (`PAS-DEBT-B6-001`) and the v0.33-M1-005 manifest. Every symbol
//! exposed here is re-exported by the parent crate via
//! `pub use crypto_shim::*;` so the public link-time surface is
//! unchanged: satellite `ld -nostdlib` lines continue to resolve
//! against the same symbol names.
//!
//! Sourcing rules (see the parent crate's module-level docs for full
//! rationale):
//!
//! - The AEAD / KDF / KEM / Ed25519 / HKDF thunks are `pub use`d from
//!   `paideia-as-crypto` so `ld` sees exactly one archive-scope
//!   definition of each. Cargo's dependency graph forces the object
//!   code into `libpaideia_satellite_runtime.a`.
//! - `mldsa65_{sign,verify}_runtime_entry` are DEFINED here as
//!   fail-closed stubs returning [`PDX_MLDSA_ERR_NO_SIGNER`] (`-6`).
//!   The satellite tools that currently ship never invoke either
//!   intrinsic at runtime; they only need the symbols to resolve.
//!   A negative sentinel from verify is band-consistent with
//!   `paideia-pq-sign`'s ABI (`0 = valid, negative = invalid`).

// ---------------------------------------------------------------------
// Crypto — re-export from the single-source `paideia-as-crypto` crate.
// ---------------------------------------------------------------------
//
// These `pub use` statements are how design §3.1's "single source of
// truth" invariant is enforced in Rust:
//   * The symbol NAMES are `#[unsafe(no_mangle)]` in the source crate,
//     so `ld` sees exactly one archive-scope definition each — no
//     duplicate-symbol linker error, no silent shadowing.
//   * `pub use` (rather than `#[allow(dead_code)] use`) documents that
//     they are part of the satellite runtime's public ABI surface.
//   * `pub use` also forces cargo to depend on `paideia-as-crypto`'s
//     object code, so the FFI thunks are packed into the staticlib
//     archive rather than dead-code-eliminated.
pub use paideia_as_crypto::ffi::paideia_crypto_argon2id_derive;
pub use paideia_as_crypto::ffi::paideia_crypto_chacha20_poly1305_open;
pub use paideia_as_crypto::ffi::paideia_crypto_chacha20_poly1305_seal;
// paideia-as#1352 — ML-KEM-768 KEM (FIPS 203). Same discipline as the
// crypto trio above: `#[unsafe(no_mangle)]` bodies live in
// `paideia-as-crypto`; `pub use` here forces cargo to pack them into
// `libpaideia_satellite_runtime.a`. R108 device-to-device key
// agreement + R91+ networking are the intended consumers.
pub use paideia_as_crypto::ffi::paideia_crypto_ml_kem_768_decaps;
pub use paideia_as_crypto::ffi::paideia_crypto_ml_kem_768_encaps;
pub use paideia_as_crypto::ffi::paideia_crypto_ml_kem_768_keygen;
// paideia-as Wave γ (γ-01 / γ-02) — HKDF-SHA256 (RFC 5869) and
// Ed25519 verify (RFC 8032 §5.1.7). Consumed by `libpdx-net`'s
// TLS 1.3 key schedule / transcript verify / record layer.
pub use paideia_as_crypto::ffi::paideia_crypto_ed25519_verify;
pub use paideia_as_crypto::ffi::paideia_crypto_hkdf_sha256;

// ---------------------------------------------------------------------
// Signing / verification — fail-closed stubs for the ML-DSA-65 pair.
// ---------------------------------------------------------------------
//
// Both `mldsa65_sign_runtime_entry` (v0.28.0 landing, issue #1330) and
// `mldsa65_verify_runtime_entry` (v0.28.1 landing, issue #1347) are
// emitted as `call` relocations by the elaborator's
// `stdlib_lowering::mldsaops` recipe whenever a `.pdx` module imports
// either intrinsic. The satellite runtime MUST define both symbols so
// satellite `ld -nostdlib` link lines resolve regardless of runtime
// reachability. Both bodies unconditionally return
// `PDX_MLDSA_ERR_NO_SIGNER`.

/// Fail-closed sentinel for the satellite `mldsa65_sign_runtime_entry`
/// AND `mldsa65_verify_runtime_entry` stubs.
///
/// Value: `-6`. Band-consistent with the existing
/// `paideia-pq-sign::ffi::PDX_MLDSA_*` codes (`-1` = InvalidParam,
/// `-2` = Length, `-3` = Authentication; `-4`/`-5` reserved). `-6` is
/// the first previously-unused code and signals "no signer is compiled
/// into this binary" as distinct from any runtime-error variant.
///
/// `.pdx`-side consumers MUST surface this code as a user-visible
/// error rather than silently treating it as success — otherwise a
/// satellite would produce a "signed" volume that is not actually
/// signed, or accept an unverified signature as valid. Design risk R4.
///
/// For verify: `paideia-pq-sign`'s ABI treats any negative return as
/// "did not authenticate" (`0 = valid, negative = invalid or bad
/// shape`). Returning `-6` slots into the existing failure band.
pub const PDX_MLDSA_ERR_NO_SIGNER: i64 = -6;

/// Satellite build of `mldsa65_sign_runtime_entry` — fail-closed.
///
/// # Contract
///
/// This symbol MUST resolve at satellite link time (the elaborator
/// emits a `call` relocation whenever a `.pdx` module imports the
/// ML-DSA-65 sign intrinsic, even when runtime never calls it). This
/// body returns [`PDX_MLDSA_ERR_NO_SIGNER`] unconditionally.
///
/// # Signature — must match `paideia-pq-sign::ffi::mldsa65_sign_runtime_entry`
///
/// SysV AMD64 register mapping:
///
/// | Register | Meaning                                          |
/// |----------|--------------------------------------------------|
/// | RDI      | `seed_ptr`     — `*const u8`, 32-byte seed       |
/// | RSI      | `msg_ptr`      — `*const u8`                     |
/// | RDX      | `msg_len`      — `usize`                         |
/// | RCX      | `sig_out_ptr`  — `*mut u8`, >= 3309 bytes        |
/// | **RAX**  | return code                                      |
///
/// Any drift from the kernel-side signature would silently break the
/// fallback safety contract. Keep them lockstep.
///
/// # Safety
///
/// The function does not dereference any argument pointer. The
/// `#[unsafe(no_mangle)]` attribute is required so `ld` finds the
/// symbol by its exact name.
#[unsafe(no_mangle)]
pub extern "C" fn mldsa65_sign_runtime_entry(
    _seed_ptr: *const u8,
    _msg_ptr: *const u8,
    _msg_len: usize,
    _sig_out_ptr: *mut u8,
) -> i64 {
    PDX_MLDSA_ERR_NO_SIGNER
}

/// Satellite build of `mldsa65_verify_runtime_entry` — fail-closed.
///
/// # Contract
///
/// This symbol MUST resolve at satellite link time — the elaborator's
/// `stdlib_lowering::mldsaops` recipe emits a `call` relocation to it
/// whenever a `.pdx` module imports the ML-DSA-65 verify intrinsic
/// (v0.28.1 landing, `#1347`), and satellite `ld -nostdlib` links
/// have no per-object dead-code elimination. This body returns
/// [`PDX_MLDSA_ERR_NO_SIGNER`] unconditionally.
///
/// The negative return is honest per `paideia-pq-sign`'s ABI
/// (`0 = valid, negative = invalid or bad shape`); a caller that
/// treats the return as boolean (`is_valid = ret == 0`) correctly
/// refuses to accept the "signature" as verified.
///
/// # Signature — must match `paideia-pq-sign::ffi::mldsa65_verify_runtime_entry`
///
/// SysV AMD64 register mapping:
///
/// | Register | Meaning                                          |
/// |----------|--------------------------------------------------|
/// | RDI      | `msg_ptr`     — `*const u8`                      |
/// | RSI      | `msg_len`     — `usize`                          |
/// | RDX      | `sig_ptr`     — `*const u8`, == 3309 bytes       |
/// | RCX      | `sig_len`     — `usize` (== 3309)                |
/// | R8       | `pubkey_ptr`  — `*const u8`, == 1952 bytes       |
/// | R9       | `pubkey_len`  — `usize` (== 1952)                |
/// | **RAX**  | return code: 0 = valid, negative = invalid       |
///
/// Any drift from the kernel-side signature would silently break the
/// fallback safety contract. Keep them lockstep.
///
/// # Safety
///
/// The function does not dereference any argument pointer. The
/// `#[unsafe(no_mangle)]` attribute is required so `ld` finds the
/// symbol by its exact name.
#[unsafe(no_mangle)]
pub extern "C" fn mldsa65_verify_runtime_entry(
    _msg_ptr: *const u8,
    _msg_len: usize,
    _sig_ptr: *const u8,
    _sig_len: usize,
    _pubkey_ptr: *const u8,
    _pubkey_len: usize,
) -> i64 {
    PDX_MLDSA_ERR_NO_SIGNER
}
