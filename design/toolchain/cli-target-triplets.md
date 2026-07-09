# CLI Target Triplets (`--target`) — Issue #1107

**Status**: MVP delivered  
**Phase**: Phase 6 m4-002  
**Ticket**: #1107

## Overview

This document specifies the `--target <triplet>` shortcut flag for `paideia-as build`, which provides a user-friendly alternative to `--emit <format>`. Instead of remembering format names like `pe-coff`, `elf64`, `pax`, users can specify a target triplet like `uefi-x86_64`.

## Grammar

The `--target` flag accepts a **target triplet** of the form:

```
<target_triplet> ::= <arch>-<object_format>
<arch> ::= x86_64
<object_format> ::= uefi | elf-kernel | elf-user | pax
```

### MVP Target Triplets

Phase 6 m4-002 supports four target triplets:

| Triplet | Emitter | Output Format | Purpose |
|---------|---------|---------------|---------|
| `uefi-x86_64` | paideia-as-emitter-pe | PE32+ COFF | UEFI firmware/bootloader |
| `elf-kernel-x86_64` | paideia-as-emitter-elf | ELF64 | Kernel-mode objects |
| `elf-user-x86_64` | paideia-as-emitter-elf | ELF64 | User-mode objects |
| `pax-x86_64` | paideia-as-emitter-pax | PAX | PaideiaOS native format |

## Mapping

Each target triplet maps deterministically to an output format:

```rust
Target::UefiX86_64      → EmitFormat::PeCoff
Target::ElfKernelX86_64 → EmitFormat::Elf64
Target::ElfUserX86_64   → EmitFormat::Elf64
Target::PaxX86_64       → EmitFormat::Pax
```

Note: `elf-kernel-x86_64` and `elf-user-x86_64` produce byte-identical output today (both map to ELF64). The distinction is semantic; separation supports future divergence (e.g., kernel-specific sections, relocations).

## CLI Semantics

### Conflict Rule

`--target` and `--emit` **cannot be used together**:

```bash
# ✓ Valid
paideia-as build foo.pdx --target uefi-x86_64 -o foo.o

# ✓ Valid (backward compat)
paideia-as build foo.pdx --emit pe-coff -o foo.o

# ✗ Error: cannot be used with
paideia-as build foo.pdx --target uefi-x86_64 --emit elf64 -o foo.o
```

The conflict is enforced by clap at parse time via `conflicts_with = "emit"` and `conflicts_with = "target"`.

### Backward Compatibility

- Omitting both `--target` and `--emit` produces `.placeholder` (Phase-1 default).
- The `--emit` flag remains fully functional.
- Existing scripts and builds are unaffected.

## Extension Policy

Future target triplets follow the grammar above. To add support:

1. Add new enum variant to `Target` in `cli.rs` with `#[value(name = "...")]`.
2. Add mapping case in `resolve_target()` in `cmd_build.rs`.
3. Add test variants in `tests/common/harness.rs::TargetTriplet`.
4. Add test case to `tests/build_emit/target_triplet.rs`.
5. Update this document.

### Candidates for Future Phases

- `elf-kernel-riscv64` — RISC-V 64-bit kernel
- `pax-riscv64` — PAX for RISC-V
- `uefi-aarch64` — ARM 64-bit UEFI
- `elf-user-arm32` — 32-bit user objects (if supported)

## Migration Guidance

### For Build Scripts

**Old**:
```bash
paideia-as build --emit pe-coff -o firmware.o firmware.pdx
paideia-as build --emit elf64 -o kernel.o kernel.pdx
```

**New** (equivalent):
```bash
paideia-as build --target uefi-x86_64 -o firmware.o firmware.pdx
paideia-as build --target elf-kernel-x86_64 -o kernel.o kernel.pdx
```

### For CI/CD

No changes required if using `--emit`. The `--target` flag is purely additive.

## Non-Goals

- Runtime triplet detection (triplets are not auto-discovered from source).
- Architecture narrowing based on target (all targets run on x86_64 hosts).
- Default target selection (no implicit `--target` from environment or config file).
- Nested triplet formats (e.g., `x86_64-pc-elf64`); naming follows existing Paideia conventions.

## Implementation

See `crates/paideia-as/src/cli.rs`, `crates/paideia-as/src/cmd_build.rs`, and `crates/paideia-as/tests/build_emit/target_triplet.rs`.

### Key Structures

**`Target` enum** (cli.rs):
```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum Target {
    #[value(name = "uefi-x86_64")]
    UefiX86_64,
    #[value(name = "elf-kernel-x86_64")]
    ElfKernelX86_64,
    #[value(name = "elf-user-x86_64")]
    ElfUserX86_64,
    #[value(name = "pax-x86_64")]
    PaxX86_64,
}
```

**Conflict enforcement** (cli.rs):
```rust
#[arg(long = "emit", conflicts_with = "target")]
emit: Option<String>,

#[arg(long = "target", value_enum, conflicts_with = "emit")]
target: Option<Target>,
```

**Format resolution** (cmd_build.rs):
```rust
fn resolve_target(target: Target) -> EmitFormat {
    match target {
        Target::UefiX86_64 => EmitFormat::PeCoff,
        Target::ElfKernelX86_64 => EmitFormat::Elf64,
        Target::ElfUserX86_64 => EmitFormat::Elf64,
        Target::PaxX86_64 => EmitFormat::Pax,
    }
}
```

## Testing

MVP coverage includes:

1. **Format correctness** (3 tests):
   - UEFI → PE32+ with correct magic, subsystem, machine.
   - ELF kernel/user → ELF64 with correct magic, class.
   - PAX → valid PAX magic.

2. **Byte-identity** (1 test):
   - `--target uefi-x86_64` ≡ `--emit pe-coff`.

3. **Error handling** (2 tests):
   - Conflict error when both flags provided.
   - Invalid value error for unrecognized triplets.

4. **Backward compatibility** (2 tests):
   - No `--target`, no `--emit` → `.placeholder` (unchanged).
   - Conflicting flags error with clap message.

All tests pass; 6 adversarial mutation probes confirm implementation correctness.

## References

- Issue #1107: `--target <triplet>` shortcut for build
- PR #1108: Phase 6 m4-001 (softarch)
- `crates/paideia-as/tests/build_emit/target_triplet.rs` (test corpus)
