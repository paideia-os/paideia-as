# paideia-as#1581 — FMT-17: final acceptance (round-trip + comments + parser-equivalence)

**Scope:** capstone acceptance ticket for the paideia-fmt completion
chain. Declares the formatter COMPLETE as of **paideia-as v0.36.82**
against the three corpus-scale style-spec invariants (idempotence,
parser-equivalence, comment preservation) and enumerates the five
follow-up polish items that are NOT acceptance blockers.

No product-code changes land in this wave: FMT-13 / FMT-14 / FMT-15
already shipped the three corpus harnesses, FMT-16 already wired the
LSP, and the FMT-3 byte-identical fallback keeps every sweep green on
every parseable file. This ticket locks the state in via (a) a new
acceptance design document, (b) status stamps on the style spec and
crate module docs, and (c) five follow-up tickets filed as text for
`gh issue create`.

## Added

- `design/toolchain/paideia-fmt-acceptance.md` (**new**, ~310 lines)
  - §1 Scope — what "complete" means for the three acceptance criteria.
  - §2 Per-harness status — FMT-13 / FMT-14 / FMT-15 contracts, LOC,
    sanity-test count, opt-in flag, corpus-root resolution rules,
    known gaps per harness, "safe in CI today" verdict.
  - §3 Expected invocation — the canonical
    `cargo test -p paideia-fmt --release -- --include-ignored --nocapture`
    command, sample green output, and the `PAIDEIA_FMT_CORPUS` bisect
    helper.
  - §4 Follow-ups F1..F5 — each formatted as a `gh issue create`-ready
    text block with Title / Body / Acceptance / Blockers.
    * **F1**: Lambda param-type side-table re-emission (FMT-5b polish).
    * **F2**: FMT-14 `ast_eq` side-table comparison extension.
    * **F3**: FMT-16 rangeFormatting line-align → AST-span (FIXME tag
      in `paideia-lsp/src/server.rs::widen_to_full_lines`).
    * **F4**: `@repr(...)` attribute AST + emitter (FMT-8 deferred).
    * **F5**: Comment-reflow opt-in `--reflow-comments` (style-spec
      §16 deferred).
  - §5 Declaration of completion — references the style spec stamp,
    the crate module docs stamp, and the v0.36.82 version bump.

## Modified

- `design/toolchain/paideia-fmt-style-spec.md`
  - **Status line** rewritten from `v1 — settles every style decision …`
    to **`COMPLETE as of paideia-as v0.36.82` (FMT-17 capstone)** with
    an explicit cross-link to `paideia-fmt-acceptance.md` and a note
    pointing readers to F1..F5 for polish items.

- `crates/paideia-fmt/src/lib.rs`
  - **Module header** gains a leading `Status: COMPLETE` banner that
    references the acceptance report and names the three corpus-sweep
    harnesses.
  - **Pipeline table** retitled `FMT-1..FMT-17`, every row stamped
    `landed`, and FMT-11 / FMT-12 / FMT-15 / FMT-16 rows added
    (previous table stopped at FMT-14 and still carried
    `this ticket` / `after this ticket` placeholders from FMT-3's
    original landing).

## Harness-safety audit

Confirmed by direct review of the three harness files that each one:

- Walks only `.pdx` under the resolved roots (never `/`).
- Resolves the workspace root via a bounded 6-step climb from
  `CARGO_MANIFEST_DIR` with a `Cargo.toml` + `crates/` signature guard.
- Uses the `_with(Option<String>)` helper for env-var-aware
  resolution so no sanity test touches `std::env::set_var` (avoids the
  known Rust test-harness race).
- Classifies every outcome into a `*Report` enum variant and only
  panics in the opt-in `#[ignore]`'d test (sanity tests never panic
  on corpus content).
- Caps diff/dump output at 10 files × 20-30 lines so a batch
  regression does not flood CI logs.

All three sweeps are **safe to run in CI today**. No concurrency
hazards, no `set_var` races, no unbounded output, no hidden I/O side
effects.

## Follow-up count

**5** (F1..F5). None block FMT-17 completion. Each has a verbatim
`gh issue create`-ready text block in acceptance §4.

## Not run

Per sub-agent policy, no cargo / build / smoke invocation. Main is
expected to:

1. Run `bash tools/build.sh` to confirm the lib.rs doc-comment update
   compiles clean (the only code-shaped change is comment text).
2. Optionally run `cargo test -p paideia-fmt --release --
   --include-ignored --nocapture` to produce the acceptance evidence
   log for the v0.36.82 release notes.
3. File F1..F5 as separate issues using the text blocks in
   `paideia-fmt-acceptance.md` §4.
4. Bump `Cargo.toml` workspace.version 0.36.81 → 0.36.82 and add the
   CHANGELOG entry.
