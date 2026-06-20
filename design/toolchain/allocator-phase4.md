# Allocator + memory model (Phase 4 m10)

**Status:** Phase 4 m10 closure appendix.
**Scope:** Documents the `Allocator` trait, the three backends (BumpAllocator, Arena, SystemAllocator), `Box<T>` semantics, and the Q3 default resolution.

## 0. Q3 resolution: allocator default

The Phase 4 plan's §15 Q3 open question asked which allocator should be the default. **Decision: dual default.**

- **PaideiaOS targets**: **Arena** (m10-003). The all-assembly kernel has no libc to wrap; linear bump is too restrictive for kernel-subsystem lifecycles; arena models "alloc within a subsystem lifecycle, reclaim all on scope-end" cleanly.
- **Non-PaideiaOS / host targets** (paideia-as the tool, test harnesses): **SystemAllocator** (m10-004). glibc / musl exists; standard malloc/free works.

Both defaults inherit the same `Allocator` trait (m10-001), so allocator-generic user code works across both targets. The target-OS gate routes the choice at compile time.

This dual-default is honest about the constraint difference between bare-metal kernel and hosted process. A single default would force one side to suffer.

## 1. The Allocator trait (m10-001)

`crates/paideia-stdlib/pdx/alloc.pdx`:

```paideia
record Layout { size: u64, align: u64 }

trait Allocator {
  fn alloc(self: &mut Self, layout: Layout) -> *u8 !{RawMem} @{paideia.raw_mem};
  fn dealloc(self: &mut Self, ptr: *u8, layout: Layout) -> () !{RawMem} @{paideia.raw_mem};
}
```

The `self: &mut Self` form gates on Phase 4 m4-m6 (borrow checker). Phase 4 m10-001 ships the surface; per-allocator impls move `Self` through the call until the borrow checker activates the in-place mutation idiom.

`!{RawMem}` and `@{paideia.raw_mem}` carry the contract from Phase 3 m1: every raw-memory operation surfaces in the effect + capability system.

## 2. BumpAllocator (m10-002)

`crates/paideia-stdlib/pdx/bump.pdx`. Single-region bump allocator: a base pointer + monotonically-increasing offset. `reset()` returns offset to 0; `dealloc` is a no-op.

Linearity: the `BumpAllocator` is itself `linear` — must be `reset`-ed OR dropped before scope-end. Phase 4 m10-002 minimum documents this via the type system; full linearity enforcement gates on m4-m6 borrow-checker.

Use case: very-short-lived allocations within a single scope.

## 3. Arena (m10-003)

`crates/paideia-stdlib/pdx/arena.pdx`. Multi-region linked list. When a region fills, the arena allocates a new region and links it. `reset()` releases all regions.

Use case: subsystem-lifecycle allocations (per-process memory, per-IPC-session buffers, per-compilation-unit ASTs). **Default for PaideiaOS targets.**

## 4. SystemAllocator (m10-004)

`crates/paideia-stdlib/pdx/system_alloc.pdx`. Wraps libc `malloc` / `free` via FFI. Gated to host targets via `#[cfg(target_os = ...)]`. PaideiaOS itself does NOT use this allocator; calling it from a PaideiaOS kernel binary fires `C1401 "SystemAllocator unavailable on PaideiaOS targets"` at link time.

Use case: paideia-as the tool, test harnesses, dev-host tooling.

## 5. Box<T> (m10-005)

`crates/paideia-stdlib/pdx/box.pdx`. `record Box<T> { ptr: *T }` with three helpers:

- `box_new(value)` heap-allocates one `T` via the ambient allocator.
- `box_deref(b)` returns the underlying `T` value.
- `box_drop(b: linear Box<T>)` releases the underlying pointer.

The `linear` qualifier ensures `box_drop` is called before scope-end (or the value is moved). Syntax sugar (`box value` / `*b`) is deferred per the m10-005 honest minimum; today user code uses the explicit function-call form.

## 6. The "ambient allocator" pattern

Allocator-generic code uses the ambient allocator without naming it. The ambient is determined at compile-time by the cfg target:

- `target_os = "linux"` (or `darwin` / `windows`) → SystemAllocator.
- `target_os = "paideia-os"` → Arena.
- Otherwise → fail at compile time with C1402 "no ambient allocator defined for target".

Phase 4 m10-006 honest minimum: pattern documented; runtime threading of the ambient through allocator-generic call sites gates on m9 monomorphisation pass activation (m9-006) + the m1 walker chokepoint.

## 7. Diagnostic catalog

| Code  | Severity | Title                                             | Range |
|-------|----------|---------------------------------------------------|-------|
| C1401 | error    | SystemAllocator unavailable on PaideiaOS targets  | cap   |
| C1402 | error    | No ambient allocator defined for target           | cap   |

Both lie in the C category (capability discipline; range 1300-1499) because allocation is a capability-mediated operation in the paideia-as model.

## 8. Corpus

14 fixtures across the 4 .pdx modules in `crates/paideia-stdlib/pdx/`:

- bump (4): bump_new_creates_zero_offset / alloc_advances_offset / alloc_respects_alignment / reset_returns_offset_zero.
- arena (4): arena_new_creates_arena / alloc_returns_pointer / multi_region / reset_releases_all.
- system_alloc (2): system_alloc_decl / system_alloc_in_linux_block.
- box (4): box_new_returns_box / deref_returns_inner / linear_discipline / drop_releases_pointer.

Test scaffolding in `crates/paideia-stdlib/tests/parse_pdx.rs` exercises each via `paideia-as check`.

## 9. Deferred to m11 / later

- **Vec<T> / HashMap / BTreeMap**: depend on Box<T> + an allocator. Phase 4 m11 stdlib bring-up.
- **Slab allocator**: for fixed-size object pools (kernel ringbuf entries, process descriptors). Future m10 follow-up.
- **Per-CPU allocators**: NUMA-aware allocator design. Phase 5+ territory.
- **Memory-protection regions**: capability-mediated page-level permissions. Distinct work track.
- **Allocator-generic stdlib**: every stdlib container takes an `A: Allocator` parameter. m11 design call.

## 10. Forward links

- **m11 stdlib bring-up**: Box<T> + Vec<T> + HashMap<K, V> + String depend on m10.
- **m1 walker hookups**: activates the elaborator-side wiring for ambient-allocator threading.
- **m4-m6 borrow stack**: activates `&mut Self` for the Allocator trait; current move-Self workaround retires.
- **PaideiaOS m1**: the first kernel subsystem written in paideia-as uses Arena per the dual-default.
