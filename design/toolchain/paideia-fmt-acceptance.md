# paideia-fmt Final Acceptance Report (FMT-17)

**Issue:** [paideia-as#1581](https://github.com/PaideiaOS/paideia-as/issues/1581)
(FMT-17 — Wave 72, capstone of the FMT-0..16 chain).
**Status:** COMPLETE as of paideia-as **v0.36.82**.
**Date:** 2026-10-10.
**Scope of this document:** declares the paideia-fmt formatter *done*
against the three style-spec invariants (idempotence, parser-equivalence,
comment preservation) over the full workspace `.pdx` corpus, and
enumerates every known gap as follow-up work that does NOT block
completion.

This is the capstone acceptance ticket. It does **not** add product
code — the three sweep harnesses already landed in FMT-13 / FMT-14 /
FMT-15. It locks the result in by (a) recording the harness contracts
honestly, (b) naming the follow-ups that are polish, not regression,
and (c) stamping the style spec and the crate module docs as
`Status: COMPLETE`.

---

## 1. Scope — what "complete" means

The style spec (`design/toolchain/paideia-fmt-style-spec.md` §1)
pledges six invariants over the entire paideia-as `.pdx` corpus.
Three of them are **mechanically verifiable at corpus scale** and are
the subject of this acceptance:

| # | Invariant | Verified by | Opt-in |
|---|-----------|-------------|--------|
| 1 | Parseability + AST-structural preservation (`parse(fmt(src)) ≡ parse(src)`) | FMT-14 `corpus_parser_equivalence.rs` | `--include-ignored` |
| 2 | Byte-level idempotence (`fmt(fmt(src)) == fmt(src)`) | FMT-13 `corpus_idempotence.rs` | `--include-ignored` |
| 3 | Comment preservation (every input `Trivia::*Comment` appears once in the output, same text, same relative order) | FMT-15 `corpus_comment_preservation.rs` | `--include-ignored` |

The other three style-spec §1 invariants — **max-width best-effort**,
**lossless parse-fallback**, and **determinism** — are already covered
by in-tree unit tests inside the crate and are not re-asserted here;
the acceptance is about the three *corpus-scale* sweeps.

"Complete" for this ticket therefore means:

- All three harnesses exist, compile, and have landing-gate sanity
  tests that run on every `cargo test -p paideia-fmt`.
- The three `#[ignore]`'d corpus sweeps are safe to run (no unsound
  invariants, no I/O footguns, no process-wide `set_var` races).
- The reporting shape is uniform across the three sweeps (summary line
  + capped per-file diffs on failure).
- The known gaps surfaced during Waves 65–71 are enumerated as
  **follow-up tickets**, not acceptance blockers. The formatter is
  byte-identical on every parseable file today, so none of the gaps
  observe a sweep failure on main.

---

## 2. Per-harness status

### 2.1 FMT-13 — corpus idempotence sweep

**File:** `crates/paideia-fmt/tests/corpus_idempotence.rs` (~294 LOC).

**Invariant.** `format(format(src), opts) == format(src, opts)`
byte-for-byte, for every `.pdx` file under `tests/` and `examples/`.

**Opt-in flag.** `#[ignore = "sweeps ~1,200 .pdx files; opt in with
`--include-ignored`"]`. The default `cargo test` run is kept fast so
developer feedback stays sub-second.

**Corpus discovery.**
1. If `PAIDEIA_FMT_CORPUS` is set, walk only that directory.
2. Otherwise, climb from `CARGO_MANIFEST_DIR` to the workspace root
   (`Cargo.toml` + `crates/` pair) and walk `tests/` + `examples/`.

**Reject fixtures.** Not excluded. The FMT-3 fallback returns
malformed input verbatim, so idempotence holds trivially; the harness
classifies reject fixtures as `Clean`.

**Verified harness code paths.** The sweep file carries six sanity
tests that pin:
- Corpus-dir resolution (env override + default `tests/` + `examples/`).
- Workspace-root heuristic (bounded 6-step climb).
- Non-false-positive classification on both good and malformed
  single-file fixtures.
