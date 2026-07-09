# `@link_section` Attribute — Custom ELF Section Emission

**Status:** v0.19 UEFI-ABI Milestone
**Issue:** PA19-r19-010 (phase-19 R19.M4.001)
**Related Tickets:** PA19-r19-010b (lambda/function bindings, deferred to Phase 19.M4.b)

## Overview

The `@link_section` attribute allows data bindings (currently `let` only) to emit into a custom-named ELF section instead of the default `.rodata` or `.data` section. This is essential for UEFI ABI compliance, where boot-time structures (e.g., UEFI header, graphics tables) must occupy well-known sections.

### Example

```pdx
module UefiHeader = structure {
  pub let header : [u8; 64] = @include_bytes("uefi_hdr.bin") @link_section(".uefi_hdr")
  pub let guid_table : [u8; 32] = @include_bytes("guids.bin") @link_section(".uefi_hdr")
}
```

The two bindings emit into a single `.uefi_hdr` ELF section, which the bootloader can locate by name and route directly to fixed memory.

## MVP Scope

**In Scope:**
- `let` bindings only (immutable data), with or without `@include_bytes`/`@include_str` embeds.
- Mutable (`let mut`) bindings — routed to `.data` section flags instead of `.rodata` flags.
- Section-name validation via regex and length constraints.
- Merging — multiple bindings with the same `@link_section` value share one ELF section.
- Interaction with `@align` — both directives apply independently.

**Out of Scope (PA19-r19-010b):**
- Lambda-shaped bindings (`let foo = fn(...) ...`) — blocked with P0284.
- Function-level `@link_section` — deferred to pa-r19-010b.

## Section-Name Policy

### Regex and Length

Valid names match: `[A-Za-z0-9._-]+` (alphanumerics, dot, underscore, hyphen).

- **Length:** 1 to 32 characters (inclusive).
- **Leading dot:** Allowed (e.g., `.uefi_hdr`) but not required.
- **Empty:** Rejected (length 0).

### Rationale

- **Dots** and **hyphens** are standard in ELF section naming (e.g., `.init_array`, `.gnu.hash`).
- **Underscore** common in custom sections (e.g., `.custom_data`).
- **Length limit (32)** balances readability and tool compatibility; most loaders handle arbitrary length, but 32 is a conventional ELF-toolchain baseline.
- **No leading dot requirement** for simplicity — `.uefi_hdr` and `uefi_hdr` are both valid, though `.uefi_hdr` is conventional.

### Diagnostic Code: P0282

Invalid section names emit **P0282**.

## Trust with Reserved Sections

No diagnostic is emitted if the user specifies a reserved section name (`.text`, `.rodata`, `.data`, `.bss`, `.symtab`, `.strtab`, etc.). Behavior is undefined and user's responsibility.

**Rationale:** Simplicity and performance. A whitelist of reserved names would add complexity. The linker and downstream tools (bootloaders, OS kernels) catch misuse at link/load time.

## Interaction with `@align`

The `@align(N)` and `@link_section(name)` directives **compose independently**:

