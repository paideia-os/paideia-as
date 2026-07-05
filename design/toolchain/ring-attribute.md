# Ring Buffer Attribute (`@ring`)

**Issue:** [#951](https://github.com/PaideiaOS/paideia-as/issues/951)
**Phase:** Phase 14 (PA14-r14-008)

## Overview

The `@ring(slots=M, slot_size=K)` attribute on a top-level `let` binding synthesizes a high-performance lock-free ring buffer structure. The compiler expands one annotated binding into four complementary data structures—pool, head pointer, tail pointer, and slot mask—each with optimal section placement and alignment for multicore SPSC/MPMC work-stealing patterns.

## Grammar

```
@ring(slots=<pow2>, slot_size=<u32>)
```

Where:
- `slots`: positive power-of-2 integer (typically 64, 256, 4096)
- `slot_size`: size of each slot in bytes (typically 8, 16, 32, 64)

Example:
```paideia
pub let mut rx_ring : u64 = uninit @ring(slots=64, slot_size=32)
```

## Synthesis Contract

When the parser encounters `@ring(slots=M, slot_size=K)`, the elaborator synthesizes **four symbols** from the original binding name:

| Symbol | Section | Size | Alignment | Initial Value |
|--------|---------|------|-----------|----------------|
| `<name>_slots` | `.bss` | `M × K` | 64 | uninitialized |
| `<name>_head` | `.data` | 8 | 8 | 0 |
| `<name>_tail` | `.data` | 8 | 8 | 0 |
| `<name>_mask` | `.rodata` | 8 | 8 | `M - 1` (little-endian u64) |

### Why This Layout?

- **`_slots` (`.bss`, align=64):** The ring buffer itself is uninitialized contiguous storage, cache-line aligned to prevent false sharing.
- **`_head` / `_tail` (`.data`, align=8):** Writable index pointers, placed in `.data` so each consumer/producer pair can have independent cache-line residency.
- **`_mask` (`.rodata`, align=8):** Immutable bit mask (slots − 1) for fast index modulo via bitwise AND. Placed in `.rodata` since it never changes and may be accessed from multiple cores.

### Path Constraints

- Slots must be a power of 2 (validated during parsing; diagnostics P0253).
- Slot size must be positive (validated during parsing; diagnostics P0260).
- Only one `@ring` directive per binding (validated during parsing; diagnostics P0261).
- Ring buffers are always marked `pub` for global access.

## Rationale

Ring buffers are the canonical synchronization primitive for lock-free work-stealing and producer–consumer patterns in multicore kernels. The synthesis approach:

1. **Avoids API bloat:** No distinct `ring_t` type; use plain `let mut` with a focused attribute.
2. **Enforces correct alignment:** Cache-line alignment prevents false-sharing bugs.
3. **Immutable mask constant:** The rodata placement of `_mask` allows speculative prefetch from multiple cores without coherence traffic.
4. **Extensible:** Future variants (`@ring(..., mode=SPSC)`, `@ring(..., mode=MPMC)`) can scale to fair-queueing or concurrent work-stealing without breaking the core synthesis.

## Diagnostics

| Code | Level | Message |
|------|-------|---------|
| P0253 | Error | `@ring` slots parameter must be a positive power of 2 |
| P0260 | Error | `@ring` slot_size parameter must be positive |
| P0261 | Error | Duplicate `@ring` directive on single binding |

## First-One Precedent

This is the first 1→N attribute synthesis (one AST binding → multiple data symbols). Future candidate attributes: `@allocator`, `@lazy_static`, `@tls`. The infrastructure is designed to scale.

## Example

Input:
```paideia
pub let mut rx_buffer : u64 = uninit @ring(slots=256, slot_size=16)
```

Output (ELF symbol table):
```
rx_buffer_slots   .bss   size=4096   align=64   global
rx_buffer_head    .data  size=8      align=8    global
rx_buffer_tail    .data  size=8      align=8    global
rx_buffer_mask    .rodata size=8     align=8    global (value: 0xff)
```

Kernel code reads/writes `rx_buffer_head` and `rx_buffer_tail` to manage the ring, and masks index calculations with `rx_buffer_mask` via AMD64 register addressing modes.
