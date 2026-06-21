# Phase 5 retrospective

**Status:** Phase 5 m7-001 closure note.
**Scope:** Documents the Phase 5 → Phase 6 transition for paideia-as.

This Phase 5 is an **earlier** Phase 5 than the originally-planned self-hosting work in
`self-hosting-phase5-plan.md`. The cross-repo escalation from paideia-os to paideia-as on
2026-06-20 — when paideia-os Phase-1 boot code surfaced a `lower_add_one` placeholder in
`paideia-as build` — re-prioritised the work. Self-hosting shifts to Phase 6+; see §5.

## 0. Scope summary

Phase 5 ran m1 through m7 across 38 enumerated issues, PRs #695–#733. The whole arc
served one goal: unblock paideia-os Phase-1 by making `paideia-as build` emit real x86_64
machine code from `.pdx` source. The milestone slicing was build-emit-shaped, not
substrate-shaped:

- **m1** (5 issues, elab-lowering): EmitWalker skeleton; Let-Literal lowering; Lambda body
  lowering (identity / double / add-immediate); Unsafe delegation surface; `cmd_build`
  chain wiring.
- **m2** (10 issues, encoder boot-ISA): 20 new `Mnemonic` variants and encoders for the
  boot-intrinsics subset — zero-op (`cli`, `sti`, `hlt`, `nop`, `swapgs`, `cpuid`); I/O
  (`in`/`out` × 8/16/32-bit); MSR (`wrmsr`, `rdmsr`, `int`); CR/DR moves;
  `lgdt`/`lidt`; `iret`/`iretq`/`sysret`; `rep stosq`; far-`jmp`.
- **m3** (5 issues, unsafe walker): `IrKind::RawInstruction` IR node; operand parser;
  mnemonic resolver; `UnsafeWalker` (originally landed broken, see §4); `cmd_build`
  wiring. New diagnostics U1605 + U1606.
- **m4** (4 issues, static data): `[T; N]` array type; array literals; `.rodata`/`.data`
  section population; `R_X86_64_PC32` relocation linking. New diagnostic P0210.
- **m5** (5 issues, symbols + relocs): `SymbolTable`; `Operand::SymbolRef` with
  `RelocSite`; SymbolTable emission into ELF64; undefined-symbol entries; real `.text`
  emission via `InstructionSideTable` (the `lower_add_one` hardcoded shim finally
  retired).
- **m6** (5 issues, end-to-end smoke): `uart_smoke.pdx` fixture; `link.ld`;
  `tools/run-smoke.sh`; byte-sequence assertion harness; QEMU smoke under `cargo test`;
  `add_one_byte_identical` regression test (the unblock marker).
- **m7** (4 issues, docs closure): this retrospective; STATUS.md update; v0.5.0 tag;
  examples README parity refresh.

## 1. Per-milestone outcomes

| Milestone | Issues | Headline outcome                                                       |
|-----------|--------|------------------------------------------------------------------------|
| m1        | 5      | EmitWalker skeleton + lambda body lowering + cmd_build chain.          |
| m2        | 10     | 20 boot-ISA mnemonics + encoders; zero-op, I/O, MSR, CR/DR, GDT/IDT.   |
| m3        | 5      | RawInstruction IR + UnsafeWalker + U1605/U1606.                        |
| m4        | 4      | `[T; N]` arrays + .rodata/.data + R_X86_64_PC32 + P0210.               |
| m5        | 5      | SymbolTable + RelocSite + undefined-symbol entries; lower_add_one out. |
| m6        | 5      | uart_smoke + link.ld + run-smoke.sh + QEMU + byte-identical regression.|
| m7        | 4      | retrospective + STATUS + v0.5.0 + examples README refresh.             |

## 2. Phase-4 carryover disposition

Phase 4 closed clean — `phase-transition-4.md` §5 enumerated 19 substrate items targeted
at the original (self-hosting) Phase 5. None of those were Phase-4-carried bugs; they
were the planned Phase-5 forward agenda. Phase 5's actual work (build-emit activation)
was reactive to paideia-os Phase-1, not Phase-4-derived. The §5 substrate list reissues
to Phase 6+ unchanged; see §5 below.

## 3. What didn't ship (honest list)

- **`paideia-as build` for records / generics / traits / borrowed-refs / stdlib types**:
  still placeholder. Phase 5 deliberately scoped to the surface paideia-os Phase-1 boot
  code uses — `let` / `fn` / `lambda` / `unsafe` / `*T`. Wider surface lowering is Phase
  6+.
- **All lambda body shapes** (m1-003 scope): identity, double, add-immediate ship.
  Curried 2-arg `add l r → l + r` is explicitly skipped (m6-005 carved it out). Phase 6+.
- **Full RIP-relative addressing**: only m2-010 far-`jmp` uses RIP-relative encoding
  directly. General-case `mov rax, [rip + symbol]` works via `SymbolRef` + `RelocSite`
  but the encoder takes the conservative path. Phase 6+ optimisation.