- `@align(N)` sets alignment within the **section** (passed to the ELF writer's alignment field for the symbol).
- `@link_section(name)` sets the **section itself** (determines which ELF section receives the bytes).

Both can be present:
```pdx
pub let header : [u8; 16] = @include_bytes("header.bin") @align(4096) @link_section(".uefi_hdr")
```

The symbol `header` is placed at a 4096-byte-aligned offset within `.uefi_hdr`.

## Interaction with Embed Directives

**Fully compatible.** Embed directives (`@include_bytes`, `@include_str`, `@guid`, future @include_bytecode) produce bytes or data that are then routed to the custom section:

```pdx
pub let data1 : [u8; 8] = @include_bytes("file.bin") @link_section(".custom")
pub let data2 : [u8; 16] = @guid("550e8400-e29b-41d4-a716-446655440000") @link_section(".guid_table")
pub let data3 : [u64; 4] = [1, 2, 3, 4] @link_section(".custom")  // numeric literals
```

All three emit into their specified custom sections.

## Emit Path (ELF only)

### IR Level

- **Lowering** (phase 15): `let_info.link_section` field is populated from the AST attribute if present.
- **Data emission** (phase 19): When creating a `DataEntry` for a let binding, the lowering code checks `link_section` and calls `entry.with_section_override(name)`.

### ELF Writer

- The `cmd_build/elf.rs` module maintains a `custom_sections` HashMap keyed by section name.
- On first encounter of a name, a new section is created with:
  - **Section header flags:** `SHF_ALLOC` (always) + `SHF_WRITE` (if binding is mutable, i.e., `let mut` routed to `.data`).
  - **Section header address alignment:** derived from the `@align` directive or symbol alignment.
- Subsequent bindings with the same name append to the same section.

### Output

Linked ELF contains a custom section (e.g., `.uefi_hdr`) with the correct byte sequence and symbol table entries pointing into it.

## PE-COFF (Deferred)

The MVP is ELF-only. PE-COFF support requires:
- Mapping ELF section names to PE-COFF section names (max 8 chars, different naming conventions).
- Adjusting section flags (e.g., PE-COFF uses different constants for executable, writable, etc.).

**Follow-up Ticket:** To be filed if paideia-os targets PE-COFF UEFI in Phase 20 or later.

## Diagnostic Codes

| Code  | Severity | Trigger                                        | Example Message                              |
|-------|----------|------------------------------------------------|----------------------------------------------|
| P0282 | Error    | Invalid section name (regex mismatch, length) | `P0282: invalid section name "..foo"`        |
| P0283 | Error    | Two `@link_section` on the same binding       | `P0283: duplicate @link_section directive on binding`     |
| P0284 | Error    | Lambda binding with `@link_section`           | `P0284: lambda-shaped bindings cannot use @link_section (deferred to pa-r19-010b)` |

## Future Work

### pa-r19-010b: Lambda and Function Bindings
- Unblock lambda expressions: `let init : (u64) -> u64 = fn(x) -> x + 1 @link_section(".init_funcs")`
- Unblock function pointers with embedded bytecode.

### Module-Level Attribute
- Possibly allow `@link_section` at the module level to set a default for all bindings (subject to binding-level override).

### PE-COFF Parity
- Implement PE-COFF section routing if paideia-os adds x86_64 UEFI PE-COFF targets.

### Diagnostics Refinement
- Cross-binding collision check (candidate future code, not P0283): warn/error if multiple bindings across modules unintentionally share a custom section name. The MVP treats this as an intentional merge (two `pub let`s with the same `@link_section` deliberately concatenate into one section) — a future flag or opt-in lint could catch the accidental case.

## Test Coverage

**Unit Tests:** `crates/paideia-as-parser/tests/...` — @link_section attribute parsing and error recovery.

**Integration Tests:** `crates/paideia-as/tests/build_emit/link_section_probe.rs` — ELF emission verification:
1. `link_section_data_emits_into_named_section` — custom section is created with correct size.
2. `link_section_data_bytes_are_correct` — section contains the expected bytes.
3. `link_section_default_section_absent_for_this_symbol` — symbol is NOT in `.rodata`.
4. `link_section_two_bindings_same_name_share_section` — multiple bindings merge into one section.
5. `link_section_writable_when_mutable` — mutable bindings set the `SHF_WRITE` flag.
6. `link_section_align_still_honored` — `@align` composability is verified.
7. `link_section_on_lambda_emits_p0284` — lambda bindings are rejected with P0284.

## References

- **Parent Issue:** PA19-r19-010
- **ELF Spec:** https://en.wikipedia.org/wiki/Executable_and_Linkable_Format (sections 4.1–4.2)
- **UEFI ABI:** https://uefi.org/ (UEFI PI Specification, chapter on bootloader interfaces)
- **paideia-as Lowering:** `crates/paideia-as/src/cmd_build.rs` (lines ~1200–1350)
- **ELF Writer:** `crates/paideia-as/src/cmd_build/elf.rs` (lines ~140–175)
