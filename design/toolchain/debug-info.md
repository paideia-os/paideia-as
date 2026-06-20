# DWARF debug info for paideia-as

**Status:** Phase 2 m11-001 vendor-ID registration + design.
**Scope:** Records the DWARF 5 vendor identifier paideia-as uses, the vendor extensions it ships, and the rationale.

## 0. The vendor ID

paideia-as uses the vendor identifier **`paideia`** (lowercase, ASCII) for all DWARF 5 vendor extensions.

### Why this name + why no collision

The DWARF 5 standard (§7.4) reserves the high bit of tag, attribute, and form numbers for vendor extensions. Vendor identifiers are conventionally strings naming the vendor. Per the DWARF Debugging Information Format Committee's [registry of common vendor extensions](https://dwarfstd.org/Vendor-Extensions.html), known vendor IDs include `GNU`, `LLVM`, `MIPS`, `APPLE`, `BORLAND`, etc.

`paideia` does not collide with any registered vendor ID as of HEAD (2026-06-19 check against the DWARF committee's published list). The name is the project's canonical short name and matches the existing PAX section prefix convention (`.paideia.caps`, `.paideia.effects`, etc. from m4).

## 1. Reserved vendor numeric ranges

Per the DWARF 5 standard, vendor extensions live above the standard reservations:

- Tag numbers: `0x4080` and above (per §7.5.4 reservation for `DW_TAG_lo_user` through `DW_TAG_hi_user`).
- Attribute numbers: `0x2000` and above (`DW_AT_lo_user`).
- Form numbers: `0x1f00` and above (`DW_FORM_lo_user`).

paideia-as carves out the following sub-ranges:

| Kind        | paideia range          | Notes                                       |
|-------------|------------------------|---------------------------------------------|
| Tag         | `0x4100..=0x41ff`      | 256 slots for paideia-specific debug tags.  |
| Attribute   | `0x2100..=0x21ff`      | 256 slots for paideia attributes.           |
| Form        | `0x1f10..=0x1f1f`      | 16 slots for paideia-specific forms.        |

## 2. Vendor extensions to be populated (m11-002)

m11-002 populates three vendor sections in the emitted ELF / PAX:

- **`.debug.paideia.caps`** — per-DIE capability-binding sites (mirrors the `.paideia.caps` PAX section from m4-003 but in DWARF DIE form for debugger consumption).
- **`.debug.paideia.effects`** — per-function effect-row annotations (mirrors `.paideia.effects` from m4-004).
- **`.debug.paideia.sig`** — per-function hybrid signature reference (mirrors `.paideia.sig` from m7-003).

The DIE form is the canonical debugger surface; the PAX sections are the canonical loader / verifier surface. Both must agree at emit time; m11-003 ships the end-to-end harness that asserts agreement.

## 3. Specific tag / attribute allocations

Initial allocations (m11-001 reserves the slots; m11-002 populates them):

| Symbol                               | Numeric  | Kind  | Purpose                                |
|--------------------------------------|----------|-------|----------------------------------------|
| `DW_TAG_paideia_capability_binding`  | `0x4100` | Tag   | Marks a capability-binding-site DIE.   |
| `DW_TAG_paideia_effect_row`          | `0x4101` | Tag   | Marks a function's effect-row DIE.     |
| `DW_TAG_paideia_signature`           | `0x4102` | Tag   | Marks a function's hybrid-signature DIE. |
| `DW_AT_paideia_lin_class`            | `0x2100` | Attr  | Substructural class: Linear / Affine / Ordered / Unrestricted. |
| `DW_AT_paideia_cap_kind`             | `0x2101` | Attr  | CapKind enum value (MmioMemCap, IpcChannel, …). |
| `DW_AT_paideia_effect_id_list`       | `0x2102` | Attr  | Block-form list of EffectId u32 values. |
| `DW_AT_paideia_row_var_id`           | `0x2103` | Attr  | Row-variable id; absent ↔ closed row.  |
| `DW_AT_paideia_sig_blake3`           | `0x2104` | Attr  | First 8 bytes of BLAKE3(hybrid_sig).   |
| `DW_FORM_paideia_effect_list`        | `0x1f10` | Form  | Length-prefixed Vec<u32> for effect rows. |

The list is intentionally minimal at m11-001. Adding a tag / attribute requires updating this doc + bumping the vendor minor version.

## 4. Vendor versioning

The vendor itself carries an internal version, recorded in a `.debug.paideia.version` section with a single 4-byte LE u32:

```
0  : major (1)
1  : minor (0)
2  : patch (0)
3  : reserved (0)
```

m11-001 sets vendor version `1.0.0.0`. Any breaking change to the allocations above bumps the major; new tags / attributes within the reserved range bump the minor.

## 5. Coexistence with standard DWARF

paideia-as emits **standard DWARF first**, with vendor extensions as additional DIEs / attributes. A consumer that doesn't understand paideia vendor extensions still gets a fully functional source-level debugging experience.

The vendor extensions are *additive*. They never replace a standard DWARF construct that already exists.

## 6. References

- [DWARF Debugging Information Format Version 5](https://dwarfstd.org/doc/DWARF5.pdf) — §7.4 / §7.5.4 reservations.
- [Vendor Extensions registry](https://dwarfstd.org/Vendor-Extensions.html).
- m4-003 `.paideia.caps` — the PAX-side mirror.
- m4-004 `.paideia.effects` — the PAX-side mirror.
- m7-003 `.paideia.sig` — the PAX-side mirror.
- OS-requirements §3.2 AS7 — the original question this doc resolves.