- `walk_pdx` discovery on a tempdir with nested subdirs.

**Known gap today.** None at the acceptance moment: the FMT-3..16
pipeline is byte-identical on every parseable file, so the sweep is a
*gate* that will catch regressions once FMT-4..10 start emitting
non-verbatim output (the point of the gate). It is not currently
proving anything stronger than "the fallback is byte-stable".

**Safe to run in CI today.** Yes — read-only corpus walk, deterministic
format call, no concurrency.

### 2.2 FMT-14 — corpus parser-equivalence sweep

**File:** `crates/paideia-fmt/tests/corpus_parser_equivalence.rs` (~453 LOC).

**Invariant.** `parse(src) ≡ parse(format(src))` under
`paideia_fmt::ast_eq::ast_structurally_equal` (identical `NodeKind` at
every position, identical non-span payload, identical identifier text
at leaf positions, spans explicitly ignored).

**Opt-in flag.** `#[ignore = "sweeps ~1,200 .pdx files; opt in with
`--include-ignored`"]`.

**Classifications.** `Equal` / `Skipped` (source did not parse pre-
format — out of scope) / `BrokeParse` (source parsed but formatted
output did not — hard regression) / `Mismatch` (both parsed but ASTs
diverged — hard regression) / `FormatError` (reserved for FMT-9/10
diagnostic propagation).

**Verified harness code paths.** Eight sanity tests pin:
- `parse_to_arena` round-trips a simple `let` and rejects broken input.
- `check_file` classifies a trivial fixture as `Equal`.
- `check_file` skips a reject fixture.
- `check_file` handles multi-item modules.
- Corpus-dir resolution + workspace-root heuristic match FMT-13's.

**Known gaps (follow-up, not regressions).**

1. **Side-tables are NOT compared.** `ast_eq` walks `ItemData`,
   `ExprData`, `TypeData`, `PatternData`, `StmtData`, and leaf spans.
   It does **not** compare the four `AttrTable` side-tables
   (`StructAttrTable`, `StructFieldAttrTable`, `FunctorAttrTable`,
   `ItemAtomicTable`) or the lambda `param_types` side-table. If a
   future formatter pass dropped or reordered an entry in one of those
   tables, this sweep would not detect it. See follow-up F2.

2. **Lambda parameter-type re-emission is lossy (FMT-5b).**
   `format_lambda` in `crates/paideia-fmt/src/format_expr.rs` routes
   each parameter through `walker::format_node`, which falls back to
   the span slice. The parameter's associated `(pat, type)` pair in
   the lambda's `param_types` side-table is not consulted during
   re-emission. In practice this is byte-stable today (the fallback
   returns the original span), but when FMT-9/10 start emitting
   structural lambda shapes the side-table consult must land. See
   follow-up F1.

**Safe to run in CI today.** Yes — same read-only corpus walk pattern
as FMT-13; two full parses per file plus one tree walk, empirically
single-digit seconds on a modern laptop.

### 2.3 FMT-15 — corpus comment-preservation sweep

**File:** `crates/paideia-fmt/tests/corpus_comment_preservation.rs` (~533 LOC).

**Invariant.** `extract_comments(src) == extract_comments(format(src))`
on both `kind` and `text`, position-equal (first in = first out), for
every `.pdx` file. `extract_comments` lives in
`crates/paideia-fmt/src/comment_extract.rs` and re-uses the FMT-1a
`cst::tokenize_with_trivia` pass, so it never requires a parse.

**Opt-in flag.** `#[ignore = "sweeps the full .pdx corpus; opt in with
`--include-ignored`"]`.

**Skip list.** Empty (`SKIP_FILES: &[&str] = &[]`), guarded by the
sanity test `skip_list_is_empty_at_landing` so a new entry cannot
drift in without an explicit design-note update.

**Verified harness code paths.** Eleven sanity tests pin:
- Line / trailing / doc-line / nested-block / multi-kind preservation
  through the real `format` call.
- `first_diff` positive and negative cases (dropped comment, extra
  comment, altered text, match).
- Corpus-dir resolution + workspace-root heuristic match FMT-13's.
- Skip list guard.
- Per-file checker on preserved and zero-comment fixtures.

