# paideia-as-linker

**Status:** Shipped — `paideia-link`, the PAX-format linker (4-phase pipeline: parse → resolve → relocate → emit).

Reads PAX objects produced by `paideia-as-emitter-pax`, resolves
cross-object symbol references, applies relocations, and emits a
final executable PAX image. Diagnostic codes in the `B1700`–`B17FF`
range.

## Example

```sh
cargo run --release --bin paideia-link -- --input a.pax b.pax --output out.pax
```

## See also

- [`../../design/toolchain/paideia-link.md`](../../design/toolchain/paideia-link.md)
  — Specification v1.0 + Phase 2 outcome; defines the 4-phase
  contract.
- `paideia-as-emitter-pax` — upstream producer.
