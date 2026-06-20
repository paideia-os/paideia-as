# Pointer types (Phase 3 m1)

**Status:** Phase 3 m1 closure appendix.
**Scope:** Documents the pointer surface area added across m1-001..012, the design decisions, and the deferrals.

## 0. Origin

The original specs `design/toolchain/syntax-reference.md` and
`design/toolchain/custom-assembler.md` live upstream in the
`paideia-os/paideia-os` repository — this paideia-as repo doesn't ship
them locally. The Phase 3 plan's m1-013 AC ("one-line update in
custom-assembler.md §6.1 indexing the appendix") therefore lands as a
follow-up PR on the upstream repo. This document is the appendix the
upstream cross-reference will point at.

## 1. Grammar — `*T`

`*T` is a raw pointer to a value of type `T`. The grammar is a prefix
operator on the type level:

```
Type ::=
  | '*' Type                -- m1-001 / paideia-as parser parse_type.rs
  | ... (existing forms)
```

Precedence: `*` binds tighter than `forall` and the linearity class
prefix. So `*forall e. T` is rejected (P0195 or a forall-position
diagnostic). `(*u8) -> u64` parses as a function taking a raw byte
pointer. `*(u64 -> u64)` is a pointer to a function-typed value.

Phase 3 ships **only** `*T`. The borrowed-reference forms `&T` and
`&mut T` are explicitly deferred — see §5.

## 2. Substructural class

`*T` has class `Unrestricted` regardless of the pointee's class.

The rationale (m1-003): a raw pointer is a number. Copying it is free;
the linearity discipline lives on whatever the pointer is dereferenced
into, not on the pointer value itself. The capability and effect
systems (RawMem + `paideia.raw_mem`) carry the read/write contract;
the linearity system carries the move/copy contract for typed values.

This is a deliberate split. Borrowed references (`&T`, `&mut T`) would
re-introduce a borrow-checker-style discipline on the pointer
*handle*; raw pointers don't, and that's what makes the m1 surface
small.

## 3. Intrinsic families

### 3.1 `index_*` (m1-004)

10 widths × 2 directions = 20 intrinsics:

```
index_u8(p: *u8, i: u64) -> u8 !{RawMem} @{paideia.raw_mem}
index_u16(p: *u16, i: u64) -> u16 !{RawMem} @{paideia.raw_mem}
...
index_f64(p: *f64, i: u64) -> f64 !{RawMem} @{paideia.raw_mem}

index_u8_set(p: *u8, i: u64, v: u8) -> () !{RawMem} @{paideia.raw_mem}
...
index_f64_set(p: *f64, i: u64, v: f64) -> () !{RawMem} @{paideia.raw_mem}
```

Per-width register sizing: the encoder (m1-007) emits the canonical
AMD64 SIB-form load/store sequence:

- width 1: `mov al,  [rdi + rcx]`     → `8a 04 0f`
- width 2: `mov ax,  [rdi + rcx * 2]` → `66 8b 04 4f`
- width 4: `mov eax, [rdi + rcx * 4]` → `8b 04 8f`
- width 8: `mov rax, [rdi + rcx * 8]` → `48 8b 04 cf`

Movzx / movsx variants for sign-extension and zero-extension to r64
will ship in a follow-up; phase 3 m1 emits to the canonically-sized
destination register (AL / AX / EAX / RAX) with implicit 32-bit-dest
zero-extension where applicable.

### 3.2 `ptr_sub*` (m1-011)

10 widths × 2 directions = 20 intrinsics:

```
ptr_sub_u8(a: *u8, b: *u8) -> u64        -- element-distance
ptr_sub_bytes_u8(a: *u8, b: *u8) -> u64  -- byte-distance
... (each width)
```

`ptr_sub_T(a, b)` returns `(a - b) / sizeof(T)` — the element count
between two pointers. `ptr_sub_bytes_T(a, b)` returns the raw byte
distance.