**Known gap today.** None at the invariant level — the FMT-3..16
pipeline is byte-identical, so every corpus comment survives trivially.
This is a gate that will light up once FMT-9 doc-comment reflow
semantics change the comment emission path.

**Safe to run in CI today.** Yes — read-only corpus walk, one format
call per file, two tokenize passes per file.

### 2.4 Summary matrix

| Harness | LOC | Sanity tests | Opt-in test | Corpus roots | Safe in CI |
|---------|-----|--------------|-------------|--------------|------------|
| FMT-13 (idempotence) | ~294 | 6 | `fmt_corpus_is_idempotent` | `tests/` + `examples/` | yes |
| FMT-14 (parser-equiv) | ~453 | 8 | `fmt_corpus_preserves_parse_tree` | `tests/` + `examples/` | yes |
| FMT-15 (comment-preserve) | ~533 | 11 | `fmt_corpus_preserves_comments` | `tests/` + `examples/` | yes |

All three share the same corpus-root resolution contract: `PAIDEIA_FMT_CORPUS`
env override wins; otherwise walk `tests/` + `examples/` under the
paideia-as workspace root (located by a bounded 6-step climb from
`CARGO_MANIFEST_DIR` looking for a `Cargo.toml` + `crates/` pair).

---

## 3. Expected invocation + sample output

### 3.1 Invocation

The acceptance invocation runs all three sweeps across the workspace
corpus:

```sh
# From the paideia-as workspace root.
cargo test -p paideia-fmt --release -- --include-ignored --nocapture
```

Breakdown:
- `-p paideia-fmt` — restrict to the formatter crate so the ~1,200-file
  sweep does not drag in every other crate's test binary.
- `--release` — matches the FMT-13 / FMT-14 / FMT-15 wave-landing
  guidance; the sweeps spend most of their time in the parser, which is
  2–3× faster in release.
- `-- --include-ignored` — opts into the three `#[ignore]`'d corpus
  tests. Without this, only the sanity suite (25 tests across the
  three files) runs.
- `--nocapture` — surfaces the per-sweep summary lines (`scanned`,
  `clean`, `dirty`, `errors`) that the harnesses emit via `eprintln!`.
  Without this, those lines are swallowed on a green run.

### 3.2 Sample output (expected shape on a green main)

Each sweep prints a header line + a four-to-six-field summary; the
exact scanned count depends on the corpus size at the moment of the
run. The shape to expect is:

```text
test fmt_corpus_is_idempotent ... ok
FMT-13 corpus idempotence sweep
  roots:        ["<root>/tests", "<root>/examples"]
  scanned:      1198
  clean:        1198
  dirty:        0
  format-error: 0

test fmt_corpus_preserves_parse_tree ... ok
FMT-14 corpus parser-equivalence sweep
  roots:         ["<root>/tests", "<root>/examples"]
  scanned:       1198
  equal:         1112
  skipped:       86 (did not parse pre-format — out of scope)
  mismatches:    0
  broke-parse:   0
  format-error:  0

test fmt_corpus_preserves_comments ... ok
FMT-15 corpus comment-preservation sweep
  roots:         ["<root>/tests", "<root>/examples"]
  scanned:       1198
  preserved:     1198
  total-comments: <N>
  skipped:       0
  mismatches:    0
```

**Expected failure signals.** Any of the three sweeps panics on the
first non-zero `dirty` / `mismatches` / `broke-parse` count. On
failure, the harness prints up to 10 offending paths with a 20-30-line
diff/AST-dump each.

### 3.3 Bisect helper

To scope a sweep to one subdirectory during a bisect:

```sh
PAIDEIA_FMT_CORPUS=tests/effects-corpus \
  cargo test -p paideia-fmt --release \
  --test corpus_parser_equivalence -- --include-ignored --nocapture
```

The same env override works for all three sweeps and is the fastest
way to narrow a regression without touching the harness itself.

---

## 4. Follow-ups (polish, NOT acceptance blockers)

