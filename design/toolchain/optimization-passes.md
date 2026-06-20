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

## 2. Phase-2-m9 honesty

Each pass ships as a **scaffolded** OptPass whose `apply` method emits a "would-fire" diagnostic marker. None of them actually mutate the IR today — the kind-only IR (m1-002) doesn't carry per-node x86_64 mnemonics.

However, every pass also ships **already-callable helper functions** that encapsulate the rewrite logic:

- `schedule_block(ops)` — reorders an explicit MemOp list.
- `dse_block(ops)` — eliminates dead stores.
- `pad_for_alignment(offset, n)` — alignment math.
- `tco_blocker(...)` — TCO eligibility predicate.
- `is_unroll_safe(trip, factor)` — unroll safety predicate.
- `can_shorten_add_to_32bit(high_bits_used)` — encoding shortener.
- `can_use_rel8(displacement)` — rel8 range check.
- `pad_for_fusion(cmp_offset, cmp_len)` — fusion alignment.
- `lay_unlikely_off_fall_through(hint)` — branch-hint layout.
- `pool_candidates(counts)` — constant-pool filter.

These are unit-tested directly. When the IR gains per-node instruction payloads (a future PR), the OptPass::apply implementations become one-line delegates to these helpers, and the "would-fire" diagnostics flip to "did-fire".

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