Effect / capability rows are **empty**. Subtraction is a register-only
operation; no memory is accessed.

Encoder (m1-011):
- `sub rax, rdi` → `48 29 f8` for the subtraction.
- `sar rax, imm8` → `48 c1 f8 <imm>` for the per-width shift when
  emitting the element-distance form (no shift for the bytes form or
  width 1 element form).

## 4. RawMem effect + `paideia.raw_mem` capability

### 4.1 RawMem (m1-005)

Declared at the prelude (`src/toolchain/abi/abi.pdx` §0). No explicit
operations — RawMem is the built-in effect carried by any `*T`
dereference. Intrinsic functions (`index_*`, `index_*_set`, future
`ptr_load_*` / `ptr_store_*`) implicitly perform it.

User code that wants to call raw-memory intrinsics must declare
`!{RawMem}` in the effect row. Calling outside a `!{RawMem}` row
will fire F1100 once the elaborator chokepoint (m1-013+) wires
intrinsic-call resolution into the effect walker.

### 4.2 `paideia.raw_mem` (m1-005)

Registered as a built-in dotted capability in
`crates/paideia-as-effects/src/capabilities.rs`. User code references
it as `@{paideia.raw_mem}` in capability rows.

The choice to require an explicit capability (rather than making raw
memory access universally available) is a Phase 3 m1 commitment to
the principle that *every* exotic side-channel — even one as
universal as memory — surfaces in the type system. Modules that
genuinely never touch raw memory will compile without ever mentioning
RawMem or `paideia.raw_mem`.

## 5. Deferred: borrowed references

`&T` and `&mut T` are **not** part of Phase 3.

The reasoning (`design/toolchain/phase-3-plan.md` §15 open question):
borrowed references require a region calculus + a borrow checker
pass. Borrow checking is a multi-milestone effort — it would
single-handedly fill or exceed the scope of m1. Phase 3 m1 instead
ships the minimal surface (`*T`) that retires the most common Phase 2
unsafe-block usage (see examples 16_memcpy and 17_strlen) without
re-architecting the type system.

The reserved diagnostic `T0511` (m1-003) is a placeholder for the
future borrowed-reference codepath. T0511 has no emit site today;
the catalog entry only reserves the code so the T05xx region stays
contiguous.

## 6. Examples impact

- `examples/15_sum_array.pdx`: `Array` placeholder retired; `*u64`
  is the array head. `read_index` stub deleted; body calls
  `index_u64(xs, i)`. (m1-008)
- `examples/16_memcpy.pdx`: `unsafe { }` block shrinks from 4
  instructions to 1 (`rep movsb` only); local `MemCopy` effect
  retired in favour of prelude `RawMem`; signature is
  `(*u8, *u8, u64) -> *u8 !{RawMem} @{paideia.raw_mem}`. (m1-009)
- `examples/17_strlen.pdx`: `unsafe { }` block fully retired; the
  byte read goes through `index_u8`; the cursor difference goes
  through `ptr_sub_bytes`. Pure tail-recursive scan over `*u8`.
  (m1-010)

The corpus regression test
`tests/end-to-end/tests/examples_compile.rs` (m1-012) gates on these
examples flipping to "compiles end-to-end" status, which activates
when the elaborator's intrinsic-call resolution chokepoint lands
(m1-013+).

## 7. Forward links

- Borrowed-reference work: deferred to a future phase (no plan yet).
- Movzx / movsx encoder variants: follow-up to m1-007.
- `ptr_add_*` / `ptr_add_bytes_*` intrinsic families: follow-up to
  m1-011 (sibling of `ptr_sub*`).
- Elaborator call-resolution chokepoint: the load-bearing
  prerequisite for full activation of m1-004 through m1-007 and
  m1-011 effect / capability enforcement. Tracked in m1-013+.
- Upstream `custom-assembler.md` §6.1 cross-reference: follow-up PR
  on `paideia-os/paideia-os`.
