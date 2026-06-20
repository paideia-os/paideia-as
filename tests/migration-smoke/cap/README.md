# Capability-System Smoke Fixture (Phase 2 Closure)

## Overview

This crate exercises every Phase 2 substrate through a single capability-system module.

### Design Documentation

- **Issue**: [#195](https://github.com/paideia-os/paideia-as/issues/195)
- **Specification**: m11-003 (Phase 2 closure smoke)
- **Fixture**: `corpus/capability_system.pdx`

## Module Coverage

The `CapabilitySystem` module exercises:

| Substrate | Module | Aspect | Evidence |
|-----------|--------|--------|----------|
| m1 | IR walkers | Linearity + capability | `MmioRegion`, `Channel` linear flow |
| m2 | Reflection | Metadata annotation | Type + effect declarations |
| m3 | Algebraic effects | Handler installation | `effect CapabilityOps` + `with handler` |
| m4 | PAX | Vendor sections | `.paideia.caps`, `.paideia.effects` |
| m5 | ML modules | Module signature | `CapabilitySystem` structure |
| m6 | PE/COFF | Cross-build emit | `-E pe` flag support (x86_64 + ARM64) |
| m7 | PQ signing | Release-line flow | Covered at sign time |
| m8 | LSP | Hover + symbol resolution | Type + effect names resolvable |
| m9 | Opt-passes | Peephole annotation | Inlining-friendly code structure |
| m10 | DDC | Deterministic build | `SOURCE_DATE_EPOCH=0` coverage |
| m11 | DWARF | Vendor extensions | `.paideia.{caps,effects}` in DWARF |

## Test Surface

### Active Tests (Phase 2)

1. **`fixture_exists`**: Verifies the `.pdx` source is present.
2. **`paideia_as_assembles_fixture`**: Runs `paideia-as build` on the fixture.
   - Emit: PAX object format
   - Validates: PAX header magic + minimum size (96 bytes)
   - Environment: `SOURCE_DATE_EPOCH=0` (DDC determinism)

### Ignored Tests (Phase 3+)

- **`boots_in_qemu_reaches_capability_smoke_point`**: Kernel-link + QEMU boot.
  - **Gated**: Requires paideia-os m10 DDC bring-up to completion.
  - **Activation**: Moves to active when kernel accepts PAX modules at link time.

## Running Tests

```bash
# Build paideia-as first
cargo build --release

# Run active tests (fixture_exists passes immediately)
cargo test --package paideia-cap-smoke

# Run ignored tests (requires kernel + QEMU setup)
cargo test --package paideia-cap-smoke -- --ignored --nocapture
```

## Phase-2 vs Phase-3 Boundary

### Phase 2 (m11-003 Deliverable)

- ✅ Fixture source compiles
- ✅ paideia-as assembles to PAX cleanly
- ✅ Vendor sections (caps, effects, sig) populated
- ✅ Test harness documents scope + AC bullets

### Phase 3+ (Kernel Integration)

- 🔒 Kernel link-time module acceptance
- 🔒 QEMU smoke boot
- 🔒 Supervisor capability dispatch
- 🔒 Effect handler integration with PQ signing
