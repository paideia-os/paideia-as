# paideia-link — PAX-format linker

**Status:** Specification v1.0 + Phase 2 outcome
**Scope:** The 4-phase linker that produces final PAX objects from one or more `.pax` inputs.

## 0. Pipeline overview (PL-D7)

paideia-link runs in four sequential phases:

1. **parse** — read each `.pax` input, validate the magic + format version, parse the header + section table. Section content is sliced from the input bytes for lazy decoding by later phases.

2. **resolve** — build a global symbol table + global capability table indexed by BLAKE3 name hash. Every undefined symbol is matched against an exporting input; every imported capability is matched against an exporting input.

3. **relocate** — apply each input's `.relocs` to its target section bytes. Phase-2-m11 supports Abs64 (write resolved symbol id + addend into the 8-byte slot at the target offset). Pc32 / GotPc32 / PltPc32 / CapBind are passed through structurally for a future PR.

4. **emit** — concatenate the relocated section contents, build a final `PaxHeader` with the `Executable` flag set, recompute the `blake3_content_hash`, write the bytes to the output path.

The high-level `link(inputs, output)` driver chains all four phases.

## 1. Section content formats

The PAX object format itself lives in `paideia-as-emitter-pax`. Each section content type has its own canonical serialisation:

| Section type      | Code | Entry size       | Module           |
|-------------------|------|------------------|------------------|
| `.code`           | 0x01 | variable         | `section`        |
| `.rodata`         | 0x02 | variable         | `section`        |
| `.data`           | 0x03 | variable         | `section`        |
| `.bss`            | 0x04 | (no on-disk)     | `section`        |
| `.paideia.caps`   | 0x10 | 32 bytes / entry | `caps`           |
| `.paideia.effects`| 0x11 | 16 + 4*N / entry | `effects`        |
| `.paideia.unsafe` | 0x12 | 40 bytes / entry | `audit`          |
| `.paideia.opt-passes` | 0x13 | 32 bytes / entry | `audit`     |
| `.paideia.lin`    | 0x14 | 32 bytes / entry | `audit`          |
| `.symtab`         | 0x20 | 48 bytes / entry | `symtab`         |
| `.relocs`         | 0x21 | 32 bytes / entry | `relocs`         |
| `.imports`        | 0x22 | 32 bytes / entry | `imports`        |
| `.exports`        | 0x23 | 32 bytes / entry | `imports`        |

The full byte layouts are documented as in-line schemas in each module.

## 2. Diagnostic codes

Linker codes live under `Category::B` (binary emission, 1700-1799) per the existing diagnostic taxonomy in `paideia-as-diagnostics/src/code.rs`.

| Code  | Source                       | Meaning                                                  |
|-------|------------------------------|----------------------------------------------------------|
| B1700 | linker: resolve              | Undefined symbol — no input exports the named symbol.    |
| B1701 | linker: resolve              | Unbound capability — no input exports the named capability. |

## 3. Strong-wins-over-Weak

Two defined symbols with the same `blake3_name_hash` are resolved by binding kind. `SymBinding::Global` wins over `SymBinding::Weak`. Two `Global` definitions are an error (PR-future; current behaviour is last-wins).

## Phase 2 outcome (m4-pax-and-paideia-link)

The m4 series (PRs #388–#399) shipped the full PAX format + the 4-phase linker:

- `paideia-as-emitter-pax` gained 12 modules: `header`, `section`, `caps`, `effects`, `audit`, `symtab`, `relocs`, `imports`, `hash`, plus the `Architecture` / `HeaderFlag` / `SectionType` / `SectionFlag` enums.
- `paideia-as-linker` was promoted from stub to `parse` + `resolve` + `relocate` + `emit` modules with a `link()` driver.
- `paideia-as build --emit pax` is wired through `cmd_build`.
- `pax-introspect` binary dumps a PAX's header + section table for debugging.
- `tests/pax-load-smoke/` ships a mock supervisor that loads a PAX, parses its metadata sections, and symbolically dispatches by BLAKE3 name hash. Sets the pattern the real m10 supervisor follows.

### Resolved: AS5 + AS8

- **AS5 (BLAKE3 content hash)** — `CanonicalContent::finalize` produces a deterministic 32-byte BLAKE3 hash over the (header-with-zeroed-hash + section-table + section-contents) byte stream. A verifier can recompute the hash from the on-disk bytes and compare.
- **AS8 (PAX object format)** — `paideia-as-emitter-pax::PaxHeader` is the canonical 96-byte header; the 12 section content types listed above cover all PaideiaOS-specific metadata (capabilities, effect rows, audit trails) plus the standard ELF-equivalent code/data/symtab/relocs/imports/exports.

### Phase-2-m12 simulation scope

The mock supervisor (`tests/pax-load-smoke/`) parses + dispatches symbolically. It does not execute code; the real PaideiaOS supervisor is the m10 ddc-bringup deliverable. The smoke harness sets the parsing pattern + the entry-point definition (phase-2-m12: first `SymEntry` with `Global` binding + `Default` visibility) that the real supervisor inherits.

## References

- `design/toolchain/custom-assembler.md` §1.1 — PAX header spec (upstream).
- `design/toolchain/custom-assembler.md` §1.2 — section table types (upstream).
- `design/toolchain/custom-assembler.md` §15 — AS-decision register (AS5 + AS8 entries).
- PRs #388–#399 — the m4 deliverable.
