//! Digital-signature schemes.
//!
//! Sibling of [`crate::kem`] — hosts the post-quantum + classical
//! signature primitives whose FFI thunks satellite and kernel
//! builds link into their `.pdx` runtime.
//!
//! Each signature impl is a marker struct with associated functions
//! (`sign`, `verify`) exposed on top of fixed-size byte buffers
//! whose lengths are compile-time constants of the underlying
//! parameter set. That shape mirrors [`crate::kem::MlKem768`] —
//! consistent with the pattern established for `Argon2id`,
//! `ChaCha20Poly1305`, and ML-KEM in this same tree.
//!
//! Current impls:
//! - [`ml_dsa_65::MlDsa65`] — Module-Lattice-based Digital
//!   Signature Algorithm, [FIPS 204] category-3 parameter set. The
//!   `no_std + alloc` sibling of the std-linked signer surface
//!   already provided by `paideia-pq-sign::mldsa` for offline tools.
//!
//! Intended consumers:
//! - Wave υ (paideia-as υ-01 / υ-02): kernel- and satellite-linked
//!   `.pdx` runtime callers that verify ML-DSA-65 signatures at
//!   `.pdxpkg` / `.pdxtrust` install time, and offline packaging
//!   tooling that produces those signatures via the sign thunk.
//! - The `paideia-pq-sign` crate keeps its own std-linked shape for
//!   the CLI-side offline signer where allocation and `std::error`
//!   are already available; the split is deliberate — the same
//!   underlying `ml-dsa` crate services both.
//!
//! [FIPS 204]: https://csrc.nist.gov/pubs/fips/204/final
//!
//! Design invariants shared by every impl:
//!
//! 1. **Fixed-size buffers on the surface.** Public keys, secret
//!    keys, and signatures are all fixed-length byte arrays per the
//!    normative spec, exposed as `pub const` sizes. The FFI thunks
//!    in [`crate::ffi`] cast raw pointers to fixed-size array
//!    references — sizes are ABI, not inputs.
//! 2. **Verify is public data.** The verifier borrows public
//!    material only, so timing side-channels on verify are
//!    non-issues per the FIPS-204 threat model.
//! 3. **Sign takes an explicit rnd input where FIPS-204 permits
//!    hedged variants.** The paideia-as-crypto surface uses the
//!    deterministic variant (`rnd = 0`) so sign output is
//!    reproducible across processes; the ACVP hedged-variant
//!    vectors that require a caller-supplied rnd live on the
//!    `paideia-pq-sign::mldsa::kat` surface and are not needed by
//!    the kernel/satellite runtime consumers.

mod ml_dsa_65;

pub use ml_dsa_65::{
    MlDsa65, MLDSA65_PK_LEN, MLDSA65_SEED_LEN, MLDSA65_SIG_LEN, SigError,
};
