# paideia-as-emitter-pax

**Status:** Shipped — PAX (PaideiaOS Architectural Executable) emitter.

PAX is the canonical PaideiaOS object format: carries capability
signatures, effect rows, PQ signature slots, and BLAKE3 content
hashes alongside the standard code / data / symbol / relocation
tables. Selected by `--emit pax` or by one of the PaideiaOS-native
`--target` triplets.

## Example

```sh
paideia-as build examples/15_sum_array.pdx --emit pax -o /tmp/sum.pax
```

## See also

- [`../../design/toolchain/paideia-link.md`](../../design/toolchain/paideia-link.md)
  — PAX format and the 4-phase linker contract.
- `paideia-as-linker` (`paideia-link`) — downstream consumer.
- `paideia-pq-sign` — the hybrid PQ signer that fills the PAX
  signature slot.
