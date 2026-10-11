# paideia-as-crypto

**Status:** Shipped — narrow cryptographic primitives for paideia-as tooling and (via `stdlib_lowering`) the PaideiaOS runtime.

Deliberately narrow: exposes traits and reference implementations
for the primitives PaideiaOS depends on for user authentication and
at-rest key sealing. Each primitive is anchored to a normative
reference (RFC / NIST publication) so trait shape and vector suite
move together.

Current primitives: Argon2id (password hashing), ChaCha20-Poly1305
(AEAD at-rest sealing), ML-KEM-768 and ML-DSA-65 (post-quantum
key-encapsulation and signatures), Ed25519 and X25519 (classical
baseline). FFI sub-modules (`ffi::argon2id`,
`ffi::chacha20_poly1305`, …) expose C-ABI thunks that `.pdx` code
calls via `stdlib_lowering::cryptoops` / `mldsaops`.

## Example

```rust
use paideia_as_crypto::Signer;
// paideia-pq-sign wires the hybrid Ed25519 + ML-DSA-65 composition
// on top of this surface.
```

## See also

- [`../../design/security/pq-trust-root.md`](../../design/security/pq-trust-root.md)
  — hybrid trust-root design (Phase 2 m7 closure).
- `paideia-pq-sign` — the hybrid signing CLI + library.
- `paideia-satellite-runtime` — the link-line shim that routes
  satellite-tool crypto FFI relocations.
