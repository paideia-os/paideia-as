# paideia-pq-sign

**Status:** Shipped — post-quantum signing CLI + library.

Wires Ed25519 (classical, RFC 8032) and ML-DSA-65 (post-quantum,
FIPS 204) behind a common `Signer` trait with AND-verification
semantics: a release artifact is accepted only if both signatures
verify. Supports RFC 3161 timestamping and a JSON-lines revocation
list. Hardware HSM backends (PKCS#11, YubiHSM2) ship with a
hybrid-fallback rule so a HSM without PQ support surfaces diagnostic
`Q0902` rather than silently degrade.

## Example

```sh
cargo run --release --bin paideia-pq-sign -- sign out.pax --key dev-hybrid.key
```

## See also

- [`../../design/security/pq-trust-root.md`](../../design/security/pq-trust-root.md)
  — Phase 2 m7 closure; hybrid trust-root design.
- [`../../docs/release-signing.md`](../../docs/release-signing.md)
  — operational guide.
- `paideia-as-crypto` — primitive layer.
