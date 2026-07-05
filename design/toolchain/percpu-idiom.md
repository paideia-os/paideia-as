# Per-CPU Counter Idiom (Issue #965)

## Overview

Per-CPU counters live at fixed offsets in the GS segment, which is per-CPU kernel memory set up at boot via `wrmsr(MSR_GS_BASE, &percpu_block[core_id])`. The GS segment base points to a per-CPU control block containing scheduler state, network stack counters, and other core-local data.

The `percpu_inc` and `percpu_add` operations enable atomic increment and add-by-N on these counters using the x86_64 `LOCK` prefix, which atomically reads, modifies, and writes memory in a single instruction, preventing concurrent threads from observing intermediate states.

## Motivation

1. **Lock-free counters**: Network stack and scheduler need atomic counters without spinning on locks. The x86 `lock inc gs:[rip+disp32]` idiom is the canonical primitive for single-instruction atomic increment.
2. **Per-core isolation**: Each core reads/writes its own counter in its own GS segment, eliminating cache coherency traffic across cores.
3. **Existing hardware primitive**: x86_64 LOCK prefix on memory operations provides atomicity via internal MESI coherency; no software spinlock overhead.

## Interface

Trait `PerCpuOps` in `paideia-stdlib/pdx/percpu.pdx`:

```paideia
trait PerCpuOps {
  fn percpu_inc(counter_gs_offset: u64) -> () !{Atomic, RawMem} @{paideia.raw_mem};
  fn percpu_add(counter_gs_offset: u64, val: u64) -> () !{Atomic, RawMem} @{paideia.raw_mem};
}
```

**Arguments:**
- `counter_gs_offset: u64` — offset within the per-CPU control block, typically a linker-resolved symbol offset (e.g., `offset_of(PerCpuBlock, sched_ticks)`)

**Returns:** `()` (unit; side effect is the atomic memory operation)

**Effect:** `!{Atomic, RawMem}`
- `Atomic`: The operation is atomic; no interleaving with concurrent memory operations on this location.
- `RawMem`: Raw memory access; bypasses allocation/borrow tracking.

**Capability:** `@{paideia.raw_mem}` — requires raw-memory access permission

## Planned Lowering

Codegen lowering (deferred to a future issue if not already supported):

### `percpu_inc(offset)`

Lowers to: `LOCK + GS-override + INC [disp32]`

**x86_64 encoding:**
```
F0 65 FF 04 25 <disp32_le>
│  │  │  └─ SIB: scaled indirect addressing (00 = no scale, 100 = ESP/RSP base, 101 = RIP-relative)
│  │  └─ FF /0 (INC r/m64)
│  └─ 65 (GS segment override prefix)
└─ F0 (LOCK prefix)
```

Or with explicit addressing mode for direct offset:
```
F0 65 FF 05 <offset_le32>    (INC gs:[rip+offset])
```

This atomically reads the 64-bit counter at `gs_base + offset`, increments it, and writes it back—all without releasing the lock.

### `percpu_add(offset, val)`

Lowers to: `LOCK + GS-override + ADD [disp32], imm`

**x86_64 encoding:**
```
F0 65 81 04 25 <disp32_le> <imm32_le>
│  │  │  │
│  │  │  └─ SIB / addressing
│  │  └─ 81 /0 (ADD r/m64, imm32 sign-extended to 64-bit)
│  └─ 65 (GS segment override)
└─ F0 (LOCK prefix)
```

**Prefix order:** `F0 (LOCK) → 65 (GS) → REX → opcode`

## Semantics

Both operations are **sequentially consistent** with respect to other atomic operations on the same counter:
- Multiple cores incrementing the same counter (e.g., via memory barrier or synchronization) will see updates in order.
- A thread reading the counter after `percpu_inc` observes the increment.

## Effect and Capability

| Term | Meaning |
|------|---------|
| `!{Atomic, RawMem}` | This operation is atomic and accesses raw memory. |
| `@{paideia.raw_mem}` | Requires the caller to hold the `paideia.raw_mem` capability. |

Rationale: Raw memory access requires explicit capability grant to prevent accidental use in high-level code. Atomic operations are guaranteed by the hardware LOCK prefix and do not require separate effect.

## Status

- **Phase 15 m1** (#965): Trait interface + design doc + fixture parse tests ✓
- **Phase 15 m2+**: Codegen lowering (if encoder support for GS segment override exists; otherwise encoder primitive backtrack issue)

The macro form (`percpu_inc!(counter_name)` expanding to `percpu_inc(offset_of(PerCpuBlock, counter_name))`) becomes idiomatic once macros land; the underlying capability is available now via the trait signature.

## Test Fixtures

Parse tests verify syntax and basic elaboration:
- `percpu_inc_single.pdx` — single-function trait for `percpu_inc`
- `percpu_add_single.pdx` — single-function trait for `percpu_add`
- `percpu_inc_zero_offset.pdx` — edge case: offset = 0 (base of control block)
- `percpu_add_various_offsets.pdx` — two-function trait exercising multiple offsets

## References

- Issue #965 (parent): Per-CPU counter idiom
- Related: #967 (per-CPU control block layout)
- x86_64 LOCK prefix semantics: Intel SDM Vol. 2A, section 3.2
- GS segment setup: paideia-os boot sequence (UEFI/firmware handoff)
