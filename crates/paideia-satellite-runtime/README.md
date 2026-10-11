# paideia-satellite-runtime

**Status:** Shipped — link-line shim for satellite host tools.

`.pdx` code compiled by `paideia-as` emits `call` relocations
against the crypto FFI intrinsics declared by the
`stdlib_lowering::cryptoops` and `stdlib_lowering::mldsaops`
recipes. This crate re-exports the authoritative per-primitive
symbol list from `paideia-as-crypto::ffi` (`argon2id`,
`chacha20_poly1305`, `mldsa65`, …) so satellite tools linking
against `.pdx`-emitted objects do not need to spell every symbol
out by hand.

## Example

Satellite tools add this crate as a build dependency; it then
pulls the right FFI thunks into the link line automatically.

## See also

- `paideia-as-crypto` — the primitive layer and canonical FFI
  symbol list (split per primitive at `paideia-as#1354`).
- `paideia-as-elaborator/stdlib_lowering.rs::cryptoops` / `mldsaops`
  — the recipes that emit the relocations.
