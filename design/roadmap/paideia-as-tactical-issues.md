# paideia-as tactical roadmap: v0.13 through v0.20

**Status:** Planning document (softarch), 2026-07-04. Tactical per-issue
breakdown for eight consecutive paideia-as releases spanning paideia-os
phases 5 through 14.

**Companion:** `osarch` produces the strategic release map (paideia-os side).
This document is the tactical mirror — the paideia-as issues that must land
for each osarch release to have a language surface to compile against.

**Not-a-plan warning:** This is a design document only. It does **not**
create GitHub milestones or issues. Numbering here is provisional (`pa-r13-*`
through `pa-r20-*`); real issue numbers are minted when the release opens.

---

## 0. Table of contents

1. Executive summary and release table
2. Aggregate encoder gap forecast (chronological)
3. Release **v0.13** — R14 encoder-completion round
4. Release **v0.14** — Driver substrate: MMIO, function pointers, ring buffers
5. Release **v0.15** — Network primitives: bit ops, checksum helpers, byte parsing
6. Release **v0.16** — CoW FS + atomics: locked bit ops, refcount atomics, cmpxchg16b
7. Release **v0.17** — Function pointer types + records elaboration depth
8. Release **v0.18** — Semantic terminal: hash tables, closures, pattern-match depth
9. Release **v0.19** — UEFI: MS x64 calling convention, GUID literals, embed primitives
10. Release **v0.20** — Self-hosting shape: paideia-as as runtime library, dynamic emit
11. Calling-convention formalization (cross-release)
12. Function-pointer type system (cross-release)
13. Records / discriminated unions (cross-release)
14. Embed primitives (`@include_bytes`, `@include_str`, `@link_section`)
15. Regression discipline: v0.12+ API stability contract for paideia-os
16. Cross-release dependency graph
17. Testing strategy (round-trip corpus + fuzz layers)
18. Top-10 highest-impact issues

---

## 1. Executive summary and release table

The paideia-as v0.12 → v0.20 arc closes three qualitative gaps that paideia-os
depends on over its phase-5-through-14 build-out:

- **Encoder completeness.** Every workaround discovered during R13/R14 (ud2,
  dec, three-op imul, cld, test-with-imm, or-imm64, rep_movsb/stosq
  robustness, bts/btr/btc) is retired by v0.15. Post-v0.15, paideia-os stops
  authoring GAS shims for x86_64 primitives.
- **Language surface.** Function pointers become first-class (`v0.17`), records
  and discriminated unions elaborate through pattern-match without opting into
  `unsafe { ... }` (v0.17 + v0.18), and closures (`v0.18`) unblock the
  semantic-terminal command surface.
- **ABI plurality.** MS x64 (UEFI) and dynamic-emit (WASM/VM userspace)
  land in v0.19 and v0.20 respectively.

### 1.1 Release-to-phase mapping

| paideia-as | paideia-os phase(s) unblocked | Theme                                       | Issues |
|------------|-------------------------------|---------------------------------------------|--------|
| v0.13      | R14 close, R15 open           | R14 encoder-workaround retirement           | 14     |
| v0.14      | Phase 5, Phase 6, Phase 7     | Driver substrate (MMIO, fnptr, rings)       | 12     |
| v0.15      | Phase 8                       | Network primitives (bit-ops, checksums)     | 11     |
| v0.16      | Phase 9                       | CoW FS + atomics (locked bit-ops, DCAS)     | 12     |
| v0.17      | Phase 10, Phase 11 pre-lift   | Function-ptr types + records elab depth     | 15     |
| v0.18      | Phase 11                      | Semantic terminal (hash tables, closures)   | 12     |
| v0.19      | Phase 12                      | UEFI: MS x64 ABI + GUID + embed primitives  | 13     |
| v0.20      | Phase 13, Phase 14 pre-lift   | Self-hosting shape (dynamic emit, PQ audit) | 10     |
| **Total**  |                               |                                             | **99** |

### 1.2 Exit-criteria highlights

- **v0.13** — zero `paideia-os R14` workarounds remain on the "known-blocking"
  list; `find-paideia-as.sh` remains strict; the R14B autonomous loop compiles
  without any GAS-shim escape hatch except the ones already tracked
  (boot_stub.S, embed primitives).
- **v0.15** — paideia-os `net/` compiles fully in `.pdx`; no C helper for
  IPv4/TCP header parsing.
- **v0.17** — paideia-os `fs/vfs/vops.pdx` declares its dispatch table as
  `let vops : record { read: (*File, *u8, u64) -> i64, write: ... } = ...`
  with no `unsafe { }` block.
- **v0.20** — paideia-as ships a `libpaideia-as-runtime` crate suitable for
  linking into paideia-os processes; the WASM jail invokes it with a code
  buffer and a symbol table.

### 1.3 Non-goals for this arc

- **Self-hosting the CLI on paideia-os.** The runtime library ships in v0.20;
  the CLI (`paideia-as build`) remains a Rust binary until Phase 14 (paideia-as
  in-tree). Full self-hosting is a **v0.21+** concern.
- **Alternate targets.** RISC-V, ARM64 are out of scope through v0.20. The
  Mnemonic enum is deliberately kept flat and x86_64-shaped until the
  encoder rewrite in the v0.21 arc.
- **Formal verification.** Only mechanical property tests (iced-x86
  round-trip, encoder byte-exact fixtures, elaborator diagnostic-code
  regressions) are contemplated. A machine-checked encoder spec (Coq / Lean)
  is a "post-v0.20" concern.

---

## 2. Aggregate encoder gap forecast (chronological)

Every encoder gap fill planned across v0.13 → v0.20, ordered by the release
that lands it and annotated with the paideia-os consumer it unblocks. When an
item duplicates an already-filed paideia-as issue number, that number is
carried through so the tactical breakdown collapses onto the existing tracker.

### 2.1 v0.13 — the R14 workaround-retirement wave

| Issue                                  | Encoder shape                     | paideia-os unblock              |
|----------------------------------------|-----------------------------------|---------------------------------|
| pa-r13-001 = **#927**                  | `mov r8/r16/r32, [mem]` narrow    | signal.c / kind_signal handler  |
| pa-r13-002 = **#928**                  | REX.B on `mov r64, [ext_base+…]`  | phys_free bitmap word load      |
| pa-r13-003 = **#929**                  | `call [mem]` + `call reg`         | vops indirect dispatch          |
| pa-r13-004                             | `ud2` (0F 0B)                     | enter_userland_initial trap tail|
| pa-r13-005                             | `dec r64` (REX.W FF /1)           | loop counters                   |
| pa-r13-006                             | `test r64, imm32` (REX.W F7 /0)   | flag-word check paths           |
| pa-r13-007                             | `test r/m64, r64` reg-reg form    | zero-check without land+cmp     |
| pa-r13-008                             | `cld` (0xFC)                      | kernel-entry DF=0 explicit      |
| pa-r13-009                             | `imul r64, r/m64, imm8/imm32`     | index scaling in loops          |
| pa-r13-010                             | `or r64, imm64` (via movabs+or)   | large-mask bit ops              |
| pa-r13-011                             | `rep_movsb` robustness pass       | elf_lite_load bulk copy         |
| pa-r13-012                             | `rep_stosq` robustness pass       | zero-fill new pages             |
| pa-r13-013                             | `sete/setne/set*` r8              | boolean lowering                |
| pa-r13-014                             | `bswap r64` (REX.W 0F C8+rd)      | network byte order (early)      |

### 2.2 v0.14 — driver substrate

| Issue      | Encoder shape                           | paideia-os unblock              |
|------------|-----------------------------------------|---------------------------------|
| pa-r14-001 | `mov [mem], imm32/imm8` narrow widths   | MMIO register store             |
| pa-r14-002 | `mov r32, [mem]` MMIO load form full    | AHCI FIS ring reads             |
| pa-r14-003 | `movnti [mem], r32/r64`                 | write-combining ring flush      |
| pa-r14-004 | `sfence` / `lfence`                     | MMIO ordering                   |
| pa-r14-005 | `wbinvd`, `invd`, `clflush [mem]`       | cache flush around DMA          |
| pa-r14-006 | `prefetch` family (0F 18 /0..3)         | ring-buffer producer/consumer   |

### 2.3 v0.15 — network primitives

| Issue      | Encoder shape                              | paideia-os unblock              |
|------------|--------------------------------------------|---------------------------------|
| pa-r15-001 | `bswap r32` (0F C8+rd, no REX.W)           | ntohl                           |
| pa-r15-002 | `xadd [mem], r64` (F0 REX.W 0F C1 /r)      | atomic add-and-fetch counter    |
| pa-r15-003 | `lock add [mem], imm8/imm32/r64`           | shared counter incr             |
| pa-r15-004 | `rol / ror r64, imm8` and `, cl`           | checksum folding                |
| pa-r15-005 | `adc / sbb r64, r/m64` (carry chain)       | multi-limb checksum             |
| pa-r15-006 | `popcnt r64, r/m64` (F3 REX.W 0F B8 /r)    | packet feature counts           |

### 2.4 v0.16 — CoW FS + atomics

| Issue      | Encoder shape                                          | paideia-os unblock              |
|------------|--------------------------------------------------------|---------------------------------|
| pa-r16-001 | `bt / bts / btr / btc r/m64, r64`                      | phys_alloc bitmap first-fit     |
| pa-r16-002 | `lock bts / btr / btc [mem], imm8`                     | phys_free atomic clear          |
| pa-r16-003 | `lock cmpxchg [m], r32`                                | 32-bit CAS (refcount)           |
| pa-r16-004 | `lock cmpxchg16b [m]` (double-width CAS)               | ABA-free freelist               |
| pa-r16-005 | `lock xadd [m], r32/r64`                               | refcount incr                   |
| pa-r16-006 | `lock and / or / xor [m], r64`                         | atomic bitfield mutations       |
| pa-r16-007 | `pause` (F3 90)                                        | spinloop hint                   |

### 2.5 v0.17 — record / fnptr encoding, closures pre-lift

| Issue      | Encoder shape                                          | paideia-os unblock              |
|------------|--------------------------------------------------------|---------------------------------|
| pa-r17-014 | `lea r64, [rip + sym + N]` with N > i32::MAX guard     | fnptr initialiser               |
| pa-r17-015 | `call [rip + sym]` (indirect via GOT-like table)       | vops dispatch                   |

### 2.6 v0.18 — semantic terminal

| Issue      | Encoder shape                                          | paideia-os unblock              |
|------------|--------------------------------------------------------|---------------------------------|
| pa-r18-011 | AVX2 baseline (`vpcmpeqb ymm, ymm, ymm`, `vpmovmskb`)  | hash-table probe SIMD           |
| pa-r18-012 | `crc32 r64, r/m64` (F2 REX.W 0F 38 F1 /r)              | hash-table hash function        |

### 2.7 v0.19 — UEFI + embed

| Issue      | Encoder shape                                          | paideia-os unblock              |
|------------|--------------------------------------------------------|---------------------------------|
| pa-r19-004 | Full ModR/M for MS x64 arg mapping (RCX, RDX, R8, R9)  | UEFI protocol call thunk        |
| pa-r19-005 | `push imm32` and `push imm8` sign-extended             | UEFI stack-arg construction     |
| pa-r19-006 | Shadow-space frame prologue emitter                    | MS x64 callee prologue          |

### 2.8 v0.20 — self-hosting shape

| Issue      | Encoder shape                                          | paideia-os unblock              |
|------------|--------------------------------------------------------|---------------------------------|
| pa-r20-002 | Public-API `emit_instruction(&mut Buf, Instruction)`   | WASM JIT emit                   |
| pa-r20-003 | `endbr64` / `endbr32` (CET)                            | indirect-branch target markers  |
| pa-r20-004 | `xsaveopt`, `xrstor` (extended state save)             | AVX-512 context switch          |

**Chronological single-list summary** (encoder issues only, in landing
order):

```
v0.13: #927, #928, #929, ud2, dec, test-imm, test-rr, cld, imul3, or-imm64,
       rep_movsb-robust, rep_stosq-robust, setcc, bswap64
v0.14: mov-mem-imm-narrow, mov-r32-mem-full, movnti, sfence/lfence,
       wbinvd/invd/clflush, prefetch
v0.15: bswap32, xadd, lock-add, rol/ror, adc/sbb, popcnt
v0.16: bt/bts/btr/btc, lock-bts/btr/btc-imm, lock-cmpxchg32,
       lock-cmpxchg16b, lock-xadd, lock-and/or/xor, pause
v0.17: lea-rip-sym-N-guard, call-rip-sym
v0.18: AVX2-baseline, crc32
v0.19: msx64-argmap, push-imm, shadow-space-frame
v0.20: emit_instruction-public-API, endbr64, xsaveopt/xrstor
```

Encoder issue count: **41** across 8 releases (out of ~99 total issues; the
rest are elaborator / type-system / diagnostics / testing infrastructure).

---

## 3. Release v0.13 — R14 encoder-completion round

**Theme.** Retire every encoder workaround discovered during the R14A/R14B
paideia-os loop. Post-v0.13, paideia-os may still rely on **language-level**
workarounds (records-with-unsafe, no closures, no first-class fnptr) but must
not need x86_64 primitive workarounds.

**Exit criteria.**