- **The original Phase 5 self-hosting work**: all 5 stdlib expansions (SmallVec / Unicode
  XID / serde-family / BLAKE3 / Lru) + Tier 1-3 self-host shift to Phase 6+. Detailed
  carryover in §5.
- **`lower_add_one` is killed but stays as a benchmark fixture**: m5-005 removed it from
  the `cmd_build` hot path; it lives on as a unit-benchmark for the old shim. Pure
  code-archaeology decision — useful as a reference point, not as a code path.
- **Cross-file `ld a.o b.o` link**: m5-004 emits undefined-symbol entries correctly; the
  end-to-end multi-object link of real paideia-as outputs (beyond the smoke fixture)
  hasn't been exercised. Phase 6 task.

None of these block paideia-os Phase-1.

## 4. What we got right

- **Cross-repo escalation discipline**: the moment paideia-os Phase-1 surfaced the
  `lower_add_one` placeholder, work was filed against paideia-as. paideia-os stayed
  clean; paideia-as is the source-of-truth fix. No band-aid in the downstream repo.
- **The m3-004 + m5-005 + m6-005 fix chain**: m3-004 originally landed broken
  (single-instruction-per-block — every multi-instruction `unsafe` body lost all but the
  first instruction). m5-005 retired `lower_add_one` from `cmd_build` but left gaps
  unresolved. m6-005 was the truth-detector: the byte-identical regression test surfaced
  **4 distinct bugs** across `lower.rs` (`ItemData::Let` child-transfer), `emit_walker.rs`
  (Placeholder callee check + missing `ret`), `cmd_build.rs` (symbol-naming pass +
  literal extraction), and `encode_instruction.rs` (LEA SIB disp=0 encoding). One
  regression test → four real bugs surfaced and fixed.
- **Continuous tempo**: m1..m7 ran in sequence with no per-milestone review pause —
  faster than Phase 4's pause-after-milestone cadence. Justified because each milestone's
  output was the next milestone's input (lowering → encoding → unsafe → data → symbols →
  smoke → docs).
- **Side-table compositionality held**: `DataSideTable`, `SymbolTable`,
  `EncodeOutput.reloc_sites` all follow the m3-007 / Phase-3 `InstructionSideTable`
  convention. Zero design churn carrying Phase 4's pattern into Phase 5.
- **Boot intrinsics ship as the focused subset of x86_64**: 20 new mnemonics, every one
  motivated by paideia-os Phase-1 boot code. No general-purpose ISA growth for its own
  sake; encoders only cover what the kernel needs.
- **The byte-identical regression test as closure marker**:
  `cargo test --test build_emit_smoke add_one_byte_identical` is the single command that
  certifies the build-emit chain produces deterministic, expected machine code. Every
  bug in the m6-005 chain was discovered through this test.

## 5. Phase-5 → Phase-6 carryover

The original Phase-5 self-hosting plan ships in Phase 6+ unchanged. Restated here so
Phase 6's opening conditions are explicit.

### Phase 6 substrate (stdlib expansions per `self-hosting-phase5-plan.md` §3):

1. **SmallVec<T, N>** in paideia-stdlib.
2. **Unicode XID** character tables in paideia-stdlib.
3. **serde-equivalent + serde_json + toml** in paideia-stdlib (or explicit SARIF/TOML drop).
4. **BLAKE3** hash module in paideia-stdlib.
5. **Lru** cache type in paideia-stdlib.

### Phase 6 substrate (Tier 1-3 self-host):

- **Tier 1**: paideia-as-lexer + paideia-as-diagnostics + paideia-as-ast + paideia-as-parser.
- **Tier 2**: paideia-as-types + paideia-as-effects + paideia-as-ir + paideia-as-elaborator
  + paideia-as-encoder + paideia-as-linker + paideia-as-dwarf.
- **Tier 3**: paideia-as-emitter-elf + paideia-as-emitter-pax + paideia-as-emitter-pe.

### Phase 6 surface activation (build-emit gaps from Phase 5):

- Walker-chain activation for records / generics / traits / borrowed-refs / stdlib types
  through `cmd_build`.
- All m1-003 lambda body shapes (curried 2-arg, deeply nested, capture-by-reference).
- General RIP-relative addressing (`mov rax, [rip + symbol]` direct encoding).
- `record` vs `struct` keyword pick + migration (carried from Phase-4 §4 unchanged).

### Phase 6+ deferrals (locked):

- paideia-lsp self-hosting (async runtime + tower-lsp port).
- paideia-pq-sign self-hosting (FFI shim vs full crypto port).
- Full NIST ACVP test vectors (gates on upstream `ml-dsa` crate).
- Stage-0b GAS AT&T-syntax variants.

## 6. Closing note

Phase 5 hit its narrow target: paideia-os Phase-1 is unblocked. The byte-identical
regression `cargo test --test build_emit_smoke add_one_byte_identical` is the closure
marker. v0.5.0 tag lands at m7-003.

After m7 closes, paideia-os development resumes from issue #1 (boot: GDT + LGDT helper).
The original self-hosting Phase 5 plan re-opens as Phase 6, against the conditions in
`self-hosting-phase5-plan.md` §8.
