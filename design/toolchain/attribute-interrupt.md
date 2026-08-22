# @interrupt / @interrupt_error ISR-entry sugar

**Issue:** [paideia-as#1278](https://github.com/PaideiaOS/paideia-as/issues/1278)
(phase 1 — parser + AST/IR plumbing) +
[paideia-as#1300](https://github.com/PaideiaOS/paideia-as/issues/1300)
(phase 2 — elaborator emit synthesis)
**Phase:** paideia-as v0.21-002 (unblocks paideia-os R18 IPI vector handlers)
**Status:** Phase-2 landed — elaborator synthesises spill / iretq / errcode-skip
around every interrupt-marked lambda.

## Motivation

Every ISR entry stub in paideia-os today is ~50 hand-written LoC in
`src/kernel/core/int/isr_trampoline.pdx`: 13 pushes to spill the caller-saved
GPRs, `cld` to normalise the direction flag, a `call` into the Rust-shaped
dispatch function, 13 pops in the reverse order, an optional `add rsp, 8` to
skip the CPU-pushed error code on vectors 8/10/11/12/13/14/17/21/29/30, and
finally `iretq`. That is 32+ vectors × 50 LoC of boilerplate that must all
agree byte-for-byte on the spill order and the errcode-skip decision.

`@interrupt(vec)` / `@interrupt_error(vec)` collapse that boilerplate into a
single symbol attribute on a `pub let` binding. The elaborator wraps the
lambda body with the exact push/pop/iretq sequence, and phase-1's
`CANONICAL_VECTORS` table (parser) resolves symbolic vector names —
`"page_fault"`, `"tlb_shootdown_ipi"`, etc. — to their numeric x86_64
identity.

## Syntax

```pdx
// No error code (vectors 0-7, 9, 16, 18-20, 32+):
pub let isr_timer : () -> () !{sysreg} @{} =
  fn () -> unsafe {
    effects: { sysreg },
    capabilities: {},
    justification: "APIC timer tick — bump counter, EOI, iretq.",
    block: {
      // handler body — no push/pop, no iretq; elaborator adds them.
      mov rax, [rip + timer_ticks];
      add rax, 1;
      mov [rip + timer_ticks], rax;
      // ... eoi write ...
    }
  } @interrupt("apic_timer")

// Error-code vector (vectors 8, 10, 11, 12, 13, 14, 17, 21, 29, 30):
pub let isr_pf : () -> () !{sysreg} @{} =
  fn () -> unsafe {
    effects: { sysreg },
    capabilities: {},
    justification: "Page fault — read CR2, dispatch to fault handler.",
    block: {
      // handler body reads CR2 for fault address, dispatches, returns.
    }
  } @interrupt_error("page_fault")
```

The `_error` suffix is not a value — it is a distinct attribute name.
Vectors that push an error code use `@interrupt_error(...)`; every other
vector uses `@interrupt(...)`. Passing the wrong form is a silent
correctness bug (the tail's `iretq` would either pop the errcode as RIP or
leave a stack imbalance for the interrupted context), so choose from Intel
SDM Vol. 3A §6.15 with care. The canonical name table
(`crates/paideia-as-parser/src/parse_item/let_item.rs::CANONICAL_VECTORS`)
records the errcode discipline for the named vectors — a follow-up phase can
cross-check `@interrupt` vs `@interrupt_error` against the canonical table
to elevate this to a compile-time diagnostic.

Numeric spellings (`@interrupt("42")`, `@interrupt_error("128")`) accept any
value in `0..=255` and are the escape hatch for LAPIC / IPI / user-defined
vectors not in the canonical table.

## Semantics

The elaborator synthesises the following around every lambda body marked as
an interrupt handler:

**Prologue** (13 GPR pushes + `cld`, emitted at Lambda-visit entry):

```asm
push rax; push rcx; push rdx; push rsi; push rdi;
push r8;  push r9;  push r10; push r11;
push r12; push r13; push r14; push r15;
cld
```

**Body** — the elaborator's normal lambda emit for the body (unsafe raw asm
in the common case; a lowered Rust-shaped body for the rare non-unsafe
case). The body **must not** contain its own `ret` or `iretq` — the
elaborator emits the exit tail below and any user-written return would land
before the pop chain, leaking the spilled GPRs onto the stack.

**Epilogue** (13 GPR pops in reverse + optional errcode skip + `iretq`,
emitted by the `emit_interrupt_epilogues` post-pass):

```asm
pop r15; pop r14; pop r13; pop r12;
pop r11; pop r10; pop r9;  pop r8;
pop rdi; pop rsi; pop rdx; pop rcx; pop rax;
// If has_error_code (from @interrupt_error):
add rsp, 8
iretq
```

`@interrupt(...)` and `@interrupt_error(...)` implicitly imply `@no_frame`
(phase-1 `lower.rs::populate_let_meta` stamps `LetInfo::no_frame = true`
alongside `LetInfo::interrupt = Some(...)`). The two flags are consulted
independently by the emit pass — the ISR spill fills the stack-frame
niche, and the elaborator's default `push rbp; mov rbp, rsp` /
`mov rsp, rbp; pop rbp` pair is suppressed.

### Spilled-register set

The 13-register set covers every GPR *except* `rbp`, `rsp`, and `rbx`.
These three are the SysV callee-saved subset the interrupted code is
entitled to have transparently preserved — an ISR body that touches them
must save/restore them inside its own body. Pushing `rsp` would smash the
just-saved value (`push rsp` semantics push a decremented rsp on x86_64,
not the pre-instruction value); pushing `rbp` on entry with a matching pop
would let dynamic-frame walkers see the interrupted frame chain, which is
what the `@no_frame` implication is meant to prevent.

### Instruction ordering

The prologue's 13 pushes and `cld` allocate the lowest emission_order
values inside the function (they run before any body dispatch). The body's
instructions — whether emitted by the elaborator's own arm dispatch or by
`UnsafeWalker` — take the next range. The epilogue post-pass runs after
`emit_pending_unsafe_bodies` and its instructions take the highest range.
The text emitter sorts by `(emission_order, node_id)`, so the resulting
.text layout is deterministically prologue-then-body-then-epilogue for
every interrupt lambda regardless of body shape.

## Restrictions

### Function-only placement (P0250)

The parser rejects `@interrupt(...)` on non-lambda bindings with **P0250**
(the same diagnostic `@no_frame` uses for its own placement gate). An ISR
entry stub only makes sense wrapping a callable.

### Unknown vector name (P0291)

The parser rejects `@interrupt("blorp")` — a name absent from the canonical
table AND not parseable as a `0..=255` numeric — with **P0291**.

### Out-of-range numeric (P0291)

`@interrupt("256")` is also **P0291**; vector numbers are exactly `0..=255`.

### Malformed argument syntax (P0290)

`@interrupt` without `("...")`, or with a non-string-literal argument, is
**P0290**.

## paideia-os migration path

Every hand-rolled ISR trampoline in
`paideia-os/src/kernel/core/int/isr_trampoline.pdx` should migrate to
`@interrupt(vec)` / `@interrupt_error(vec)` in the following order (each is
independently landable, no cross-vector coupling):

1. **LAPIC vectors** — `apic_timer` (240), `apic_spurious` (241),
   `tlb_shootdown_ipi` (242). These are the R18 IPI vectors the phase-2
   emit unblocks; their spill discipline is trivial (no errcode) and the
   body is dispatch-only.
2. **Legacy IRQ vectors** — `timer` (32), `keyboard` (33), `cascade` (34),
   `com1` (36). Same shape as LAPIC vectors, still no errcode.
3. **Non-errcode exceptions** — `divide_error` (0), `debug` (1), `nmi`
   (2), `breakpoint` (3), `overflow` (4), `bound_range` (5),
   `invalid_opcode` (6), `device_not_available` (7), `x87_fp` (16),
   `machine_check` (18), `simd_fp` (19), `virtualization` (20),
   `hypervisor_injection` (28).
4. **Errcode exceptions** — `double_fault` (8), `invalid_tss` (10),
   `segment_not_present` (11), `stack_segment_fault` (12),
   `general_protection` (13), `page_fault` (14), `alignment_check` (17),
   `control_protection` (21), `vmm_communication` (29), `security` (30).
   Use `@interrupt_error(...)` for these — the elaborator emits the
   matching `add rsp, 8` before `iretq`.

Each migration is a five-line patch: strip the 50 LoC hand-rolled
trampoline, replace with the `@interrupt(vec)` sugar on a `pub let`
wrapping the dispatch call. The IDT population code (also in
`isr_trampoline.pdx`) is unchanged — the ISR symbol names still resolve.

## References

- Intel SDM Vol. 3A §6.15 — exception vectors and error-code discipline
- `crates/paideia-as-parser/src/parse_item/let_item.rs::CANONICAL_VECTORS`
  — canonical name → vector-number table
- `crates/paideia-as-elaborator/src/emit_walker.rs::emit_interrupt_prologue`
  — 13-push spill + `cld` prologue emission
- `crates/paideia-as-elaborator/src/emit_walker.rs::emit_interrupt_epilogue`
  — 13-pop restore + optional errcode-skip + `iretq` epilogue emission
- `crates/paideia-as-elaborator/src/emit_walker.rs::emit_interrupt_epilogues`
  — the post-pass that wires the epilogue in for every interrupt-marked lambda