- Every gap on the list in the task briefing (§ "Additional gaps found by
  workarounds during R14B") retired, with paideia-os TODO comments removed in
  the follow-up submodule bump.
- paideia-os R14 loop passes with no `.S` files added since v0.12.0 except the
  three that predate the arc (`boot_stub.S`, `pq_stub.S`, one `.incbin`
  glue).
- `find-paideia-as.sh` remains strict at commit boundary.
- 3300+ workspace tests (baseline 3119 at v0.11.0, ~3200 projected at v0.12.0
  close; delta ~100 for R13-cycle bug fixes and R14 gap fills).

### 3.1 Issues

#### pa-r13-001 (mirrors #927) — narrow-width `mov r8/r16/r32, [mem]`

**Goal.** `mov al, [rdi]` / `mov ax, [rdi]` / `mov eax, [rdi]` must emit the
correct operand-size prefix and drop REX.W; today `register_name_to_regid`
collapses al/ax/eax/rax to the same `RegId` and the load-form encoder
unconditionally sets REX.W.

**AC.**

- IR gains a `MovMem { width: IntWidth }` variant OR the existing `Mov` load
  gains a `width` field threaded through `unsafe_walker` from the destination
  register's declared width.
- Encoder emits: `8A /r` for r8, `66 8B /r` for r16, `8B /r` for r32 (no
  REX.W), `REX.W 8B /r` for r64.
- REX.B still emitted when base ∈ r8–r15; REX.X still emitted when index ∈
  r8–r15; REX.R still emitted when destination ∈ r8–r15.
- 24 byte-exact tests (4 widths × [reg-base, reg+disp8, reg+disp32,
  r8-r15-base, r8-r15-dest, index-scale]) + 4 iced-x86 round-trip tests.
- Elaborator diagnostic **T0533** for width mismatch: `mov al, [rdi + 1000]`
  where the type of `[rdi + 1000]` is inferred to be `u64` should either
  succeed with an explicit `as u8` cast or diagnose.

**Touching files.**

- `crates/paideia-as-ir/src/instruction.rs` — extend `Mov` or add
  `MovMemSized`.
- `crates/paideia-as-elaborator/src/unsafe_walker.rs` — width-inference for
  memory load; register-name → width map.
- `crates/paideia-as-encoder/src/encode.rs` — new `encode_mov_mem_sized`;
  route from `encode_mov`.
- `crates/paideia-as-encoder/tests/mov_mem_narrow.rs` — new.

**Deps.** None (self-contained bug-fix shape).

**LOC.** ~350 (150 encoder, 80 elaborator, 100 tests, 20 IR).

---

#### pa-r13-002 (mirrors #928) — REX.B drop on `mov r64, [ext_base + …]` SIB

**Goal.** `mov rax, [r13 + rsi*8]` currently drops REX.B because
`emit_indexed_load` (and its store peer) treats the SIB base as always
r0-r7. This is the REX.B counterpart to #911's REX.X fix.

**AC.**

- `encode_mov` and every SIB-emitting helper (indexed-load, indexed-store,
  `encode_lea`, `encode_add [mem]`, `encode_sub [mem]`, `encode_cmp [mem]`)
  sets REX.B when SIB.base ∈ r8–r15.
- REX.X for index and REX.R for dest are re-verified in an audit test suite
  that iterates every combination of (dest, base, index) ∈ {r7, r8, r15}.
- 27 byte-exact tests (3^3 dest/base/index combinations for `mov r64, [b +
  i*s + d]`) + 3 iced-x86 round-trip.
- The audit test suite grows a compile-fail row for each MNEMONIC_TABLE entry
  that has a memory operand.

**Touching files.** `encode.rs` (broad — every SIB helper); new test file
`sib_rex_audit.rs`.

**Deps.** None; interacts with pa-r13-001 (both touch REX prefix
computation); recommend landing 001 first so the audit corpus tests are
authored on the new width-aware path.

**LOC.** ~500 (300 encoder churn, 200 tests).

---

#### pa-r13-003 (mirrors #929) — indirect `call [mem]` and `call reg`

**Goal.** `crates/paideia-as-encoder/src/encode.rs:1364` currently encodes
`call` as `E8 rel32` only. `call [rdi + 24]` (vops indirect dispatch) and
`call rax` (function-pointer variable) both fail today.

**AC.**

- Add `CallTarget` variant to the call encoder path: `Direct(RelSym)` (existing),
  `MemIndirect(MemOperand)`, `RegIndirect(RegId)`.
- Encoding: `FF /2` for register-indirect (no REX.W; REX.B for r8–r15);
  `FF /2 [ModR/M+SIB+disp]` for memory-indirect.
- Elaborator: `call foo` where `foo` is a local of type `(*T) -> U`
  lowers to `call reg`; `call [rdi + N]` where `[rdi + N]` types as a
  function-pointer lowers to `call [mem]`.
- 18 byte-exact tests (register-indirect: rax, rcx, r8, r15; memory-indirect:
  [rdi], [rdi+8], [rdi+0x100], [r13], [r13+8], SIB forms with index) + 4
  iced-x86 round-trip.
- Elaborator diagnostic **T0534** when the call target's type is not
  callable.

**Touching files.**

- `crates/paideia-as-ir/src/instruction.rs` — `CallTarget` enum.
- `crates/paideia-as-encoder/src/encode.rs` — `encode_call_indirect`.
- `crates/paideia-as-elaborator/src/unsafe_walker.rs` — call-target resolution.
- Tests: `crates/paideia-as-encoder/tests/call_indirect.rs`.

**Deps.** pa-r17-015 (`call [rip + sym]`) builds on this; land the memory
form here in the SIB-shape and defer the RIP-relative form to v0.17 where
first-class fnptr types drive the elaborator side.

**LOC.** ~420 (180 encoder, 120 elaborator, 100 tests, 20 IR).

---

#### pa-r13-004 — `ud2` (0F 0B)

**Goal.** Two-byte undefined-instruction opcode for "unreachable" tail slots
(enter_userland_initial's post-iretq slot, `unreachable!()` in future).

**AC.**

- Mnemonic table entry; IR arity 0; estimated size 2.
- Encoder: emit `0F 0B` unconditionally.
- 2 tests (byte-exact + iced round-trip).
- Retires the paideia-os workaround `hlt` in `enter_userland_initial`.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/ud2.rs`.

**Deps.** None.

**LOC.** ~80.

---

#### pa-r13-005 — `dec r64` (REX.W FF /1)

**Goal.** Decrement a 64-bit register in place; retires the R14B workaround
of `sub reg, 1`.

**AC.**

- Mnemonic table entry; IR arity 1; estimated size 3.
- Encoder: `REX.W FF /1`; REX.B for r8-r15; 3-byte upper bound.
- 6 byte-exact tests (rax, rcx, rbx, r8, r13, r15) + 1 iced round-trip.
- Also add `inc r64` (`REX.W FF /0`) at same LOC cost.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/inc_dec.rs`.

**Deps.** None.

**LOC.** ~180 (60 for each of inc/dec + shared prep + tests).

---

#### pa-r13-006 — `test r64, imm32` (REX.W F7 /0)

**Goal.** Retire the R14B workaround of `and r64, imm` + `cmp r64, 0`.

**AC.**

- Encoder path: `REX.W F7 /0 id` (7 bytes) — sign-extended imm32.
- Range check: values in i32::MIN..=i32::MAX; anything wider errors with
  `Unsupported("64-bit immediate test not yet supported")` and points at
  the `and+cmp` workaround.
- **Special-case:** RAX may use `REX.W A9 id` (one byte shorter); optional
  optimisation.
- 5 byte-exact tests (rax, rcx, r10, r15 + rax-short-form-check) + 1 iced
  round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/test_imm.rs`.

**Deps.** None.

**LOC.** ~200.

---

#### pa-r13-007 — `test r/m64, r64` reg-reg form

**Goal.** `test rax, rax` is the canonical "is zero?" primitive; paideia-os
uses `cmp rax, 0` (5 bytes) throughout; `test rax, rax` is 3.

**AC.**

- Encoder: `REX.W 85 /r`; REX.B / REX.R for r8-r15.
- 6 byte-exact tests + 1 iced round-trip.
- Peephole hint in `crates/paideia-as-elaborator/src/opt/`: `cmp reg, 0`
  followed by `jz/jnz` may lower to `test reg, reg` + `jz/jnz` — **but this
  is a v0.14 optimisation pass, not a v0.13 lowering**. v0.13 only offers
  the primitive.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/test_reg.rs`.

**Deps.** None.

**LOC.** ~150.

---

#### pa-r13-008 — `cld` (0xFC) and `std` (0xFD)

**Goal.** Explicit direction-flag clear on kernel entry.

**AC.**

- Both mnemonics; IR arity 0; 1 byte each.
- 4 tests (byte-exact + iced for each).
- Update kernel-entry prologue emitter to prefer explicit `cld`.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/cld_std.rs`.

**Deps.** None.

**LOC.** ~120.

---

#### pa-r13-009 — `imul r64, r/m64, imm8/imm32` (three-operand form)

**Goal.** Three-operand imul lets us write `imul rax, rsi, 8` for index
scaling without clobbering the source.

**AC.**

- Encoder path: `REX.W 6B /r ib` (imm8, 5 bytes) and `REX.W 69 /r id`
  (imm32, 8 bytes); already partially exists (see `encode.rs:587,603`)
  for two-register-plus-immediate; verify the **three-register** shape
  (dst, src, imm) is exposed rather than the mutating (dst, imm) shape.
- Elaborator arity handling: `imul rax, rsi, 8` parses; the middle operand
  is the source and the last is the immediate.
- 8 byte-exact tests (imm8 + imm32 across rax/rcx/r8/r15 dest + rax/rsi/r10
  src) + 2 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/imul_three_op.rs`.

**Deps.** None (encoder helpers exist; this is arity-plumbing).

**LOC.** ~220.

---

#### pa-r13-010 — `or r64, imm64` via `movabs + or reg, reg` macro-expansion

**Goal.** `or rax, 0xFFFF_FFFF_0000_0000` has no direct encoding (x86_64
`or r64, imm` sign-extends imm32). Provide macro-expansion in the elaborator
to `movabs r_scratch, imm64` + `or dst, r_scratch`.

**AC.**

- Elaborator recognises `or r64, imm64` when imm64 doesn't fit in i32; emits
  IR sequence `[movabs scratch, imm64; or dst, scratch]`.
- **Scratch register allocation** — reserve r11 as the "expander scratch"
  slot (unused by any handler-bracketed operation per calling-convention
  doc §1); document.
- Same treatment for `and r64, imm64` and `xor r64, imm64`.
- 12 byte-exact tests (or/and/xor × 4 sample imm64 values including all
  bits set) + 3 iced round-trip.

**Touching files.**

- `crates/paideia-as-elaborator/src/two_phase.rs` — expansion pass hook.
- Tests: `imm64_bitops_expansion.rs`.

**Deps.** None; interacts with pa-r14-005 (cache flush uses this shape).

**LOC.** ~280.

---

#### pa-r13-011 — `rep_movsb` robustness pass

**Goal.** `rep_movsb` exists as a mnemonic, but the R14B workaround suggests
callers hit a corner case in the emitter (address-mode or REX-prefix drop).
Audit and fix.

**AC.**

- Audit: enumerate every code path in `encode.rs` that touches
  `Mnemonic::RepMovsb`. Identify the drop.
- Fix + 6 new byte-exact tests that specifically exercise the failing shape
  (likely: `rep movsb` following a `cld` in the same block; or after a
  segment-prefix `[gs:...]` operand two lines earlier).
- Landing note: the elaborator's Pass 2 label-drain path (pa-r13-011 v0.12,
  #924) may already have made this go away; confirm by reproducing the R14B
  hlt-loop.

**Touching files.** `encode.rs`, `unsafe_walker.rs`, `tests/rep_movsb.rs`.

**Deps.** None.

**LOC.** ~200.

---

#### pa-r13-012 — `rep_stosq` robustness pass

**Goal.** Sibling of pa-r13-011 for stosq.

**AC.** Same shape as pa-r13-011.

**Touching files.** Same as pa-r13-011.

**Deps.** None.

**LOC.** ~180.

---

#### pa-r13-013 — `setcc r8` family

**Goal.** `sete al` / `setne al` / `setz al` etc for boolean lowering
(records with `bool` fields will need this in v0.17).

**AC.**

- All 16 `Jcc` conditions mirrored as `setcc` mnemonics (or a single `SetCc(Cond)`
  variant like `Jcc(Cond)`).
- Encoder: `0F 9x /0 ModR/M` (3 bytes, no REX needed for al-dl; REX for
  spl-r15b).
- 16 × 3 = 48 byte-exact tests (all conditions × [al, r10b, r15b]) + 4 iced
  round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/setcc.rs`.

**Deps.** None.

**LOC.** ~350.

---

#### pa-r13-014 — `bswap r64` (REX.W 0F C8+rd)

**Goal.** Byte swap for network byte order; needed by v0.15 network stack.

**AC.**

- Mnemonic + encoder; `REX.W 0F C8+rd`; REX.B for r8–r15; 3-byte upper
  bound.
- 6 byte-exact tests (rax, rcx, r8, r15 + high-reg alias check) + 1 iced.
- v0.15 adds the 32-bit variant separately (`0F C8+rd` no REX.W).

**Touching files.** `instruction.rs`, `encode.rs`, `tests/bswap64.rs`.

**Deps.** None.

**LOC.** ~150.

### 3.2 Testing strategy — v0.13

- **Round-trip corpus extension.** Add
  `crates/paideia-as-encoder/tests/roundtrip_corpus_r13.rs` that walks a
  YAML-like table of `{ mnemonic, operand-shape, expected-bytes }` rows
  and iced-x86-decodes each. Every issue above adds ≥ 3 rows.
- **Prefix-audit test.** `tests/prefix_audit.rs` iterates the MNEMONIC_TABLE
  and, for every entry that has ≥ 1 register operand, generates
  {r0, r7, r8, r15} substitutions and asserts REX prefix presence matches
  a hand-authored expectation table. This is the "REX.B drop" long tail
  regression net.

### 3.3 Cross-release deps forward

- pa-r13-013 (setcc) unblocks record `bool` field access in v0.17.
- pa-r13-003 (call indirect) is the SIB-shape foundation that v0.17's
  `call [rip + sym]` builds on.
- pa-r13-014 (bswap64) plus v0.15's bswap32 close network byte-order needs.
- pa-r13-010 (imm64 bitops expansion) is a template for v0.16's `lock`
  variants of the same shape (heavyweight expansion via r11 scratch).

---

## 4. Release v0.14 — Driver substrate

**Theme.** MMIO register banks, ring-buffer synthesis, function-pointer
dispatch **without** first-class fnptr type (still lowered via manual
`unsafe { call reg }`). This is the last release where drivers must escape
into `unsafe { }` for dispatch tables.

**Exit criteria.**

- paideia-os phase 5 (block driver) compiles fully in `.pdx`.
- Phase 6 (VFS) dispatch is inline `unsafe { let vop = [table + off]; call
  vop; }` — v0.17 lifts to first-class.
- Phase 7 (process manager) fork/exec compiles in `.pdx` with no C helper.

### 4.1 Issues

#### pa-r14-001 — narrow-width `mov [mem], imm` forms

**Goal.** `mov byte [rdi + 4], 0x01` / `mov word [rdi + 6], 0x1234` /
`mov dword [rdi + 8], 0x1234_5678`. Today only `mov qword [mem], imm32-sxt`
exists.

**AC.**

- Encoder: `C6 /0 ib` (r8), `66 C7 /0 iw` (r16), `C7 /0 id` (r32), `REX.W C7 /0 id`
  (r64, already exists).
- **Width-tag on the memory operand** — the elaborator infers width from a
  declared type-annotated memory operand: `mov (byte)[rdi], 0x01` or
  `mov [rdi] : u8, 0x01`. **Grammar choice: prefer type-ascription** at
  operand site: `mov [rdi] : u8, 0x01`. Documented in
  `design/toolchain/mov-mem-width-syntax.md` (new).
- 12 byte-exact tests + 3 iced round-trip.

**Touching files.** Parser, elaborator, encoder, tests.

**Deps.** None.

**LOC.** ~450.

---

#### pa-r14-002 — full `mov r32, [mem]` shape

**Goal.** Pair to pa-r13-001. Same width-inference machinery, load side of
r32.

**AC.**

- Delivered as part of pa-r13-001; this issue is the elaborator-side follow-up
  in `unsafe_walker.rs` that plumbs the width tag through every
  `Operand::MemSib`-carrying instruction.
- 8 byte-exact tests + 2 iced round-trip.

**Touching files.** `unsafe_walker.rs`, `encode.rs`.

**Deps.** pa-r13-001.

**LOC.** ~200.

---

#### pa-r14-003 — `movnti [mem], r32/r64`

**Goal.** Non-temporal MMIO store (skips cache); needed for write-combining
regions.

**AC.**

- Encoder: `0F C3 /r` (r32), `REX.W 0F C3 /r` (r64); no REX.W for r32 form
  (regression against a common pitfall — write a test that fails today's
  code path).
- 8 byte-exact tests + 2 iced round-trip.
- Effect annotation: `!{RawMem, NonTemporal}` (new effect variant); tracked
  in `crates/paideia-as-effects`.

**Touching files.** `instruction.rs`, `encode.rs`, `paideia-as-effects/`,
tests.

**Deps.** None.

**LOC.** ~280.

---

#### pa-r14-004 — `sfence`, `lfence`

**Goal.** Complete the fence trio; `mfence` shipped in v0.12.

**AC.**

- Encoder: `0F AE F8` (sfence), `0F AE E8` (lfence); 3 bytes each.
- 4 tests (byte-exact + iced for each).

**Touching files.** `instruction.rs`, `encode.rs`, `tests/fences.rs`.

**Deps.** None.

**LOC.** ~120.

---

#### pa-r14-005 — `wbinvd`, `invd`, `clflush [mem]`, `clflushopt`

**Goal.** Cache management around DMA and MMIO.

**AC.**

- `wbinvd`: `0F 09` (2 bytes, no operands).
- `invd`: `0F 08` (2 bytes, no operands).
- `clflush [mem]`: `0F AE /7` (5-byte upper bound).
- `clflushopt [mem]`: `66 0F AE /7` (6-byte upper bound).
- 12 byte-exact tests + 4 iced round-trip.
- Effect annotation: `!{CachePolicy}` (new); document semantics.

**Touching files.** `instruction.rs`, `encode.rs`, `paideia-as-effects/`,
tests.

**Deps.** None.

**LOC.** ~360.

---

#### pa-r14-006 — `prefetch` family (`prefetchnta`, `prefetcht0/t1/t2`)

**Goal.** Ring-buffer producer prefetches next slot.

**AC.**

- Encoder: `0F 18 /0..3` (5 bytes each with disp32); `/0=prefetchnta`,
  `/1=t0`, `/2=t1`, `/3=t2`.
- 16 byte-exact tests + 4 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/prefetch.rs`.

**Deps.** None.

**LOC.** ~250.

---

#### pa-r14-007 — MMIO helper macro library

**Goal.** paideia-os phase 5 authors want `mmio_read_u32(addr) -> u32` and
`mmio_write_u32(addr, val)` as inline-expanded macros with the effect
signature `!{RawMem, MmioRead|MmioWrite}` and `@paideia.mmio` capability
gate.

**AC.**

- Stdlib module `paideia-stdlib/src/mmio.pdx` (new) provides the four
  primitives (r8, r16, r32, r64 in each direction) as `pub fn` with
  effects.
- Effect kind `Mmio { direction: In/Out }` added.
- 8 integration tests via `crates/paideia-as-test/` that compile a small
  driver stub.

**Touching files.** `paideia-stdlib/` (new module), `paideia-as-effects/`,
integration tests.

**Deps.** pa-r14-003.

**LOC.** ~500.

---

#### pa-r14-008 — ring-buffer synthesis primitives

**Goal.** paideia-os phase 5 (AHCI FIS ring) and phase 8 (NIC ring) need a
consistent shape for "declare a queue in shared memory". Provide a `@ring`
attribute on a `pub let` that emits a struct with head/tail/mask + slot
array, aligned to a cache line.

**AC.**

- `@ring(slots=64, slot_size=32)` attribute grammar; parser validates.
- Elaborator emits four data-table entries: `<name>_slots`, `<name>_head`,
  `<name>_tail`, `<name>_mask`.
- Rejects non-power-of-two slot counts (**parser diagnostic P0260**).
- 6 integration tests.
- Documented in `design/toolchain/ring-attribute.md` (new).

**Touching files.** Parser, elaborator, docs, tests.

**Deps.** None.

**LOC.** ~600.

---

#### pa-r14-009 — function-pointer dispatch (unsafe pattern)

**Goal.** Manual VFS `vops` dispatch: `unsafe { mov rax, [rdi + 24]; call
rax; }`. Ensure this pattern compiles cleanly with pa-r13-003 in place; it's
not first-class, but v0.14 documents the pattern and provides a test
fixture in `paideia-as-test`.

**AC.**

- 4 integration tests exercise the shape end-to-end.
- Documentation in `design/toolchain/fnptr-unsafe-pattern.md` (new; v0.17
  will supersede).

**Touching files.** `paideia-as-test/`, docs.

**Deps.** pa-r13-003.

**LOC.** ~200.

---

#### pa-r14-010 — driver-side effect vocabulary

**Goal.** Formalise the effects: `MmioRead`, `MmioWrite`, `CachePolicy`,
`NonTemporal`, `DmaBarrier`. These are used by v0.15 and v0.16 code paths;
land the vocabulary here so the surface is stable.

**AC.**

- All five effect variants added to `paideia-as-effects/src/effect.rs`.
- Effect-inference rules extended; documented in
  `design/effects/driver-effects.md` (new).
- Round-trip tests via elaborator.

**Touching files.** `paideia-as-effects/`, `paideia-as-elaborator/`, docs.

**Deps.** None.

**LOC.** ~400.

---

#### pa-r14-011 — peephole: `cmp reg, 0` → `test reg, reg`

**Goal.** Consume pa-r13-007's primitive; add an optimisation pass in
`paideia-as-elaborator/src/opt/` that rewrites `cmp reg, 0` immediately
followed by `jz/jnz` into `test reg, reg` + `jz/jnz` when the intervening
flags aren't consumed.

**AC.**

- New file `paideia-as-elaborator/src/opt/peephole_test.rs`.
- Correctness lemma: `test reg, reg` sets ZF/SF/PF identically to `cmp
  reg, 0`; CF/OF differ but are not used by `jz/jnz`.
- Enabled at `-O1` (i.e. `paideia-as build --optimize 1`).
- 6 unit tests.

**Touching files.** `paideia-as-elaborator/src/opt/`, CLI flag plumb.

**Deps.** pa-r13-007.

**LOC.** ~250.

---

#### pa-r14-012 — driver corpus integration test

**Goal.** End-to-end integration test that compiles a minimal AHCI FIS
ring driver stub (~100 LOC of `.pdx`) and verifies the emitted ELF has
correct .rodata for register-offset constants, .data for the ring
descriptors, and .text for the dispatch loop.

**AC.**

- Test file in `crates/paideia-as-test/` (or wherever integration tests
  live).
- Test passes as part of `cargo test --workspace`.
- Documented as a canary: any future regression in MMIO / ring / dispatch
  breaks this test.

**Touching files.** integration test crate.

**Deps.** pa-r14-001..010.

**LOC.** ~350.

### 4.2 Testing strategy — v0.14

- **Driver-shape corpus.** Add `tests/driver_corpus.rs` with 10 driver
  stubs (block, net, serial, HPET, apic, ioapic, hpet timer, ...).
- **Effect-inference audit.** Every new effect variant gets a compile-fail
  test row that asserts the diagnostic code fires when the effect is not
  handled.

### 4.3 Cross-release deps forward

- pa-r14-003 (movnti) and pa-r14-005 (clflush) are prerequisites for the
  v0.16 CoW FS journal write barrier.
- pa-r14-008 (ring attribute) is what v0.15 network stack uses to declare
  NIC descriptor rings.
- pa-r14-010 (effect vocabulary) freezes the driver-effect surface.

---

## 5. Release v0.15 — Network primitives

**Theme.** Byte-level packet parsing, checksum folding, atomic counters for
per-CPU stats. All primitives that let paideia-os phase 8 (network stack)
compile without descending into GAS.

**Exit criteria.**

- paideia-os `net/ipv4/`, `net/tcp/`, `net/udp/` compile fully in `.pdx`.
- No C-helper checksum function; the fold-add-carry loop compiles in `.pdx`.
- Per-CPU counters use `lock xadd` directly; no CAS-loop workaround.

### 5.1 Issues

#### pa-r15-001 — `bswap r32`

**Goal.** 32-bit byte-swap for `htonl` / `ntohl`.

**AC.**

- Encoder: `0F C8+rd`; no REX.W; REX.B for r8-r15.
- 6 byte-exact tests + 1 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, `tests/bswap32.rs`.

**Deps.** pa-r13-014.

**LOC.** ~120.

---

#### pa-r15-002 — `xadd [mem], r64` (locked)

**Goal.** Fetch-and-add for shared counters.

**AC.**

- Encoder: `F0 REX.W 0F C1 /r` (10-byte upper bound); memory form implicitly
  locked when LOCK prefix present.
- Also 32-bit form: `F0 0F C1 /r` (no REX.W).
- 10 byte-exact tests + 2 iced round-trip.
- Effect: `!{Atomic}`.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None (encoder shape mirrors `lock cmpxchg` from v0.12).

**LOC.** ~280.

---

#### pa-r15-003 — `lock add [mem], imm8/imm32/r64` and `lock sub` sibling

**Goal.** Atomic increment of a shared counter without needing to load the
old value.

**AC.**

- Encoder: `F0 REX.W 83 /0 ib`, `F0 REX.W 81 /0 id`, `F0 REX.W 01 /r`.
- Mirror `lock sub` shapes.
- 16 byte-exact tests + 4 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~360.

---

#### pa-r15-004 — `rol r64, imm8/cl`, `ror r64, imm8/cl`

**Goal.** Bit rotation for checksum folding and hash mixing.

**AC.**

- Encoder: `REX.W C1 /0 ib` (rol imm), `REX.W D3 /0` (rol cl); `REX.W C1 /1`,
  `REX.W D3 /1` (ror).
- **Special case:** rotate by 1 has a shorter form (`REX.W D1 /0..1`); emit
  when count is 1.
- 12 byte-exact tests + 3 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~300.

---

#### pa-r15-005 — `adc r64, r/m64` and `sbb r64, r/m64` (carry chain)

**Goal.** Multi-limb arithmetic for checksums that fold via carry.

**AC.**

- Encoder: `REX.W 13 /r` (adc), `REX.W 1B /r` (sbb); memory operand variants
  as well.
- 12 byte-exact tests + 3 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~320.

---

#### pa-r15-006 — `popcnt r64, r/m64`

**Goal.** Bit-population count for packet feature reporting.

**AC.**

- Encoder: `F3 REX.W 0F B8 /r` (6-byte upper bound).
- Also 32-bit variant `F3 0F B8 /r`.
- 8 byte-exact tests + 2 iced round-trip.
- Feature gate: emit compile-time diagnostic if the target CPU doesn't
  declare CPUID.01H:ECX.POPCNT[bit 23]; **for v0.15, assume Nehalem+**
  (paideia-os's target baseline).

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~240.

---

#### pa-r15-007 — byte-parsing macro library

**Goal.** paideia-os network stack wants `let ver = get_u8(pkt, 0)`,
`let len = get_u16_be(pkt, 2)`, `let src = get_u32_be(pkt, 12)`. Provide as
inline-expanded macros in `paideia-stdlib/src/bytes.pdx`.

**AC.**

- Stdlib module `paideia-stdlib/src/bytes.pdx` (new) with 24 primitives
  (u8/u16/u32/u64 × be/le × read/write).
- Each is `#[inline(always)]`-equivalent (lowered to 1-3 instructions).
- 24 unit tests + 6 integration tests via a small IPv4 header parser.

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r13-014, pa-r15-001.

**LOC.** ~450.

---

#### pa-r15-008 — checksum-fold intrinsic

**Goal.** IPv4 header checksum needs the "16-bit ones-complement sum with
end-around carry" idiom; provide as a stdlib function that the paideia-os
IPv4 handler calls.

**AC.**

- Stdlib `paideia-stdlib/src/checksum.pdx` exports
  `fn ipv4_checksum(hdr: *u8, len: u16) -> u16`.
- Implementation uses `adc` + fold via `mov + shr + add + adc`.
- 8 unit tests with reference vectors from RFC 1071.

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r15-005.

**LOC.** ~250.

---

#### pa-r15-009 — protocol-dispatch pattern

**Goal.** Byte-triggered dispatch (IPv4 protocol field → handler). paideia-os
today needs a jump-table shape. Provide `@jump_table` attribute on a
`match` expression over `u8` scrutinee that emits a jump-table when arms
are dense.

**AC.**

- Elaborator recognises `@jump_table` attribute on `match` scrutinee.
- Emits jump table in `.rodata` + `jmp [rax*8 + table]` shape.
- Arms not covered fall through to the default arm.
- 6 integration tests.

**Touching files.** Parser, elaborator, docs.

**Deps.** None (uses existing memory-indirect jmp).

**LOC.** ~450.

---

#### pa-r15-010 — per-CPU counter idiom

**Goal.** Formalise `[gs:offset]` per-CPU counter increment as a single macro
`percpu_inc!(counter_name)`.

**AC.**

- Stdlib macro that expands to `lock inc [gs:sym]` (or `lock add [gs:sym], 1`).
- Documented in `design/toolchain/percpu-idiom.md` (new).
- 4 integration tests.

**Touching files.** `paideia-stdlib/`, docs.

**Deps.** pa-r15-003.

**LOC.** ~180.

---

#### pa-r15-011 — network corpus integration test

**Goal.** Compile a minimal IPv4 + UDP echo server stub in `.pdx`
end-to-end; verify emitted binary.

**AC.** Compiles clean; qemu-smoke-conditional test passes if qemu is
present in the environment.

**Touching files.** integration tests.

**Deps.** pa-r15-001..010.

**LOC.** ~400.

### 5.2 Testing strategy — v0.15

- **Byte-parse fuzz.** New fuzz target `fuzz/parse_bytes.rs` that generates
  random buffers and asserts `get_u32_be(buf, i)` never OOBs the buffer
  bounds — the elaborator effect analysis must catch it.
- **Checksum RFC-1071 corpus.** 100 vectors.

### 5.3 Cross-release deps forward

- pa-r15-004 (rol/ror) also used by v0.18 hash-table mixing.
- pa-r15-009 (jump table) is the pattern v0.18 semantic-terminal command
  dispatch uses.

---

## 6. Release v0.16 — CoW FS + atomics

**Theme.** Locked bit-ops (bts/btr/btc), double-width CAS (cmpxchg16b) for
ABA-free freelists, pause hint for spinloops. This is the last "primitive"
release before the language surface starts widening in v0.17.

**Exit criteria.**

- paideia-os phase 9 (CoW FS) compiles fully in `.pdx`; phys_alloc /
  phys_free / refcount_incr / refcount_decr are all pure `.pdx`.
- Spinloop bodies use `pause`.

### 6.1 Issues

#### pa-r16-001 — `bt / bts / btr / btc r/m64, r64` (register form)

**Goal.** Bit test / set / reset / complement — bitmap primitive.

**AC.**

- Encoder: `REX.W 0F A3 /r` (bt), `REX.W 0F AB /r` (bts), `REX.W 0F B3 /r`
  (btr), `REX.W 0F BB /r` (btc).
- Memory-operand forms as well (bit index in register selects a bit in a
  memory-resident bitmap, wrapping every 64 bits).
- 16 byte-exact tests + 4 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~400.

---

#### pa-r16-002 — `lock bts / btr / btc [mem], imm8` and register form

**Goal.** Atomic bit set / clear / complement — phys_alloc/phys_free
building block.

**AC.**

- Encoder: `F0 REX.W 0F BA /5 ib` (locked bts imm8), `/6 ib` (btr), `/7 ib`
  (btc); register-operand form `F0 REX.W 0F AB/B3/BB /r`.
- 12 byte-exact tests + 3 iced round-trip.
- Effect: `!{Atomic}`.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** pa-r16-001.

**LOC.** ~360.

---

#### pa-r16-003 — `lock cmpxchg [m], r32` (32-bit CAS)

**Goal.** 32-bit CAS for u32 refcounts.

**AC.**

- Encoder: `F0 0F B1 /r` (no REX.W); REX otherwise as needed.
- 6 byte-exact tests + 1 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~200.

---

#### pa-r16-004 — `lock cmpxchg16b [m]` (double-width CAS)

**Goal.** ABA-free freelist head via 128-bit compare-and-swap.

**AC.**

- Encoder: `F0 REX.W 0F C7 /1` (5 + memory-operand bytes); requires 16-byte
  aligned operand.
- Grammar: exposed as `unsafe { lock cmpxchg16b [rdi] }`; implicit
  RDX:RAX = expected, RCX:RBX = new; ZF = success.
- 4 byte-exact tests + 1 iced round-trip.
- Feature gate: check CPUID.01H:ECX.CMPXCHG16B[bit 13] at compile time.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~340.

---

#### pa-r16-005 — `lock xadd [m], r32/r64`

**Goal.** Fetch-and-add for refcount, extending v0.15's non-locked form.

**AC.**

- Encoder: `F0 REX.W 0F C1 /r` (already exists in pa-r15-002 with LOCK
  optional); this issue formalises the LOCK-prefixed shape as a distinct
  mnemonic with the `!{Atomic}` effect.
- 6 byte-exact tests.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** pa-r15-002.

**LOC.** ~200.

---

#### pa-r16-006 — `lock and / or / xor [m], r64`

**Goal.** Atomic bitfield mutation for shared flag words.

**AC.**

- Encoder: `F0 REX.W 21 /r` (and), `F0 REX.W 09 /r` (or), `F0 REX.W 31 /r`
  (xor).
- 9 byte-exact tests + 3 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~280.

---

#### pa-r16-007 — `pause` (F3 90)

**Goal.** Spinloop hint.

**AC.**

- Encoder: `F3 90` (2 bytes, no operands).
- 2 tests (byte-exact + iced round-trip).
- Stdlib macro `spin_hint!()` expands to `pause`.

**Touching files.** `instruction.rs`, `encode.rs`, `paideia-stdlib/`, tests.

**Deps.** None.

**LOC.** ~150.

---

#### pa-r16-008 — bitmap-scan intrinsic

**Goal.** paideia-os phys_alloc first-fit scan wants `bsf r64, r/m64` (bit
scan forward) — return the lowest set bit index of a word.

**AC.**

- Encoder: `REX.W 0F BC /r` (bsf), `REX.W 0F BD /r` (bsr).
- Modern-alt: `tzcnt` (`F3 REX.W 0F BC /r`, BMI1 required).
- 12 byte-exact tests + 3 iced round-trip.

**Touching files.** `instruction.rs`, `encode.rs`, tests.

**Deps.** None.

**LOC.** ~300.

---

#### pa-r16-009 — refcount-atomic stdlib

**Goal.** paideia-stdlib exposes
`refcount_incr(*u32) -> u32` and `refcount_decr(*u32) -> u32` and
`refcount_decr_and_test(*u32) -> bool` primitives, each expanding to a
single `lock xadd` or `lock cmpxchg` loop.

**AC.**

- Stdlib module `paideia-stdlib/src/refcount.pdx` (new).
- Effect: `!{Atomic}`.
- 6 unit tests.

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r16-005.

**LOC.** ~220.

---

#### pa-r16-010 — bitmap stdlib

**Goal.** `bitmap_first_free(*u64, usize) -> Option<usize>`,
`bitmap_set(*u64, usize) -> bool` (returns previous), etc.

**AC.**

- Stdlib module `paideia-stdlib/src/bitmap.pdx` (new).
- 8 unit tests including boundary conditions (bit 63 → bit 64 crossing).

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r16-001, pa-r16-002, pa-r16-008.

**LOC.** ~350.

---

#### pa-r16-011 — freelist stdlib (cmpxchg16b-backed)

**Goal.** ABA-free intrusive-linked-list `push`/`pop` primitive.

**AC.**

- Stdlib module `paideia-stdlib/src/freelist.pdx` (new); requires the
  freelist head to be a 16-byte-aligned `(ptr, tag)` pair.
- Effect: `!{Atomic}`.
- 4 unit tests including a 4-thread stress test in a fixture.
- CPU-feature guard.

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r16-004.

**LOC.** ~320.

---

#### pa-r16-012 — CoW FS integration test

**Goal.** Compile a minimal CoW FS `phys_alloc + phys_free + refcount`
scaffold end-to-end.

**AC.** Compiles clean; passes in-process test harness.

**Touching files.** integration tests.

**Deps.** pa-r16-001..011.

**LOC.** ~380.

### 6.2 Testing strategy — v0.16

- **Concurrent-correctness fuzz.** `fuzz/atomic_freelist.rs` runs the
  freelist primitive under Loom / Shuttle-like model checker. This is heavy
  but bounded — 8 threads × 8 ops depth.
- **Bitmap boundary corpus.** 64-bit word boundaries, 4096-bit page
  boundaries.

### 6.3 Cross-release deps forward

- pa-r16-004 (cmpxchg16b) is a hard prerequisite for the v0.18
  semantic-terminal hash-table's lock-free open-addressing scheme.

---

## 7. Release v0.17 — Function-pointer types + records / enums elaboration depth

**Theme.** First-class function-pointer types, complete elaboration of
records and discriminated unions **through pattern-match without descending
into `unsafe { ... }`**. This is the release where paideia-os stops writing
tagged-union dispatch by hand.

**Exit criteria.**

- paideia-os `fs/vfs/vops.pdx` declares:
  ```
  pub let vops : record {
    read:  (*File, *u8, u64) -> i64,
    write: (*File, *u8, u64) -> i64,
    ...
  } = ...
  ```
  and calls `(vops.read)(f, buf, len)` without any `unsafe { }` block.
- Pattern-match over enum variants routes through the encoder as jump-table
  or if-cascade based on arm density.

### 7.1 Issues

#### pa-r17-001 — function-pointer type grammar

**Goal.** `(*T, u64) -> R` as a type; `(*T, u64) -> R !{Effect}` with an
effect row.

**AC.**

- Parser extension: `Type ::= '(' Type (',' Type)* ')' '->' Type ('!' EffectRow)?`.
- Precedence: function-type binds looser than `*T` and record-type.
- AST node `TypeData::FnPtr { params, ret, effects }`.
- Documented in `design/toolchain/function-pointer-types.md` (new).

**Touching files.** Parser, AST, docs.

**Deps.** None.

**LOC.** ~500.

---

#### pa-r17-002 — function-pointer type checking

**Goal.** `let f : (u64) -> u64 = &some_fn;` — verify signature match at
elaboration.

**AC.**

- Elaborator gains a `TypeCheckFnPtr` pass: address-of a `pub fn` produces
  a value with the fn's signature type; assignment to a fnptr-typed
  binding requires equal signatures (arity, param types, ret type, effect
  row).
- Effect-row match rule: assignee's effect row must be a **subset** of
  target's (widening on assignment is safe; narrowing is not).
- 12 unit tests including subset-effect assignment, arity mismatch, param
  type mismatch, return type mismatch.
- Diagnostic **T0535** for mismatch.

**Touching files.** `paideia-as-elaborator/`, tests, docs.

**Deps.** pa-r17-001.

**LOC.** ~450.

---

#### pa-r17-003 — address-of function `&fn_name` lowering

**Goal.** `&some_fn` produces a symbol relocation targeting the function's
`.text` entry.

**AC.**

- Elaborator: `ExprData::AddrOf(Ident)` where the ident resolves to a
  `pub fn` lowers to a `SymbolRef { name, addend: 0 }` in IR.
- Emitter path: address-of expressions in a global initialiser produce a
  `.data` slot with an `R_X86_64_64` relocation to the function symbol.
- 6 integration tests.

**Touching files.** Elaborator, emitter, tests.

**Deps.** pa-r17-001, pa-r17-002.

**LOC.** ~380.

---

#### pa-r17-004 — first-class `call fptr` lowering

**Goal.** `(f)(x, y)` where `f : (u64, u64) -> u64` lowers to
`call reg`; `(vops.read)(f, buf, len)` where the fnptr comes from a
record field lowers to `call [mem]`.

**AC.**

- Elaborator recognises call-expression whose callee is a fnptr-typed
  expression (not a `pub fn` name).
- Lowering choice:
  - Callee in register → `call reg` (uses pa-r13-003).
  - Callee is a field access on a record → `call [base + field_offset]`
    directly (avoids the load-then-call sequence).
  - Callee is a `[rip + sym]` load → `call [rip + sym]` (uses pa-r17-015).
- Argument marshalling per SysV: RDI, RSI, RDX, RCX, R8, R9 for first six
  int/ptr args.
- 12 integration tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r17-002, pa-r13-003, pa-r17-015.

**LOC.** ~600.

---

#### pa-r17-005 — record field access lowering to typed offset

**Goal.** `r.field` where `r : record { field: u32, ... }` lowers to a
width-correct memory load. Today the m7-002 layout algorithm computes
offsets but the emitter path assumes u64 loads.

**AC.**

- Elaborator: `PostfixExpr::Field(base, ident)` computes offset via
  `record_layout::offset_of(record_ty, ident)` and emits width-correct
  load.
- Covers u8, u16, u32, u64, i8..i64, *T, fnptr types.
- 16 unit tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r13-001 (narrow-width load); pa-r13-013 (bool via setcc).

**LOC.** ~420.

---

#### pa-r17-006 — record field assignment lowering

**Goal.** `r.field = v` lowers to a width-correct memory store.

**AC.**

- Elaborator: `AssignExpr(FieldAccess(base, ident), rhs)` computes offset;
  width-correct store.
- 16 unit tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r14-001.

**LOC.** ~380.

---

#### pa-r17-007 — enum variant constructors

**Goal.** `Result::Ok(42)` lowers to a discriminant + payload in a value
slot; `Result::Err(errno)` similarly.

**AC.**

- Elaborator: `EnumVariantCons` node lowered to a stack-allocated tagged
  union with 8-byte discriminant + payload region sized to the maximum
  variant.
- Pass-by-value in registers when the enum fits in ≤ 16 bytes (fits in
  RAX:RDX); on stack otherwise.
- 12 unit tests.

**Touching files.** Elaborator, tests, docs.

**Deps.** None (m7 already ships the layout algorithm).

**LOC.** ~500.

---

#### pa-r17-008 — pattern-match on enum variants without `unsafe`

**Goal.** `match r { Ok(x) => ..., Err(e) => ... }` compiles as a
discriminant-load + conditional-branch cascade (or jump-table if variants
are dense).

**AC.**

- Elaborator: `match` expression over an enum-typed scrutinee lowers to
  a `cmp discriminant, N` + `jne next_arm` cascade.
- Dense variants (≥ 4 consecutive discriminants) → jump-table via pa-r15-009.
- Payload binding: pattern `Ok(x)` binds `x` to the payload slot.
- Exhaustiveness check remains as-is (m7-009).
- 20 integration tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r17-007, pa-r15-009, pa-r13-013.

**LOC.** ~700.

---

#### pa-r17-009 — nested pattern binding

**Goal.** `match r { Ok(Point { x, y }) => ..., ... }` binds nested fields.

**AC.**

- Elaborator: recursive pattern-lower emits nested field loads with
  correct offsets and widths.
- 10 integration tests including edge cases (record inside enum inside
  record).

**Touching files.** Elaborator, tests.

**Deps.** pa-r17-008.

**LOC.** ~450.

---

#### pa-r17-010 — record literal in global initialiser

**Goal.** `pub let vops : Vops = Vops { read: &read_impl, write: &write_impl, ... }`
compiles to a `.data` symbol with per-field relocations.

**AC.**

- Emitter: record-literal in global position emits a `.data` block whose
  content is the field values, with `R_X86_64_64` relocations for each
  fnptr field.
- 10 integration tests.

**Touching files.** Emitter, tests.

**Deps.** pa-r17-003, pa-r17-005.

**LOC.** ~380.

---

#### pa-r17-011 — enum size limit for register-passed values

**Goal.** Enums ≤ 16 bytes pass in RAX:RDX; larger enums pass by pointer.
Formalise and enforce.

**AC.**

- Elaborator: after enum-layout computation, if `sizeof(enum) > 16`, the
  function's return-type-lowering emits a pointer-return convention (RDI =
  return-slot address, per SysV).
- 6 unit tests.

**Touching files.** Elaborator, calling-convention doc.

**Deps.** pa-r17-007.

**LOC.** ~300.

---

#### pa-r17-012 — pure-function body branching (retire T0532 stub)

**Goal.** Close the m7-004-follow-up (see #913): pure fn body containing
`if`/`match`/`while` should lower to real branches, not diagnose.

**AC.**

- Elaborator: pure-function body with `if`/`match`/`while` walks children
  and emits real IR sequences (not the stub that dropped children).
- Retires diagnostic T0532; existing tests updated.
- 12 unit tests.

**Touching files.** Elaborator, tests.

**Deps.** None (this is the language-level version of what the workaround
of wrapping in `unsafe { block: { ... } }` bought).

**LOC.** ~600.

---

#### pa-r17-013 — `match` in trailing-expression position of pure fn

**Goal.** `pub fn classify(x: u32) -> Class { match x { 0 => Class::Zero,
_ => Class::NonZero } }` compiles.

**AC.**

- Elaborator: match-in-return position produces a value in RAX (or
  RAX:RDX for larger enums).
- 8 integration tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r17-012, pa-r17-008.

**LOC.** ~380.

---

#### pa-r17-014 (encoder) — `lea r64, [rip + sym + N]` with N > i32::MAX guard

**Goal.** Record-field initialisers of the form `&sym.field` at global scope
compute `sym_addr + field_offset`; if `field_offset > i32::MAX` (unlikely
but possible for very large records), the elaborator must reject with a
proper diagnostic instead of silently truncating.

**AC.**

- Elaborator or emitter check: `addend` on `SymbolRef` in a `lea [rip + sym +
  addend]` context is bounded by i32; overflow → diagnostic **T0536**.
- 4 unit tests.

**Touching files.** Elaborator, emitter, tests.

**Deps.** None (defensive).

**LOC.** ~150.

---

#### pa-r17-015 (encoder) — `call [rip + sym]`

**Goal.** Direct indirect-call through a global fnptr slot (GOT-like).

**AC.**

- Encoder: `FF /2 [rip + disp32]`; emits `R_X86_64_PC32` relocation with -4
  bias.
- 6 byte-exact tests + 2 iced round-trip.
- Elaborator: `(vops.read)(f, buf, len)` where `vops` is a `pub let` at
  module scope lowers to `call [rip + vops + read_offset]` directly (single
  instruction instead of load-then-call).

**Touching files.** Encoder, elaborator, tests.

**Deps.** pa-r13-003.

**LOC.** ~280.

### 7.2 Testing strategy — v0.17

- **Fnptr conformance corpus.** Add `tests/fnptr_conformance.rs` with 30
  fixtures covering direct call, indirect call, argument marshalling, effect
  row inference across calls.
- **Enum pattern-match corpus.** 40 fixtures — nested, guards, exhaustive vs.
  non-exhaustive with default, ordering-sensitive matches, no-payload
  variants.
- **Record layout audit.** 20 fixtures verify field offsets and total sizes
  against a reference computation.

### 7.3 Cross-release deps forward

- pa-r17-004 (fnptr call lowering) is the foundation for v0.18's closure
  environment.
- pa-r17-008 (enum match) is what v0.18 semantic terminal uses for command
  routing.

---

## 8. Release v0.18 — Semantic terminal: hash tables, closures, pattern-match depth

**Theme.** The paideia-os semantic terminal (phase 11) needs first-class
records, closures over records, hash tables for command lookup, and a
robust pattern-match compiler. This release delivers those.

**Exit criteria.**

- paideia-os semantic terminal command dispatch is a hash-table lookup +
  closure call, all in `.pdx`.
- Closures over ≤ 3 captures compile to `(env_ptr, code_ptr)` pairs
  passed by value.

### 8.1 Issues

#### pa-r18-001 — closure-type grammar and lowering

**Goal.** `let f : |u64| -> u64 = |x| x + captured_val;` — anonymous
functions capturing an environment.

**AC.**

- Parser: closure literal `|params| body` and closure type `|T| -> R
  ! {Eff}`.
- Elaborator: capture-analysis identifies free variables; synthesises a
  record-typed environment; lowers closure to `(env_ptr, code_ptr)` pair.
- Environment stored on caller's stack (Phase 1 of "closure
  representations"); heap-allocation deferred to v0.20.
- 12 integration tests.
- Documented in `design/toolchain/closures.md` (new).

**Touching files.** Parser, AST, elaborator, docs.

**Deps.** pa-r17-004.

**LOC.** ~800.

---

#### pa-r18-002 — closure-call lowering

**Goal.** `f(x)` where `f` is a closure loads the code pointer and calls it
with the environment pointer passed in a dedicated register (RCX per
paideia-native convention; SysV would use RDI).

**AC.**

- Calling convention amendment: closures pass `env_ptr` in **R14** (which
  the calling-convention doc reserves as an "effect / environment"
  register). Documented.
- Elaborator lowers `f(x)` to `mov r14, [f + 0]; call [f + 8]`.
- 10 integration tests.

**Touching files.** Elaborator, calling-convention doc, tests.

**Deps.** pa-r18-001.

**LOC.** ~450.

---

#### pa-r18-003 — hash-table stdlib

**Goal.** `HashMap<K, V>` in `paideia-stdlib/src/hashmap.pdx` with
open-addressing, robin-hood-ish probing, `get`, `insert`, `remove`.

**AC.**

- Stdlib module `hashmap.pdx` (new).
- Hash function: `fxhash`-like via rol + xor + multiply.
- Effect: `!{Alloc}` when the load factor forces a rehash; `!{}` on
  lookups.
- 24 unit tests (including collision cascades, deletion tombstones,
  rehash under load-factor threshold).

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r17-007 (enums for Option<V>), pa-r15-004 (rol for hash).

**LOC.** ~700.

---

#### pa-r18-004 — `Option<T>` and `Result<T, E>` in stdlib

**Goal.** Standardise the two most common enums.

**AC.**

- `paideia-stdlib/src/option.pdx` and `result.pdx`.
- Combinators: `map`, `unwrap_or`, `and_then`.
- 20 unit tests total.

**Touching files.** `paideia-stdlib/`.

**Deps.** pa-r17-007, pa-r17-008.

**LOC.** ~300.

---

#### pa-r18-005 — string / str stdlib layer

**Goal.** `str` is a `record { ptr: *u8, len: u64 }`; provide comparison,
substring, split, hash.

**AC.**

- `paideia-stdlib/src/str.pdx`.
- 16 unit tests.

**Touching files.** `paideia-stdlib/`.

**Deps.** pa-r17-005, pa-r17-006.

**LOC.** ~400.

---

#### pa-r18-006 — command-dispatch pattern documentation

**Goal.** Document the "semantic terminal command dispatch" pattern as a
canonical shape: hash-table from command-name → closure. This is not code;
it's a documentation deliverable to onboard paideia-os phase 11.

**AC.** Documentation in `design/toolchain/command-dispatch-pattern.md`;
worked example.

**Touching files.** Docs.

**Deps.** pa-r18-001..005.

**LOC.** ~250.

---

#### pa-r18-007 — pattern guards `match x { N if pred => ..., _ => ... }`

**Goal.** Refutable pattern with additional guard predicate.

**AC.**

- Parser: `MatchArm ::= Pattern ('if' Expr)? '=>' Expr`.
- Elaborator: guard failure re-enters the match cascade at the next arm.
- Exhaustiveness: guarded arms don't count as exhaustive (must have
  unguarded fallback).
- 12 integration tests.

**Touching files.** Parser, elaborator, tests.

**Deps.** pa-r17-008.

**LOC.** ~450.

---

#### pa-r18-008 — or-patterns `Ok(x) | Err(x) => ...`

**Goal.** Pattern with `|` alternates.

**AC.**

- Elaborator: or-pattern lowers to a conjunction over the alternates, each
  binding the same set of names to compatible-typed slots.
- 8 integration tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r17-008.

**LOC.** ~350.

---

#### pa-r18-009 — bind-and-match patterns `x @ Pat`

**Goal.** Bind the whole scrutinee to `x` while also destructuring.

**AC.**

- Elaborator: `x @ Pat` binds `x` to the pre-destructure value; inner
  pattern runs on the same value.
- 6 integration tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r17-008.

**LOC.** ~250.

---

#### pa-r18-010 — hash-based dispatch integration

**Goal.** Combine pa-r18-003 (hashmap), pa-r18-005 (str), pa-r17-004
(fnptr call) into a demonstration integration test: a 30-command terminal
scaffold that compiles and dispatches.

**AC.** Compiles clean; passes.

**Touching files.** integration tests.

**Deps.** pa-r18-001..009.

**LOC.** ~450.

---

#### pa-r18-011 (encoder) — AVX2 baseline for hash-table probe

**Goal.** SIMD probe for open-addressing hash table: load 16 metadata bytes,
compare against target, extract mask, first-set-bit → probe index.

**AC.**

- Encoder support for: `vpcmpeqb ymm, ymm, ymm`, `vpmovmskb r32, ymm`,
  `vpxor ymm, ymm, ymm`, `vmovdqu ymm, [mem]`.
- VEX prefix encoding infrastructure (3-byte VEX for AVX2).
- 20 byte-exact tests + 4 iced round-trip.
- Documented in `design/encoding/vex-prefix.md` (new).

**Touching files.** Encoder (major addition), docs, tests.

**Deps.** None (large new surface).

**LOC.** ~1200 (this is the biggest single issue in v0.18).

---

#### pa-r18-012 (encoder) — `crc32 r64, r/m64`

**Goal.** SSE 4.2 CRC32 for hash-table hashing.

**AC.**

- Encoder: `F2 REX.W 0F 38 F1 /r` (r64) and 32/16/8-bit sibling forms.
- 8 byte-exact tests + 2 iced round-trip.
- Feature gate: CPUID.01H:ECX.SSE4_2[bit 20].

**Touching files.** Encoder, tests.

**Deps.** None.

**LOC.** ~280.

### 8.2 Testing strategy — v0.18

- **Closure escape audit.** Fuzz target that generates random closures and
  verifies that captures with a lifetime shorter than the closure are
  rejected. (Requires lifetime inference to be robust — this may reveal
  latent bugs; log-only tests until v0.19.)
- **Hash-table stress.** 1000-entry insert-lookup-delete cycles against a
  reference Rust HashMap.
- **Pattern-match cost audit.** Verify that exhaustive matches over dense
  enums (≥ 4 variants) emit a jump-table, not an if-cascade.

### 8.3 Cross-release deps forward

- pa-r18-011 (AVX2) is a substrate for v0.20's WASM/SIMD lowering.
- pa-r18-001..002 (closures) are what the semantic terminal command language
  compiles into.

---

## 9. Release v0.19 — UEFI: MS x64 calling convention + GUID + embed primitives

**Theme.** Bridge to UEFI firmware. Introduce MS x64 calling convention as
an annotation, GUID literals, `@include_bytes` and `@include_str` embed
primitives.

**Exit criteria.**

- paideia-os UEFI stub compiles in `.pdx`; can invoke `BootServices` protocol
  methods.
- `@include_bytes("firmware.bin")` retires the `.incbin` GAS glue.

### 9.1 Issues

#### pa-r19-001 — `extern "ms"` calling-convention annotation

**Goal.** `extern "ms" pub fn efi_get_time(time: *EfiTime) -> u64` declares
a function callable via the MS x64 ABI.

**AC.**

- Parser: `extern` keyword; `"ms"` and `"sysv"` (the default) as ABI
  strings.
- Elaborator: the ABI is a property of the fn signature; on call, the
  emitter uses MS x64 register mapping.
- Documented in `design/toolchain/calling-convention.md` (extension).

**Touching files.** Parser, AST, elaborator, docs.

**Deps.** None.

**LOC.** ~350.

---

#### pa-r19-002 — MS x64 argument mapping

**Goal.** First 4 int/ptr args in RCX, RDX, R8, R9; further args on stack
above 32-byte shadow space.

**AC.**

- Elaborator's call-lowering pass gains an ABI-aware register-mapper:
  SysV = {RDI, RSI, RDX, RCX, R8, R9}; MS = {RCX, RDX, R8, R9}.
- Return value in RAX (both ABIs agree).
- 16 unit tests.

**Touching files.** Elaborator, tests.

**Deps.** pa-r19-001.

**LOC.** ~450.

---

#### pa-r19-003 — shadow-space frame emission

**Goal.** MS x64 caller must reserve 32 bytes of shadow space on the stack
before every call.

**AC.**

- Emitter adds 32 bytes to the call-site `sub rsp, N` in MS-ABI call
  context.
- Restored via `add rsp, N` after the call.
- 8 integration tests.

**Touching files.** Emitter, tests.

**Deps.** pa-r19-002.

**LOC.** ~350.

---

#### pa-r19-004 (encoder) — full ModR/M for MS x64 arg mapping

**Goal.** The encoder side of pa-r19-002: no new instructions, but the
`mov rcx, ...` / `mov rdx, ...` / `mov r8, ...` / `mov r9, ...` shapes
that MS ABI callers emit must all be well-tested.

**AC.** 16 fixtures across arg-count 0..8; register substitutions;
memory-load arguments; struct-by-value pass rules (deferred to v0.20).

**Touching files.** encoder audit tests.

**Deps.** pa-r19-002.

**LOC.** ~200.

---

#### pa-r19-005 (encoder) — `push imm32` and `push imm8` sign-extended

**Goal.** Stack-arg construction for MS x64 (beyond the fourth arg).

**AC.**

- Encoder: `68 id` (push imm32), `6A ib` (push imm8, sign-extended).
- 6 byte-exact tests + 2 iced round-trip.

**Touching files.** Encoder, tests.

**Deps.** None.

**LOC.** ~180.

---

#### pa-r19-006 — MS x64 callee prologue emitter

**Goal.** `extern "ms" pub fn foo() { ... }` needs a prologue that saves
non-volatile MS registers (RBX, RBP, RDI, RSI, R12–R15, XMM6–XMM15 for
FP-heavy fns).

**AC.**

- Elaborator emits prologue based on function-body register-use analysis.
- 10 unit tests including RSP-alignment invariant (RSP mod 16 == 0 after
  prologue).

**Touching files.** Elaborator, tests.

**Deps.** pa-r19-001.

**LOC.** ~420.

---

#### pa-r19-007 — GUID literal `@guid("12345678-1234-1234-1234-123456789abc")`

**Goal.** UEFI protocols identified by GUID.

**AC.**

- Parser recognises `@guid("...")` attribute on a `pub let` binding.
- Elaborator lowers to a 16-byte `.rodata` blob with UEFI's mixed-endian
  layout (first 8 bytes little-endian per subfield, last 8 big-endian).
- Diagnostic **P0270** for malformed GUID.
- 8 unit tests.
- Documented in `design/toolchain/guid-literal.md` (new).

**Touching files.** Parser, elaborator, docs, tests.

**Deps.** None.

**LOC.** ~350.

---

#### pa-r19-008 — `@include_bytes("path/to/file")` embed primitive

**Goal.** Retire the `.incbin` GAS shim for embedding a firmware blob at a
symbol.

**AC.**

- Parser: `pub let X : [u8; N] = @include_bytes("path/to/file")` — grammar
  and validation.
- Elaborator: reads the file at compile time (relative to the source file's
  directory or to a `--include-dir` search list); enforces size ≤ N; emits
  as `.rodata` (or `.data` if `mut`) with the file's exact bytes.
- Path resolution: relative to including file; error P0279 if not found (see issue #1013).
- Diagnostic **P0280** for size mismatch (file too large > 16 MiB). T0558 guards type mismatch.
- 10 unit tests including read-my-own-source-file recursion detection.

**Touching files.** Parser, elaborator, tests, docs.

**Deps.** None.

**LOC.** ~600.

---

#### pa-r19-009 — `@include_str("path")` and `@include_bytes_as_str`

**Goal.** Sibling for text embeds.

**AC.**

- Same shape as pa-r19-008 but produces a `str` value.
- UTF-8 validation.
- 6 unit tests.

**Touching files.** Parser, elaborator, tests.

**Deps.** pa-r19-008.

**LOC.** ~250.

---

#### pa-r19-010 — `@link_section("name")` attribute

**Goal.** Emit a symbol to a non-default section (paideia-os wants `.uefi_hdr`
for the UEFI header).

**AC.**

- Parser: `@link_section("name")` on `pub let` or `pub fn`.
- Emitter: honors the section name; validates section-name characters.
- Diagnostic **P0273** for invalid section name.
- 8 unit tests.

**Touching files.** Parser, elaborator, emitter, tests.

**Deps.** None.

**LOC.** ~400.

---

#### pa-r19-011 — UEFI header emission helpers

**Goal.** paideia-stdlib exports a helper macro that composes a valid UEFI
image header (DOS stub + PE32+ header) for the paideia-os UEFI loader.

**AC.**

- `paideia-stdlib/src/uefi.pdx` (new).
- 4 integration tests via a minimal UEFI app fixture.

**Touching files.** `paideia-stdlib/`, tests.

**Deps.** pa-r19-010, pa-r19-008.

**LOC.** ~500.

---

#### pa-r19-012 — MS x64 <-> SysV bridge thunks

**Goal.** Formalise auto-generated thunks at ABI boundaries.

**AC.**

- When paideia-native code calls `extern "ms" fn`, the emitter inserts a
  save-R15 + save-R12-R13 + swap-argmap + call + restore prologue/epilogue.
- Similarly inverse (MS caller → paideia-native callee) is documented per
  §2.5 of `calling-convention.md`.
- 8 unit tests.

**Touching files.** Elaborator, calling-convention doc, tests.

**Deps.** pa-r19-002, pa-r19-006.

**LOC.** ~500.

---

#### pa-r19-013 — UEFI stub integration test

**Goal.** Full stub compiles end-to-end.

**AC.** Compiles clean; PE32+ output validated by size/section audit.

**Touching files.** integration tests.

**Deps.** pa-r19-001..012.

**LOC.** ~350.

### 9.2 Testing strategy — v0.19

- **ABI-cross-call corpus.** 20 fixtures: SysV → MS, MS → SysV, nested calls
  across ABI boundaries; verify argument mapping and shadow-space are
  correct via a byte-exact fixture reference.
- **Embed-primitive fuzz.** Fuzz `@include_bytes` with random path strings;
  ensure malicious paths (with `../..`) are rejected or normalised as
  documented.

### 9.3 Cross-release deps forward

- pa-r19-008 (embed primitive) unblocks v0.20's runtime-library shape: the
  WASM jail wants to embed its own bytecode.

---

## 10. Release v0.20 — Self-hosting shape

**Theme.** Ship `libpaideia-as-runtime` as a linkable library so paideia-os
processes (WASM jail, dynamic-code paths) can emit code at runtime. Groundwork
for full self-hosting (v0.21+, paideia-as running on paideia-os).

**Exit criteria.**

- `crates/paideia-as-runtime` (new) crate ships a public API for
  `emit_instruction(&mut Buf, Instruction)` and `resolve_symbols(&mut
  Buf, &SymbolTable)`.
- paideia-os phase 10 WASM/VM userspace uses this API to lower WASM
  bytecode.
- PQ signature discipline (see paideia-pq-sign) audit passes on the runtime
  library.

### 10.1 Issues

#### pa-r20-001 — extract `paideia-as-runtime` crate

**Goal.** Split the encoder + elaborator IR type into a `no_std`-friendly
library crate.

**AC.**

- New crate `crates/paideia-as-runtime/`.
- Re-exports IR types + a public `emit_instruction` function.
- Compiles with `no_std` + `alloc` only.
- Existing CLI (`paideia-as build`) depends on the runtime crate.
- 8 unit tests.

**Touching files.** New crate + workspace `Cargo.toml`, existing CLI
dependency changes.

**Deps.** None.

**LOC.** ~600 (mostly moves).

---

#### pa-r20-002 — public `emit_instruction(&mut Buf, Instruction)` API

**Goal.** Stable public entry point for downstream (paideia-os WASM jail).

**AC.**

- API signature: `fn emit_instruction(buf: &mut CodeBuffer, ins:
  Instruction) -> Result<(), EmitError>`.
- Error variants documented; `EmitError` is a stable public enum.
- 12 unit tests including error paths.

**Touching files.** `paideia-as-runtime/`.

**Deps.** pa-r20-001.

**LOC.** ~400.

---

#### pa-r20-003 (encoder) — `endbr64` and `endbr32` (CET)

**Goal.** Indirect-branch target markers for Control-flow Enforcement
Technology.

**AC.**

- Encoder: `F3 0F 1E FA` (endbr64), `F3 0F 1E FB` (endbr32).
- 4 tests (byte-exact + iced for each).
- Emitter: every `pub fn` whose address is taken (via `&fn_name` in a
  fnptr initialiser) or every `extern "ms"` function gets an `endbr64`
  prepended.
- Feature gate documented.

**Touching files.** Encoder, elaborator, tests.

**Deps.** pa-r17-003.

**LOC.** ~350.

---

#### pa-r20-004 (encoder) — `xsaveopt`, `xrstor` (extended state)

**Goal.** AVX-512 context switch support.

**AC.**

- Encoder: `0F AE /6` (xsaveopt), `0F AE /5` (xrstor); memory operand.
- 6 byte-exact tests + 2 iced round-trip.
- Requires EDX:EAX to hold state-component bitmap (implicit).

**Touching files.** Encoder, tests.

**Deps.** None.

**LOC.** ~280.

---

#### pa-r20-005 — dynamic-emit example: WASM `i32.add` lowering

**Goal.** Demonstration example lowering a WASM `i32.add` opcode into
x86_64 `add r32, r32` via the runtime API. Documents the pattern for
paideia-os phase 10.

**AC.**

- Example in `crates/paideia-as-runtime/examples/wasm_add.rs`.
- Documented in `design/toolchain/dynamic-emit.md` (new).

**Touching files.** `paideia-as-runtime/examples/`, docs.

**Deps.** pa-r20-002.

**LOC.** ~350.

---

#### pa-r20-006 — symbol resolution API

**Goal.** Runtime library exposes `resolve_symbols(&mut Buf, &SymbolTable)`
so callers can plug in kernel-provided symbol addresses.

**AC.**

- Public API + 8 unit tests.
- Documented in `design/toolchain/dynamic-emit.md`.

**Touching files.** `paideia-as-runtime/`.

**Deps.** pa-r20-002.

**LOC.** ~350.

---

#### pa-r20-007 — PQ signature discipline for runtime library

**Goal.** Every artifact produced by `paideia-as-runtime` (in the paideia-os
context, this is JIT-emitted WASM code) must be ML-DSA-signed before
paideia-os accepts it for execution.

**AC.**

- `paideia-pq-sign` gains a `sign_runtime_buffer(&[u8]) -> Signature`
  entry point.
- Runtime library ships an example integrating the sign step.
- Documented in `design/security/pq-runtime-signing.md` (new).

**Touching files.** `paideia-pq-sign/`, docs, examples.

**Deps.** pa-r20-002.

**LOC.** ~450.

---

#### pa-r20-008 — v0.20 API-freeze test suite

**Goal.** Assert public API stability of the runtime crate; break-detection
via a snapshot of the pub type + fn signatures.

**AC.**

- Test suite that reads `paideia-as-runtime/src/lib.rs` and asserts the
  pub surface matches a checked-in signature file.
- Any breaking API change requires updating the signature file (visible
  in PR review).

**Touching files.** New test file + signature snapshot.

**Deps.** pa-r20-002, pa-r20-006.

**LOC.** ~200.

---

#### pa-r20-009 — self-hosting audit

**Goal.** Enumerate what still blocks `paideia-as build` from running on
paideia-os. Not code; a written audit that seeds the v0.21+ roadmap.

**AC.**

- Written audit at `design/roadmap/self-hosting-audit.md` (new).
- Ranked list of blockers: filesystem access, argv parsing, stdout, ...
- Cross-reference to paideia-os phase-14 planning.

**Touching files.** Docs.

**Deps.** pa-r20-001..008.

**LOC.** ~400.

---

#### pa-r20-010 — v0.20 integration + release notes

**Goal.** Compile v0.12→v0.20 delta into a summary release note; bump
crates/version + tag.

**AC.**

- Release note in `CHANGELOG.md`.
- All crate versions bumped consistently.
- Tag `v0.20.0` pushed.

**Touching files.** `CHANGELOG.md`, all crate `Cargo.toml`, tag.

**Deps.** pa-r20-001..009.

**LOC.** ~200.

### 10.2 Testing strategy — v0.20

- **Runtime-emit fuzz.** Fuzz target that generates random `Instruction`
  values, emits, then iced-decodes; every round-trip must be lossless
  or return a documented `EmitError`.
- **Public-API stability CI check.** Signature snapshot enforced.

### 10.3 Cross-release deps forward

- pa-r20-002 (runtime emit API) is the foundation for paideia-os phase 10
  (WASM/VM userspace).
- pa-r20-007 (PQ signature discipline) is required for paideia-os phase 13
  (PQ trust root).

---

## 11. Calling-convention formalisation (cross-release)

The current `design/toolchain/calling-convention.md` documents SysV as the
default with §2.5 sketching a UEFI thunk. This roadmap requires that
sketch to firm up into machinery.

### 11.1 ABI-tagged fn types

The type of a function now carries an ABI tag:

```
FnType ::= 'extern' '"' AbiName '"' ? '(' Params ')' '->' RetType ('!' EffectRow)?
AbiName ::= 'sysv' | 'ms' | 'paideia'
```

- `paideia` is the default; unnamed `extern` clauses assume `sysv`.
- `paideia` and `sysv` are register-layout-compatible for the first six
  int/ptr args; they differ in R14/R15 discipline.
- `ms` uses the Microsoft x64 register set.

### 11.2 ABI-tagged fnptr types (v0.17 opens, v0.19 closes)

A fnptr type may or may not carry an ABI tag. Untagged fnptrs default to
`paideia`. `let f : extern "ms" (u64) -> u64 = &efi_get_time;` binds a
MS-ABI fnptr.

**Assignment rule.** A fnptr of ABI `A` may only be assigned from a value
of ABI `A`; there is no implicit thunk generation. Explicit thunk creation
is via a macro that wraps a fn into the other ABI.

### 11.3 Effect-row extension for ABI boundary

When paideia code calls an MS-ABI function, an implicit effect
`!{AbiBoundary(ms)}` is added to the caller's effect row (surfaces the
"we crossed an ABI, R14/R15 discipline was suspended" fact to the effect
inference system).

### 11.4 Register partition per ABI

| Register | paideia (default) | sysv        | ms          |
|----------|-------------------|-------------|-------------|
| RAX      | scratch / retval  | scratch     | scratch     |
| RCX      | arg / scratch     | arg 4       | arg 1       |
| RDX      | arg / scratch     | arg 3       | arg 2       |
| RBX      | callee-saved      | callee-saved| callee-saved|
| RSI      | arg / scratch     | arg 2       | callee-saved|
| RDI      | arg / scratch     | arg 1       | callee-saved|
| RBP      | frame ptr         | frame ptr   | callee-saved|
| R8       | arg / scratch     | arg 5       | arg 3       |
| R9       | arg / scratch     | arg 6       | arg 4       |
| R10-R11  | scratch           | scratch     | scratch     |
| R12–R13  | capability (LAM)  | callee-saved| callee-saved|
| R14      | closure env ptr   | callee-saved| callee-saved|
| R15      | effect env ptr    | callee-saved| callee-saved|

### 11.5 Cross-ABI thunk emission (v0.19)

- **paideia → sysv:** save R14, R15; move args from paideia arg-mapping to
  sysv arg-mapping (no-op if paideia inherits from sysv); call; restore R14,
  R15.
- **paideia → ms:** save R14, R15; move args paideia-map → ms-map;
  allocate 32-byte shadow space; call; deallocate; restore.
- **ms → paideia:** stub the closure-env register R14 to zero (no closure
  env); zero R15 (no handler env); call; restore RSP alignment.
- **sysv → paideia:** move args sysv → paideia (no-op for first six);
  R15 = zero; call.

### 11.6 Testing

- **Symmetric round-trip suite.** Every ABI pair (A, B) has a test:
  `fn a_calls_b_calls_a() { ... }` verifies register discipline holds
  end-to-end.

---

## 12. Function-pointer type system (cross-release)

Function pointers are a **first-class value** starting v0.17.

### 12.1 Grammar

- **Type:** `(T1, T2) -> R` (SysV/paideia default); `extern "ms" (T1, T2) ->
  R` (ABI-tagged).
- **Type variance:** invariant in parameter and return types; variance in
  effect row is subtyping (subset assignable to superset — see §11.3).
- **Nullability:** fnptr types are **non-nullable**. `Option<(u64) -> u64>`
  is the way to express nullability. Zero-address fnptr assignment is
  rejected at compile time.

### 12.2 Value construction

- **Address-of a `pub fn`:** `&fn_name`. Type: `sig(fn_name)`.
- **Load from a fnptr-typed memory location:** `[some_ptr]` where
  `some_ptr : *fnptr-type`. Type is the fnptr type.
- **Record-field access:** `vops.read` where `vops` is a record.
- **Closure literal (v0.18):** `|params| body`. Type: closure type
  (fnptr + env-ptr pair).

### 12.3 Lowering

- **Global fnptr in `.data`:** compile-time-resolvable symbol relocation.
  Emitted as `R_X86_64_64` in initialiser.
- **Local fnptr in a register:** loaded via `mov reg, [rip + sym]` or
  `mov reg, [struct + offset]`; the register holds the raw code address.
- **`call fnptr`:** dispatches to `call reg` (pa-r13-003), `call [mem]`
  (pa-r13-003 mem form), or `call [rip + sym]` (pa-r17-015) depending on
  where the fnptr value lives.

### 12.4 Interaction with effect system

- **Fnptr type includes an effect row.** `(u64) -> u64 !{Alloc}` and
  `(u64) -> u64 !{}` are distinct types.
- **Effect assignment rule (subset).** `f : (u64) -> u64 !{Alloc, IO}`
  may be assigned to a value of type `(u64) -> u64 !{Alloc, IO,
  UserFault}` (widening — safe: the target expects the fnptr to have at
  least those effects, and the actual fnptr has fewer).
- Enforced at elaboration by `check_effect_subset` (extended from
  `check_pure.rs`).

### 12.5 Interaction with capability/linearity

- Fnptr values are **Unrestricted** (like `*T`; the linearity is on the
  callee's local state, not the pointer).
- A fnptr may **not** capture a capability register (R12/R13) implicitly;
  closures over capability values are rejected with **T0537**.

### 12.6 Reflection

`crates/paideia-as-reflect/` (deferred to v0.21) exposes reflection over
fnptr signatures via effect and calling-convention rows. Not in scope for
this roadmap.

---

## 13. Discriminated unions and records (cross-release)

### 13.1 Records (m7 baseline, elaboration depth in v0.17)

- Grammar and layout algorithm: shipped at m7 (see
  `design/toolchain/records-enums-phase4.md`).
- **v0.17 additions:** field-access lowering with correct width
  (pa-r17-005); field-assignment lowering (pa-r17-006); global initialiser
  with fnptr fields (pa-r17-010).
- Records are unrestricted-linearity by default; a `linear record { ... }`
  form is possible but deferred to v0.21.

### 13.2 Discriminated unions / enums (m7 baseline, deep support in v0.17)

- Grammar: `enum { Variant1, Variant2(T), Variant3 { f: T } }`.
- Layout: 8-byte discriminant + max-payload region.
- **v0.17 additions:** variant construction (pa-r17-007), pattern-match with
  discriminant-cascade or jump-table (pa-r17-008), nested pattern binding
  (pa-r17-009).
- **v0.18 additions:** pattern guards (pa-r18-007), or-patterns
  (pa-r18-008), bind-and-match (pa-r18-009).

### 13.3 Exhaustiveness

Already checked at m7-009. v0.18 pattern-guard support requires the check
to distinguish guarded from unguarded arms.

### 13.4 Interior discriminant layout choice

- **Default:** 8-byte tag before payload. Simple, uniform, cache-line safe.
- **Niche optimisation:** if the enum has a variant with a payload whose
  type has a "niche" (e.g. a fnptr, always non-zero), reuse that niche for
  the discriminant. **Deferred to v0.21** — Rust's approach informs but
  isn't copied.

### 13.5 Interaction with SIMD / hash tables

Records ≤ 32 bytes may be probed as an AVX2 metadata block in
`paideia-stdlib/src/hashmap.pdx`. This is a v0.18 concern.

---

## 14. Embed primitives

### 14.1 `@include_bytes("path")`

- **Grammar:** `@include_bytes("relative/or/absolute/path")` as the RHS of a
  `pub let X : [u8; N] = ...` at module scope.
- **Path resolution:** relative to the including source file, or from a
  search list configurable via `paideia-as build --include-dir DIR`.
- **Size validation:** N must match the file's byte count; elaborator diagnoses
  **T0558** on mismatch (and P0280 if file > 16 MiB).
- **Section routing:** `.rodata` (default) or `.data` (if `mut`);
  overridable via `@link_section("...")`.
- **Retires:** paideia-os's `.incbin`-in-a-`.S`-file workaround for
  embedding boot artifacts.

### 14.2 `@include_str("path")` and `@include_bytes_as_str("path")`

- `@include_str("path")` produces a `str` value; UTF-8 validation.
- `@include_bytes_as_str("path")` produces a `str` without validation
  (unsafe; caller asserts UTF-8 validity via an `unsafe { }` context).

### 14.3 `@link_section("name")`

- Places a symbol in a specific ELF section (or PE section, for MS ABI).
- Section-name characters validated (`[a-zA-Z0-9._-]+`, ≤ 32 chars).

### 14.4 `@align(N)` (already shipped as PA10-006y, #878)

Retained.

### 14.5 `@ring(slots=N, slot_size=M)` (v0.14)

Ring-buffer synthesis attribute (see pa-r14-008).

### 14.6 `@guid("...")` (v0.19)

UEFI GUID literal (see pa-r19-007).

### 14.7 `@jump_table` (v0.15)

Attribute on a `match` expression that forces jump-table emission (see
pa-r15-009).

### 14.8 Attribute-vocabulary discipline

- **Reserved prefix:** `@` is reserved for compiler-known attributes.
- **Unknown attribute:** parser diagnostic **P0250** (already shipped).
- **New attribute registration process:** every new `@`-attribute must ship
  with a doc under `design/toolchain/attribute-<name>.md`.

---

## 15. Regression discipline: v0.12+ API compatibility for paideia-os

### 15.1 What paideia-os depends on today (v0.12.0 surface)

- Every instruction in the `Mnemonic` enum currently exposed (Mov, Add, Sub,
  Cmp, Test, Jcc, Jmp, Call, Ret, RepMovsb, Lea, Lgdt, Lidt, MovCr, MovDr,
  Wrmsr, Rdmsr, In, Out, Iret, Iretq, Sysret, Syscall, Swapgs, Cpuid, Cli,
  Sti, Hlt, Int, Nop, RepStosq, FarJmp, Movzx, Movsx, Not, Push, Pop,
  Pushfq, Popfq, Int3, MovSized, Shl, Shr, Sar, Imul, And, Or, Xor,
  Invlpg, Rdtsc, Div, Idiv, Ltr, Xchg, LockCmpxchg, Mfence, Fxsave,
  Fxrstor).
- Every `Operand` variant (Reg, SegReg, Imm64, MemSib, MemDisp, MemRipRel,
  MemSeg, SymbolRef, LabelRef, Var).
- Attribute vocabulary at v0.12: `@align`.
- Parser surface: modules, `pub let`, `pub fn`, `unsafe { }`, `let`,
  `match`, `if`/`else`, `while`, `loop`.

### 15.2 Compatibility guarantees (through v0.20)

- **Additive additions only** to the `Mnemonic` enum. Removing a variant or
  changing its meaning is a breaking change gated by a major version bump.
- **Additive additions only** to `Operand`. New variants are additive; the
  existing variants keep their meaning.
- **Additive additions only** to attribute vocabulary. New `@`-attributes
  are additive; existing ones keep their meaning.
- **Public API of `paideia-as-runtime` (from v0.20)** — snapshot-tested;
  breaking changes require version bump + CHANGELOG note.

### 15.3 Acceptable breaking changes and their gating

- **Effect-row addition to fnptr types (v0.17).** This is technically a
  parser-level break: today `(u64) -> u64` parses as a fnptr type but
  isn't a first-class binding target. v0.17 gives that syntax semantic
  meaning. Acceptable because it opens a new capability; paideia-os
  submodule bump is coordinated.
- **ABI tag in fn declarations (v0.19).** `extern "ms"` prefix is new; not
  a break of existing code.
- **`@include_bytes` grammar (v0.19).** New attribute; no break.

### 15.4 Deprecation policy

- No deprecations planned through v0.20.
- Post-v0.20: attribute deprecation cycle (2-release warning) if any
  emerges.

### 15.5 Version-discipline reminder

Per `feedback_paideia_as_version_discipline.md`: `workspace.version` + git
tag + CHANGELOG entry move together at each phase close. `find-paideia-as.sh`
stays strict.

Each release above (v0.13 → v0.20) triggers a version bump at close of the
release.

---

## 16. Cross-release dependency graph

### 16.1 Hard dependencies (later release blocked until earlier lands)

```
pa-r13-001 (narrow mov r/m8/16/32 load)
    → pa-r14-002 (mov r32 mem full)
    → pa-r17-005 (record field access with correct width)

pa-r13-003 (call indirect)
    → pa-r14-009 (fnptr dispatch unsafe pattern)
    → pa-r17-004 (fnptr call lowering)
    → pa-r17-015 (call [rip + sym])
    → pa-r18-002 (closure call)

pa-r13-013 (setcc)
    → pa-r17-005 (bool record fields via setcc)

pa-r14-001 (mov [mem], imm narrow)
    → pa-r17-006 (record field assignment)

pa-r15-009 (jump table)
    → pa-r17-008 (enum match dense jump)
    → pa-r18-010 (hash dispatch)

pa-r16-004 (cmpxchg16b)
    → pa-r18-003 (hashmap lock-free tail)

pa-r17-004 (fnptr call lowering)
    → pa-r18-001..002 (closures)

pa-r17-001 (fnptr grammar)
    → pa-r18-001 (closure grammar; extends fnptr)
    → pa-r19-001 (extern "ms" — ABI tag on fnptr type)

pa-r18-011 (AVX2 baseline)
    → pa-r20-005 (WASM SIMD lowering)

pa-r19-002 (MS argmap)
    → pa-r19-012 (bridge thunks)

pa-r20-001..002 (runtime crate + emit API)
    → pa-r20-005..007 (WASM example + symbol resolve + PQ sign)
```

### 16.2 Soft dependencies (would-benefit-from ordering)

- pa-r15-004 (rol/ror) before pa-r18-003 (hash function).
- pa-r13-007 (test reg, reg) before pa-r14-011 (peephole).
- pa-r14-010 (effect vocabulary) before pa-r15-007 (byte-parse macros use
  RawMem).

### 16.3 Blocking chains (longest path)

Longest hard-dependency chain:

```
pa-r13-003 (v0.13, ~2 wk)
    → pa-r17-004 (v0.17, ~4 wk)
    → pa-r18-002 (v0.18, ~2 wk)
    → pa-r20-005 (v0.20, ~1 wk)
```

Total: ~9 person-weeks on the critical path. Other work parallelises around
it.

---

## 17. Testing strategy (round-trip corpus + fuzz layers)

### 17.1 Round-trip corpus growth per release

| Release | Corpus rows added | Cumulative |
|---------|-------------------|------------|
| v0.13   | ~200 (encoder gap wave)   | ~500 |
| v0.14   | ~120 (MMIO + rings)       | ~620 |
| v0.15   | ~80 (network prims)       | ~700 |
| v0.16   | ~100 (atomics + bitmap)   | ~800 |
| v0.17   | ~180 (fnptr + enum match) | ~980 |
| v0.18   | ~140 (closures + hash)    | ~1120 |
| v0.19   | ~140 (UEFI + embed)       | ~1260 |
| v0.20   | ~80 (runtime + JIT)       | ~1340 |

### 17.2 Fuzz layers introduced

- **v0.13:** REX-prefix audit fuzz — iterate all register triples and
  assert prefix correctness against a hand-authored table.
- **v0.14:** effect-inference fuzz — random programs; assert effect row
  inference is stable.
- **v0.15:** byte-parse OOB fuzz — random buffers, random offsets;
  effect analysis catches OOB.
- **v0.16:** concurrent-correctness fuzz — Loom/Shuttle-style model
  checking on freelist ops.
- **v0.17:** fnptr signature fuzz — random signatures; assignment
  compatibility.
- **v0.18:** closure escape fuzz — captured-lifetime shorter than
  closure lifetime → rejected.
- **v0.19:** ABI-cross-call fuzz — random SysV↔MS boundaries; register
  discipline holds.
- **v0.20:** runtime-emit fuzz — random Instruction; round-trip.

### 17.3 Test-count budget

Baseline at v0.12.0: ~3200 tests. Projected at v0.20.0: **~4800 tests**
(the +1600 comes from the corpus growth in §17.1 plus per-issue AC test
counts summed across §3..§10 which come to ~800 unit + 400 integration).

### 17.4 Continuous-encoder-audit test

`tests/prefix_audit.rs` (v0.13) is the long-lived regression net: every
new mnemonic must add a row; the CI job re-runs the audit and any silently
skipped instruction fails.

### 17.5 paideia-os canary tests

- **v0.13.0 → paideia-os R14 close:** `boot_stub` still compiles; no
  regression on R13-cycle fixtures.
- **v0.14.0 → phase-5 (block driver) end-to-end:** driver stub compiles.
- **v0.17.0 → phase-6 (VFS) end-to-end:** vops table compiles.
- **v0.20.0 → phase-10 (WASM/VM) end-to-end:** JIT stub compiles.

Each canary lives in `crates/paideia-as-test/tests/paideia_os_canary.rs`
(new). The paideia-os side runs its own canaries; this file guards against
paideia-as-side regressions.

---

## 18. Top-10 highest-impact issues

Ranked by how much paideia-os work each unblocks (subjective; each entry
lists the phases it touches).

1. **pa-r13-003 — indirect call (mem + reg).** Every VFS/vops dispatch,
   every WASM/VM opcode dispatch, every closure invocation flows through
   this. Blocks: phase 6 VFS, phase 8 network, phase 10 WASM, phase 11
   semantic terminal.

2. **pa-r17-004 — first-class fnptr call lowering.** Retires every
   `unsafe { call reg }` workaround. Blocks: phase 6, phase 10, phase 11.

3. **pa-r16-002 — locked bit-ops (bts/btr/btc).** Real phys_free bitmap.
   Blocks: phase 9 CoW FS core allocation.

4. **pa-r17-008 — enum pattern-match without unsafe.** Every kernel state
   transition ("what kind of message?", "what protocol?"). Blocks: phase
   6, phase 7, phase 8, phase 11.

5. **pa-r13-001 — narrow-width mem load (#927).** Every u8/u16/u32 field
   read from a memory-resident struct. Blocks: phase 5, phase 7, phase 8,
   phase 11 (records).

6. **pa-r16-004 — cmpxchg16b (DCAS).** ABA-free freelists. Blocks: phase
   9 CoW FS + phase 13 SMP hardening.

7. **pa-r18-003 — HashMap stdlib.** Command dispatch, symbol tables,
   ambient path lookup. Blocks: phase 11.

8. **pa-r19-001..006 — MS x64 calling convention.** UEFI. Blocks: phase 12.

9. **pa-r19-008 — @include_bytes.** Retires GAS `.incbin` shim. Blocks:
   phase 12 UEFI + any phase that embeds a signed blob.

10. **pa-r20-002 — runtime emit API.** Foundation of dynamic-code (WASM
    JIT, semantic-terminal command compilation, on-line paideia-as tools).
    Blocks: phase 10 WASM/VM, phase 11 semantic terminal, phase 14
    self-hosting.

---

## Appendix A. Provisional per-release issue-count summary

| Release | Encoder | Elaborator / IR | Stdlib | Docs / infra | Total |
|---------|---------|-----------------|--------|--------------|-------|
| v0.13   | 12      | 2               | 0      | 0            | 14    |
| v0.14   | 6       | 3               | 2      | 1            | 12    |
| v0.15   | 6       | 2               | 3      | 0            | 11    |
| v0.16   | 8       | 0               | 3      | 1            | 12    |
| v0.17   | 2       | 11              | 0      | 2            | 15    |
| v0.18   | 2       | 6               | 3      | 1            | 12    |
| v0.19   | 2       | 6               | 1      | 4            | 13    |
| v0.20   | 2       | 0               | 0      | 8            | 10    |
| **Total** | **40** | **30**          | **12** | **17**       | **99** |

## Appendix B. Provisional per-release LOC estimate

| Release | Encoder LOC | Elaborator LOC | Stdlib LOC | Docs / infra LOC | Total |
|---------|-------------|-----------------|------------|-------------------|-------|
| v0.13   | 3100        | 800             | 0          | 400               | 4300  |
| v0.14   | 1900        | 900             | 950        | 300               | 4050  |
| v0.15   | 1300        | 700             | 1250       | 100               | 3350  |
| v0.16   | 2200        | 0               | 890        | 300               | 3390  |
| v0.17   | 430         | 4600            | 0          | 500               | 5530  |
| v0.18   | 1480        | 1900            | 1400       | 350               | 5130  |
| v0.19   | 380         | 2500            | 500        | 900               | 4280  |
| v0.20   | 630         | 0               | 0          | 2350              | 2980  |
| **Total** | **11420** | **11400**       | **4990**   | **5200**          | **33010** |

## Appendix C. Notes for the closing bump

At each release close:

1. Bump `workspace.version` in the top-level `Cargo.toml`.
2. Tag `v<X>.<Y>.0` on `main`.
3. Add a CHANGELOG entry mirroring the v0.12.0 shape (this file's structure).
4. Bump the paideia-os `tools/paideia-as` submodule pointer.
5. Update paideia-os `design/toolchain/` references to the new features.
6. `find-paideia-as.sh` remains strict.
7. `feedback_paideia_os_no_cicd.md` remains in force; verification is
   local-only.

## Appendix D. Open questions carried into v0.13 opening

- **Q1.** Is pa-r13-011 / pa-r13-012 (rep_movsb/stosq robustness) really an
  encoder bug, or an elaborator label-alias regression that #924 already
  fixed? Reproduce on a clean checkout before spending encoder effort.
- **Q2.** For pa-r13-010 (or-imm64 expansion), does reserving R11 as
  expander scratch conflict with any paideia-native discipline? Cross-check
  with `calling-convention.md` §1.
- **Q3.** For pa-r17-011 (enum-return-by-pointer), what's the size cutoff?
  16 bytes (RAX:RDX) is proposed; SysV would use 16-byte struct-return
  cutoff too, but paideia's Capability-carrying enums may want a higher
  cutoff (e.g. 24 bytes) so `Result<Capability, Error>` fits in
  R12:R13:RAX. Design-time decision, resolved before v0.17 opens.
- **Q4.** For pa-r19-007 (@guid), is the mixed-endian layout the correct
  choice? Cross-check with EDK II source before v0.19 opens.
- **Q5.** For pa-r20-007 (PQ runtime signing), how does the paideia-as
  runtime library integrate with paideia-os's PQ trust root
  (`design/security/pq-trust-root.md`)? Coordination point.

---

**End of document.**
