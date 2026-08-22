# Corpus S/F/C sweep (deferred artefact for #1245)

**Tool:** `tools/corpus-sfc-sweep.py`
**Issue:** [#1245 — M3 follow-up: run corpus S/F/C sweep once lin_class/effect payload wiring lands](https://github.com/paideia-os/paideia-as/issues/1245)
**Depends on:** #1237 (root walker fix — closed)

## 1. Why this tool exists

The root-walker fix landed in #1237 (`cmd_build` seeds `LinearityWalker`,
`EffectRowWalker`, and `CapWalker` from `find_outermost_root(&ir)`
rather than `IrNodeId::new(1)`). Walkers now traverse the real IR
subtree, but the S/F/C diagnostics they emit remain **dormant** for
three separate reasons:

1. **LinearityWalker** (S0900, S0901, S0902, S0904, S0907) reads
   `IrNode.lin_class`. Phase-1 lowering (`crates/paideia-as-elaborator/
   src/lower.rs`) unconditionally sets `lin_class = LinClass::Unrestricted`.
   Only test fixtures manually override it. Diagnostics fire once
   syntax-driven `lin_class` propagation lands (m5).
2. **EffectRowWalker** (F1100, F1101, F1105, F1106) reads the
   `perform_ops` and `handle_effects` injection tables
   (`crates/paideia-as-elaborator/src/effect_walker.rs`). `cmd_build`
   does not currently populate either table; only walker unit tests do.
   Diagnostics fire once m3 effect/handler wiring populates them from
   the CLI path.
3. **CapWalker** (C1300) reads the `lambda_declared` injection table
   (`crates/paideia-as-elaborator/src/cap_walker.rs`). Same story:
   populated by tests only. C1301 (capability-mismatch) is partially
   surfaced today because it inspects `App` nodes' `cap_set` field.

Design decision from
`design/paideia-as/non-milestone-issue-1237-cmd-build-root-walker.md`
§4.2: do not sweep the corpus on the walker fix alone; wait until
payloads land, then sweep once, then author expect-failure fixtures.

## 2. What the tool does

`tools/corpus-sfc-sweep.py`:

- Walks every `.pdx` under configured roots (default: `examples/` and
  `tests/`), recursively.
- For each fixture, invokes
  `paideia-as build --emit placeholder --sarif <tmp>`.
  Placeholder emit runs the full walker pipeline but skips ELF/PAX/PE
  encoding, so unrelated emit-stage errors do not mask walker output.
- Parses the SARIF file and aggregates every diagnostic with a ruleId
  matching `[SFC]\d{4}` into per-code totals and per-file breakdowns.
- Reports the **featured codes** enumerated in #1245's acceptance
  criteria (S0900, S0901, S0902, S0904, S0907, F1100, F1101, F1105,
  F1106, C1300) prominently; other S/F/C codes appear under
  "Other S/F/C codes observed".
- Optional `--json <path>` produces a machine-readable summary suitable
  for CI-side regression assertions.

## 3. When to re-run

Re-run this sweep whenever:

- Phase-1 lowering starts propagating `lin_class` from surface syntax
  (m5). Grep guardrail: `LinClass::Unrestricted` occurrences drop in
  `crates/paideia-as-elaborator/src/lower/`.
- `cmd_build.rs` starts calling `EffectRowWalker::inject_perform_op` /
  `inject_handle_effect` or `CapWalker::inject_lambda_declared` from
  the CLI path (today only walker unit tests call these).
- Any new S/F/C diagnostic is added to the catalog — the sweep surfaces
  whether it fires against the current corpus.

## 4. Baseline recorded at close of #1245

Sweep run **2026-08-22**, `target/release/paideia-as`, 895 fixtures
across `examples/` and `tests/`:

| Code  | Firings | Notes                                              |
|-------|---------|----------------------------------------------------|
| S0900 | 0       | dormant — `lin_class` is Unrestricted everywhere   |
| S0901 | 0       | dormant                                            |
| S0902 | 0       | dormant                                            |
| S0904 | 0       | dormant                                            |
| S0907 | 0       | dormant                                            |
| F1100 | 0       | dormant — `perform_ops` unpopulated in CLI         |
| F1101 | 0       | dormant                                            |
| F1105 | 0       | dormant                                            |
| F1106 | 0       | dormant                                            |
| C1300 | 0       | dormant — `lambda_declared` unpopulated in CLI     |
| C1301 | 4       | partially live (inspects `App.cap_set`) — see §5   |

## 5. C1301 candidates for expect-failure fixture authoring

The sweep already surfaces C1301 (capability-mismatch on `App`) at four
sites — these are candidates for the acceptance-criteria step 2
(expect-failure fixture per newly-eligible diagnostic), even without
further payload wiring:

- `tests/build-emit/boot_observable.pdx`
- `tests/build-emit/control_flow/jz_backward_local.pdx`
- `tests/build-emit/pa10_006l_inout.pdx`
- `tests/effects-corpus/corpus/reject/r_mmio_effect_without_cap.pdx`

Authoring these fixtures is out of scope for #1245 (which is scoped to
the deferred sweep tool + baseline). Track under a follow-up issue once
the full featured-code set becomes live.

## 6. CI integration (future)

Once payloads land and the featured codes go non-zero, wire the sweep
into `tools/paideia-as-pre-push.sh` (or a lightweight CI job) with a
JSON snapshot comparison so per-fixture counts do not silently drift.
The sweep is designed to be idempotent and parallel-safe
(`--jobs N`), so a full 895-fixture pass takes ~2s on 8 workers.