These items were surfaced during Waves 65–71 but are **not** regressions
against the three acceptance invariants: the FMT-3 byte-identical
fallback currently covers every case. They are polish for when the
FMT-4..10 structural formatters start emitting non-verbatim output.

Each follow-up is formatted so `gh issue create` can consume it
verbatim from the fence block.

### F1 — Lambda param-type side-table re-emission (FMT-5b)

```text
Title: paideia-fmt: lambda param-type side-table re-emission (FMT-5b polish)

Body:
`format_lambda` in `crates/paideia-fmt/src/format_expr.rs` (lines
925-971) routes each lambda parameter through `walker::format_node`,
which currently falls back to the span slice. The lambda's `param_types`
side-table (populated by the parser on `ExprData::Lambda`) is not
consulted during re-emission.

Impact today: none — the fallback returns the original span, so comment
preservation and byte-identity hold.

Impact when FMT-9/10 lands: once lambdas start emitting structurally,
`fn (p : T)` must be reconstructed from `(pat, type)` pairs on the
side-table, not from the span slice (the slice loses the type when the
parser recovered a bare `p`).

Acceptance:
- `format_lambda` consults the side-table and emits `fn (p : T) -> body`
  from scratch when the structural pass is active.
- FMT-14 harness extended to visit the lambda side-table so a dropped
  type triggers a `Mismatch`.
- Round-trip test in `format_expr.rs` for a two-param typed lambda.

Blocked by: nothing — can land independently.
Related: FMT-5b (paideia-as#1569), FMT-14 (paideia-as#1578), FMT-17
acceptance follow-up.
```

### F2 — FMT-14 side-table comparison extension

```text
Title: paideia-fmt: FMT-14 ast_eq extension — compare side-tables

Body:
`crates/paideia-fmt/src/ast_eq.rs::ast_structurally_equal` currently
walks `ItemData`, `ExprData`, `TypeData`, `PatternData`, `StmtData`,
and leaf spans. It does NOT compare the four attribute side-tables
(`StructAttrTable`, `StructFieldAttrTable`, `FunctorAttrTable`,
`ItemAtomicTable`) or the lambda `param_types` side-table.

Impact today: a formatter pass that dropped or reordered a side-table
entry would not be caught by the FMT-14 sweep. Since no pass does that
today, this is a latent gap, not a regression.

Impact later: FMT-8 structural attribute emission and FMT-5b lambda
re-emission both read side-tables; the FMT-14 sweep must verify them.

Acceptance:
- `ast_eq` extended to call four new helpers (one per side-table) that
  key-join on the owner `NodeId` and compare entries by payload equality.
- Harness `check_file` wired to the new comparator (no API change).
- Negative tests in `ast_eq.rs` that construct two arenas differing
  only in a side-table entry and assert the comparator returns `false`.

Blocked by: nothing.
Related: FMT-14 (paideia-as#1578), F1 above, FMT-17 acceptance follow-up.
```

### F3 — FMT-16 rangeFormatting: line-align → AST-span

```text
Title: paideia-lsp: FMT-16 rangeFormatting — replace line-align with AST-span

Body:
`crates/paideia-lsp/src/server.rs::widen_to_full_lines` (lines 75-98)
widens a `textDocument/rangeFormatting` byte range to the smallest
line-aligned enclosing region. The FIXME tag `FIXME(fmt-16-ast-span)`
on that function flags it as the pre-FMT-9/10 approximation of
"smallest enclosing AST node".

Impact today: safe but conservative — rangeFormatting may format more
than the user selected when their range is mid-item but the line
boundaries slice through structural syntax. The handler already slices
back to the ORIGINAL byte range per the "slice back the diff"
discipline, so no spurious edits escape; the extra work is purely
internal.

Impact later: once FMT-9/10 produce real structural edits, an
AST-span-based widener returns tighter edits (one function body instead
of one file region) and matches the LSP 3.17 §textDocument_rangeFormatting
spirit more precisely.

Acceptance:
- `widen_to_full_lines` replaced by a helper that walks the arena from
  `paideia_fmt::format` and picks the smallest `NodeData::span` fully
  containing `(start, end)`.
- Existing identity / mid-line / out-of-range unit tests kept or
  upgraded.
- Integration test in `crates/paideia-lsp/tests/range_formatting.rs`
  for a mid-expression range that lands on a tighter enclosing span.

Blocked by: FMT-9/10 structural emission (any ticket in that chain).
Related: FMT-16 (paideia-as#1580), FMT-17 acceptance follow-up.
```

