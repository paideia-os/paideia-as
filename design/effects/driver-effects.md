# Driver-Side Effect Vocabulary (pa-r14-010)

## Overview

This document specifies the **driver-side effect discipline** for PaideiaOS kernel device drivers. Effects track side effects (I/O, state changes) that require explicit handler installation in the type system. The 5 driver effects enable hardware-aware programming with compile-time verification.

## Effects

### 1. MmioRead
- **Purpose**: Memory-mapped I/O read from device space.
- **Reference**: #950 (pa-r14-007)
- **Declared in**: `src/toolchain/abi/abi.pdx`
- **Example instructions/APIs**: Generic MMIO read operations from driver frameworks.

### 2. MmioWrite
- **Purpose**: Memory-mapped I/O write to device space.
- **Reference**: #950 (pa-r14-007)
- **Declared in**: `src/toolchain/abi/abi.pdx`
- **Example instructions/APIs**: Generic MMIO write operations from driver frameworks.

### 3. CachePolicy
- **Purpose**: Modifies cache-line state (coherency operations).
- **Reference**: #953 (pa-r14-010)
- **Declared in**: `src/toolchain/abi/abi.pdx`
- **Associated instructions**:
  - `wbinvd` — write-back and invalidate entire cache
  - `invd` — invalidate entire cache (no write-back)
  - `clflush` — invalidate cache-line at address
  - `clflushopt` — optimized cache-line invalidation
  - `clwb` — cache-line write-back
- **Use case**: Cache coherency when DMA operations access main memory; required before/after device writes that may not observe cache state.

### 4. NonTemporal
- **Purpose**: Bypasses cache (non-temporal access).
- **Reference**: #953 (pa-r14-010)
- **Declared in**: `src/toolchain/abi/abi.pdx`
- **Associated instructions**:
  - `movnti` — non-temporal move to memory (64-bit)
  - `movntdq` — non-temporal move to memory (128-bit SSE)
  - `movntpd` / `movntps` — non-temporal floating-point moves
- **Use case**: Bulk data transfer to device buffers or DMA staging areas where caching would cause unnecessary eviction.

### 5. DmaBarrier
- **Purpose**: Orders DMA completion (synchronization barrier).
- **Reference**: #953 (pa-r14-010)
- **Declared in**: `src/toolchain/abi/abi.pdx`
- **Associated instructions**:
  - `sfence` — store fence (when paired with DMA setup)
  - `mfence` — full fence (complete memory barrier)
- **Use case**: Ensures all prior writes reach the device before allowing device DMA to proceed; guarantees memory ordering across the CPU↔device interface.

## Composition Rules

Effects compose via effect rows `!{E1, E2, ...}`. Common compositions for drivers:

| Composition | Use Case |
|---|---|
| `!{MmioWrite, CachePolicy}` | Write to device, then flush cache to ensure device sees memory state |
| `!{NonTemporal, DmaBarrier}` | Bulk transfer to DMA buffer, then fence before device reads it |
| `!{MmioRead, MmioWrite, CachePolicy}` | Complex device interaction (read-modify-write + coherency) |

**Subtyping and unification** (phase-3+): Effect rows with row variables (`!{E1, ..., r}`) unify with concrete rows via the row-polymorphic unifier in `paideia-as-effects/src/unify.rs`.

## Cross-References

- **#950 (pa-r14-007)**: Initial MMIO effect declarations in `abi.pdx`.
- **#953 (pa-r14-010)**: This issue — formalizes the full vocabulary and adds Rust machinery.
- **pa-r14-005**: Cache instruction consumers (wbinvd, clflush implementations).
- **pa-r14-003**: Non-temporal move (movnti) implementation.
- **design/toolchain/custom-assembler.md §4**: Effect formalism and row semantics.
- **design/toolchain/abi.md §0**: ABI canonical definitions.

## Implementation Status

### Completed (v0.14)
- ✓ Effect declarations in `src/toolchain/abi/abi.pdx` — CachePolicy, NonTemporal, DmaBarrier, MmioRead, MmioWrite
- ✓ Round-trip parsing tests (`.pdx` source → AST → IR) — 4 test fixtures in `tests/end-to-end/codes/pa_r14_010_*.pdx`

### Deferred (v0.15+)
- Registration in effect registry (string interning) — effects will be interned on first use per existing EffectRegistry design
- Effect composition in type signatures — parser currently does not support effects in fn signatures; requires grammar extension
- Effect propagation from mnemonics (e.g., `wbinvd` → `CachePolicy`) — requires instruction metadata in the IR
- Effect inference at call sites — requires full type threading through elaborator
- Capability guards for driver code — requires capability-system integration (#453)

## Notes

Per **custom-assembler.md §4.2**, effects are part of the function signature alongside linearity and capabilities:

```
fn_signature := 
  | "fn" params "->" type
  | "fn" params "->" "!" "{" effects "}" type
  | "fn" params "@" "{" capabilities "}" "->" "!" "{" effects "}" type
```

The effect row is checked at **call sites** via row unification; handlers are validated via the **F1101 checker** (`check_handler.rs`).
