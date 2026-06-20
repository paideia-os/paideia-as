# Optimization Pass Catalog — Phase 2 Outcome

**Status:** Phase 2 m9 deliverable closure.
**Scope:** Documents the opt-in optimization pass catalog shipped across m9-001..011 (PRs #446–#457). Sister doc to `pq-trust-root.md` for the m7 closure.

## 0. Discipline

Per OS-requirements §6, optimization is **opt-in**. The user annotates functions, loops, or call sites with `#[peephole]`, `#[schedule]`, `#[unroll(n)]`, etc.; the dispatcher walks the canonical catalog in fixed order and invokes only the requested passes. Unannotated code receives no rewrites — the assembler emits the source instruction stream verbatim.

This discipline trades global cleverness for predictability. PaideiaOS code wants to be auditable: every emitted byte should be traceable to a specific source-level construct, modified by a specific annotated optimization.

## 1. The canonical catalog

Catalog order (m9-001 establishes the dispatch sequence; m9-002..009 add the entries):

```
noop                  (m9-001, smoke test)
peephole              (m9-002, O1500)
schedule              (m9-003, O1503)
macro-fusion          (m9-004, O1504)     [emitter]
dse                   (m9-005, O1505)
encode-tight          (m9-006, O1506)     [emitter]
branch-hint           (m9-007, O1507)
align                 (m9-007, O1508)     [emitter]
pool-constants        (m9-007, O1509)
tailcall              (m9-008, O1510)
unroll                (m9-009, O1511/O1512)
```

Two passes (`macro-fusion`, `encode-tight`, `align`) live in `paideia-as-emitter-elf::opt` because they're code-layout concerns. The rest live in `paideia-as-ir::opt`. Both crates use the same `OptPass` trait + `OptDiagSink` shapes, so the catalog dispatcher composes uniformly.

## 2. Phase-3-m3 closure

Phase 3 m2 added the per-node `Instruction` payload + `InstructionSideTable` (m2-001..006) — the IR finally carries per-node x86_64 mnemonics. Phase 3 m3 (this section) flips every pass from "would-fire" markers to real rewrites that read and mutate that side-table.

Per-pass status after m3 closure (PRs #553–#560):

- **peephole** (m3-001 / PR #553) — REAL: 5/8 rewrites fully working; 3 stubbed pending Mnemonic enum expansion (StrengthReduceMul/Div, CombinePushPop need Mul/Div/Shl/Shr/Push/Pop in the m2-001 catalog).
- **schedule** (m3-002 / PR #554) — REAL diagnostic emission (O1503 per non-trivial reorder). The arena-mutation (rearranging children of the parent block) is a TODO until the IR gains explicit basic-block structure.
- **dse** (m3-003 / PR #555) — REAL: reads InstructionSideTable, extracts MemSib operands, delegates to the phase-2 `_impl`, removes dead stores from the table, emits O1505 per removal.
- **encode-tight** (m3-004 / PR #556) — REAL: encoder consults `can_shorten_add_to_32bit` + `can_use_rel8` at emit time; `EncodeStats` tracks the tightening count for caller-side O1506 emission.
- **tailcall** (m3-005 / PR #557) — REAL structural rewrite (Call followed by Ret → Jmp; emits O1510). Recursion detection (call target == enclosing function symbol) is a TODO pending the elaborator surfacing call-target symbols.
- **unroll** (m3-006 / PR #558) — STUB with diagnostic emission (`would-fire` per loop-entry candidate, O1511). The body-duplication + remainder-loop generation is a TODO pending loop-entry markers from the elaborator. Retires the `phase-transition-2.md` §5 D-row "Remainder-loop generation for `#[unroll(n)]`".
- **macro-fusion** (m3-007 / PR #559) — STUB: emits O1504 per (Cmp, Jcc) detection site. Real EncodingHint flagging is m4 encoder integration.
- **branch-hint** (m3-007 / PR #559) — STUB: emits O1507 per Jcc. Real prefix emission (0x2E / 0x3E) is m4 encoder integration.
- **align** (m3-007 / PR #559) — STUB: emits O1508 per alignment site. Real `.align 16` directive insertion is the m4 emit stage.
- **pool-constants** (m3-007 / PR #559) — STUB: emits O1509 per repeated immediate. Real constant-pool section + RIP-relative loads land with m4 paideia-link integration.

m3-008 (PR #560) shipped `tests/opt-regression/` as a workspace member with 10 per-pass regression test files (19 active + 1 ignored for encode-tight's diagnostic-side wiring) that pin each pass's diagnostic shape. A real-rewrite landing in a future PR (e.g., loop-entry markers activating unroll's body duplication) breaks the regression test and forces an honest update.

## 2.1 Phase 4 m1 — would-fire flip closure

Phase 4 m1-007..010 closed the 4 would-fire passes from Phase 3 m3-007:

| Pass            | Activated at        | Diagnostic | What flipped                                      |
|-----------------|---------------------|------------|---------------------------------------------------|
| macro-fusion    | m1-007              | O1504      | Detects adjacent (Cmp, Jcc) pairs; sets fusion EncodingHint flag on the Cmp. |
| branch-hint     | m1-008              | O1507      | Sets branch-hint prefix flag (0x3E taken / 0x2E not-taken) on Jcc. |
| align           | m1-009              | O1508      | Reads m8-006 LoopMetaTable; marks loop-entry for alignment. |
| pool-constants  | m1-010              | O1509      | Detects repeated Imm64 (≥2 occurrences); interns into new `ConstantPoolTable`. |

After m1, **9/10 m3 passes ship real rewrites**. Only `unroll` (O1511) remains as would-fire — body duplication is m3-006 closure follow-up gated on full Loop-node lowering inside the IR arena.

## 2.2 Phase 4 m2 — multi-emit parity

Phase 4 m2 closes the chain that started at Phase 2 m9-002's "Phase-2-m9 honesty" disclaimer. The disclaimer chain officially closes here.

m2-001..004 wire the post-rewrite `InstructionSideTable` through every emitter:

- **ELF64** (m2-001 ancestor; Phase 3 already wired): emits post-rewrite `.text` from the side-table.
- **PE/COFF** (m2-001): `emit_text_from_instructions()` reads from `InstructionSideTable`; PE/COFF output matches ELF64 modulo header + ABI bridge. m3 rewrites visible in PE/COFF.
- **DWARF** (m2-002): `.debug_line` rows match post-rewrite instruction offsets via the offset_map returned by `emit_text_from_instructions`. `.debug_info` references post-rewrite `.text` ranges.
- **PAX** (m2-003): new `.paideia.opt-passes` section records per-function rewrite counts emitted by each pass; `.paideia.caps` / `.paideia.effects` reference post-rewrite ranges; `pax-introspect` displays per-pass counts.

m2-004 wires a per-emit DDC fixture: `tools/ddc/fixtures/m2-004-passes.pdx` runs through ELF64 / PE/COFF / PAX twice each via `tools/ddc/run.sh`; `cmp -s` confirms byte-identical post-rewrite output per format. Combined with the m10 `SOURCE_DATE_EPOCH` determinism gate from Phase 2, this is a hard contract that opt-pass output is reproducible.

The "Phase-2-m9 honesty" disclaimer ("Each pass ships as a scaffolded OptPass whose `apply` method emits a 'would-fire' diagnostic marker. None of them actually mutate the IR today...") is now historically accurate **only for unroll**. Every other pass in the catalog ships real rewrites that:

1. Read from `InstructionSideTable` (m2-001 schema, m1-001..004 walker population).
2. Mutate the table or emit EncodingHint flags (m1-007..010 for the m3-007 passes).
3. Surface in ELF64 + PE/COFF + DWARF + PAX outputs identically (m2-001..003).
4. Verify byte-identical across runs (m2-004 DDC fixture).

The disclaimer chain closes here.



The **already-callable phase-2 helpers** are preserved as `*_impl` internal functions so their existing test coverage stays green:

- `schedule_block_impl(ops)` — latency-aware scheduler over (idx, InstructionClass).
- `dse_block_impl(ops)` — reverse-walk dead-store elimination over MemOp list.
- `pad_for_alignment(offset, n)`, `pad_for_fusion(cmp_offset, cmp_len)`,
  `lay_unlikely_off_fall_through(hint)`, `pool_candidates(counts)` — unchanged.
- `tco_blocker_impl(...)` — 4-bool blocker predicate.
- `is_unroll_safe_impl(trip, factor)` — trip-count-aware safety guard.
- `can_shorten_add_to_32bit(high_bits_used)`, `can_use_rel8(displacement)` — encoding shorteners; consulted by m3-004's encode-time wiring.

The phase-3-m3-closure pattern: the public API speaks the m2-001 vocabulary today; the analysis bodies port incrementally; the `*_impl` variants are reachable for tests and don't need to be renamed away.

## 3. Annotation grammar

`#[opt1, opt2, opt3]` — list of pass names; argument lists like `#[unroll(8)]` are stripped of args at the dispatch level (each pass parses its own args at invocation). Whitespace-tolerant. Empty annotation = no passes requested.

## 4. Composition guarantees (m9-010)

`dispatch_collecting_order(arena, root, &requested)` returns the invocation sequence for a verification consumer. The catalog-order property is pinned by:

- 5 unit tests covering BTreeSet alpha-vs-catalog-order divergence, empty-request handling, unknown-pass-name handling, and subset ordering.
- 1 proptest over random pass-index sequences (0..10 elements drawn from the catalog) that verifies invoked sequences are strictly increasing in catalog position regardless of input shape.

## 5. Diagnostic codes

| Code  | Pass               | Severity | Status |
|-------|--------------------|----------|--------|
| O1500 | peephole           | note     | live   |
| O1501 | reserved           | note     | reserved |
| O1502 | reserved           | note     | reserved |
| O1503 | schedule           | note     | live   |
| O1504 | macro-fusion       | note     | live   |
| O1505 | dse                | note     | live   |
| O1506 | encode-tight       | note     | live   |
| O1507 | branch-hint        | note     | live   |
| O1508 | align              | note     | live   |
| O1509 | pool-constants     | note     | live   |
| O1510 | tailcall           | note     | live   |
| O1511 | unroll (applied)   | note     | live   |
| O1512 | unroll (warning)   | warning  | live   |

m9-011 ships a regression test (`paideia-as-diagnostics::tests/opt_codes_present.rs`) that pins the contiguous O1500..O1512 set + the ≥10-codes-total invariant.

## 6. Test corpora

Each pass has its own `tests/opt-regression/<pass>/` workspace member with:
- A `Cargo.toml` declaring the test crate.
- A `corpus/` directory of `.pdx` fixtures (typically 3–4, one per scenario the pass should handle).
- A `tests/runner.rs` that pins the fixture count + per-fixture assertions about the expected rewrite.

11 corpora total (one per opt pass), each compatible with the m1-013 cross-build harness pattern for future activation.

## 7. AS / OS-requirements resolution

- **OS-requirements §6 design-clarification 5 (TCO)** — resolved by m9-008: TCO ships in Phase 2 to support the kernel's exception-unwinder path. CapabilityBoundary / EffectHandlerInstalling / DifferentCallConvention / FrameRequiresEpilogue all suppress.

## 8. Phase-2-m9 deferrals

- **Actual rewrites** — the OptPass::apply implementations stay scaffolded until the IR exposes per-node instruction payloads. The helpers are already correct.
- **Per-rewrite peephole codes** — O1501 / O1502 are reserved for future fine-grained peephole diagnostics (one code per Rewrite variant). Today O1500 covers the whole pass.
- **Remainder loops for unroll** — `is_unroll_safe` returns false on indivisible trip counts; emitting a remainder loop is a follow-up.
- **Profile-guided pass ordering** — the catalog is canonical. Profile-guided ordering would invert that; out of scope for Phase 2.

## 9. References

- `tests/opt-regression/<pass>/` — per-pass corpora.
- `crates/paideia-as-ir/src/opt/` — pass implementations + dispatcher + composition tests.
- `crates/paideia-as-emitter-elf/src/opt/` — emitter-side passes (macro-fusion, encode-tight, align).
- `crates/paideia-as-diagnostics/catalog.toml` — O-code registry.
- PRs #446–#457 — the m9 deliverable.
- Upstream `optimization-passes.md` — the canonical specification this appendix mirrors.
