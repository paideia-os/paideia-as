# paideia-as-encoder

**Status:** Shipped — shared x86_64 instruction encoder used by every backend.

Byte-level x86_64 instruction encoding. One of the god files
(`encode_instruction.rs`, `instruction.rs`) called out by the
`feedback_god_file_refactor` directive for an upcoming survey +
refactor.

See `feedback_pdx_encoder_pitfalls` for the recurring encoder
pitfalls softarchs keep re-hitting (`test rN,rN`, `and r11,
imm64`, 2-op `imul r,imm`, multiline `pub let ="..."`,
module-basename mismatch, fingerprint tag size off-by-one).

## Example

Encoding happens via dispatch over `Instruction` / `Mnemonic`
(from `paideia-as-runtime`) and lands bytes in a `Vec<u8>`:

```rust
use paideia_as_encoder::encode;
// consumed by both ELF and PE backends; see dispatch.rs.
```

## See also

- [`../../design/encoding/32bit-mode-survey.md`](../../design/encoding/32bit-mode-survey.md)
  — PVH boot-bridge 32-bit forms.
- `paideia-as-emit` — stable JIT-ready wrapper around the single-
  instruction path.
- `paideia-as-emitter-elf` / `paideia-as-emitter-pax` /
  `paideia-as-emitter-pe` — backend consumers.