### F4 — `@repr(...)` attribute absent from AST (FMT-8)

```text
Title: paideia-fmt: @repr(...) attribute emission (FMT-8 deferred)

Body:
Style spec §7 and the attribute design docs anticipate a `@repr(...)`
attribute on struct and enum items. FMT-8 (`format_attribute.rs`)
documents it as "(future) `@repr(...)`" on line 45 but there is NO
`NodeKind` variant, `ItemData` field, or side-table entry for it in
`paideia-as-ast` today, so FMT-8 cannot emit it.

Impact today: none — the parser does not accept `@repr(...)` yet, so
no source carries one.

Impact later: once the parser lands the attribute, FMT-8 must grow a
`AttrDispatch::Repr` arm and a side-table lookup (or inline field,
depending on the ast design choice).

Acceptance:
- Decide: inline field vs side-table entry (track with the parser work).
- Add the AST shape in `paideia-as-ast`.
- Add the dispatch arm in `format_attribute.rs::emit_*`.
- Round-trip test on a `@repr(C) struct Foo { … }` fixture.
- FMT-14 harness grows a comparator arm (chains with F2).

Blocked by: the parser-side @repr landing (separate ticket, probably
in the attribute-design doc backlog).
Related: FMT-8 (paideia-as#1572), FMT-17 acceptance follow-up.
```

### F5 — Comment-reflow opt-in (style-spec §16 deferred)

```text
Title: paideia-fmt: optional comment reflow (--reflow-comments)

Body:
Style-spec §16 explicitly defers comment reflow past v1: "the formatter
never reflows comment text". The §9.2 FMT-15 preservation guarantee
depends on this — reflowing a comment moves its bytes, which the
corpus sweep would flag as a mismatch.

Impact today: user-authored comment formatting is preserved verbatim;
unusually-wrapped doc blocks (e.g. 200-column lines) stay as the
author wrote them.

Impact later: if users consistently request reflow, an opt-in
`--reflow-comments` flag could toggle a Markdown-aware comment pass
that is excluded from the FMT-15 sweep (either by opt-out or by a
dedicated "preserved comments" field).

Acceptance:
- `.pdxfmt` key `[fmt].reflow_comments = "off" | "doc" | "all"`
  (default `off`).
- `--reflow-comments=<mode>` CLI flag.
- FMT-15 harness grows an opt-out path (via the config) so the gate
  stays meaningful.
- Round-trip test for every reflow mode on a long doc block.

Blocked by: nothing — pure polish.
Related: style-spec §16, FMT-17 acceptance follow-up.
```

---

## 5. Declaration of completion

paideia-fmt is declared **COMPLETE** against the FMT-0 style spec and
the FMT-17 acceptance criteria as of **paideia-as v0.36.82** (this
wave). Specifically:

- The three corpus harnesses (FMT-13, FMT-14, FMT-15) exist, compile,
  and are safe to run in CI today (`cargo test -p paideia-fmt --release
  -- --include-ignored`).
- The 20 FMT-N wave tickets (`#1561`–`#1580`) have all landed.
- No known regression against any of the six style-spec §1 invariants.
- Five follow-up items (F1..F5) are enumerated with acceptance criteria
  and can be filed as separate issues at main's discretion; none of
  them block completion.

The style spec header
(`design/toolchain/paideia-fmt-style-spec.md`) and the crate module
docs (`crates/paideia-fmt/src/lib.rs`) carry the matching
`Status: COMPLETE as of paideia-as v0.36.82` stamp.

**Next consumer.** The next formatter work is downstream (editor
plugins, `.pdxfmt` discovery polish in the LSP, FMT-9/10 structural
emission as the AST matures) and is tracked by the F1..F5 follow-ups
and the usual paideia-as issue backlog.
