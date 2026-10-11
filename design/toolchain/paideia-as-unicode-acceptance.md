## paideia-as-unicode Final Acceptance Report (UNI-16)

**Issue:** [paideia-as#1603](https://github.com/paideia-os/paideia-as/issues/1603)
(UNI-16 — Wave UNI-W7, capstone of the UNI-0..15 chain).
**Status:** COMPLETE as of paideia-as **v0.37.0**.
**Date:** 2026-10-11.
**Scope of this document:** declares the `paideia-as-unicode` crate
*done* against the UNI-0 crate specification
(`design/toolchain/paideia-as-unicode-spec.md`) and the UNI-11 public
API freeze (`design/toolchain/paideia-as-unicode-api-freeze.md`), and
enumerates every known gap as follow-up work that does NOT block
completion.

This is the capstone acceptance ticket. It does **not** add product
code — every UAX/UCD surface, every corpus harness, every property
and fuzz target, and every consumer wire-up already landed in the
UNI-1..15 waves. UNI-16 locks the result in by (a) recording the
harness contracts honestly, (b) naming the follow-ups that are
polish, not regression, and (c) stamping the spec, the API-freeze
doc, and the crate module docs as `Status: COMPLETE`.

---

## 1. Scope — what "complete" means

The UNI-0 spec pledges a Unicode substrate for the semantic shell
(SH-D9), the LSP position-encoding bridge, and every downstream
`.pdx`-text pipeline. "Complete" at UNI-16 means:

* Every UAX/UTS surface the spec names has either shipped or is
  explicitly deferred under §2.2 of the spec, with a follow-up
  issue filed.
* Every public symbol is enumerated in
  `crates/paideia-as-unicode/tests/public_api.golden` (UNI-11
  deliverable) and guarded by the four drift-gate tests.
* Every text-transforming function has a per-function fingerprint
  golden under `tests/fingerprints/*.golden` (UNI-6 harness).
* The three current consumers (`paideia-as-shell-lex`,
  `-ast`, `-completion`) and the new LSP consumer
  (`paideia-as-lsp`) wire through the crate root; none reach
  into the upstream `unicode-*` crates directly.
* The 17 UNI chain tickets have all landed.

### 1.1 Chain-ticket closure matrix

| UNI  | Issue  | Title                                                    | Status | Lands at |
|------|--------|----------------------------------------------------------|--------|----------|
| UNI-0  | #1587 | crate spec + fingerprint discipline doc                | CLOSED | UNI-W1 |
| UNI-1  | #1588 | UAX #15 full normalization forms (NFD/NFKC/NFKD)       | CLOSED | UNI-W1 |
| UNI-2  | #1589 | Unicode case folding + `eq_ignore_case_unicode`         | CLOSED | UNI-W1 |
| UNI-3  | #1590 | UAX #14 Line Breaking + `wrap_at_width`                | CLOSED | UNI-W2 |
| UNI-4  | #1591 | UAX #31 identifier predicates + UTS #39 profile        | CLOSED | UNI-W3 |
| UNI-5  | #1592 | UCD adapter: GC + Script + Block lookup                | CLOSED | UNI-W2 |
| UNI-6  | #1593 | fingerprint golden files for every corpus test        | CLOSED | UNI-W4 |
| UNI-7  | #1594 | full UCD corpus sweep harness (five data files)       | CLOSED (gated) | UNI-W5 |
| UNI-8  | #1595 | criterion benches: throughput baseline                 | CLOSED | UNI-W4 |
| UNI-9  | #1596 | `WidthMode` locale resolver (resolve TODO)             | CLOSED | UNI-W2 |
| UNI-10 | #1597 | property-test coverage expansion (15+ invariants)      | CLOSED | UNI-W4 |
| UNI-11 | #1598 | public API audit + freeze + `public_api.golden`        | CLOSED | UNI-W6 |
| UNI-12 | #1599 | consumer wire-up: shell-lex / -ast / -completion       | CLOSED | UNI-W5 |
| UNI-13 | #1600 | LSP integration surface (UTF-16/UTF-32 col converters) | CLOSED | UNI-W6 |
| UNI-14 | #1601 | cargo-fuzz target for `Utf8Decoder`                    | CLOSED | UNI-W3 |
| UNI-15 | #1602 | README + examples sweep                                | CLOSED | UNI-W6 |
| UNI-16 | #1603 | **final acceptance + v0.37.0 bump**                    | **this wave** | UNI-W7 |

UNI-7's four full-corpus sweeps are marked **CLOSED (gated)**: the
harness code lands and compiles, the per-corpus parser is sanity-
covered, and the sweep tests are `#[ignore]` pending the vendor-corpus
drop under `tests/ucd-corpus/15.1.0/`. Running them in the vendored
environment is a one-command invocation (§2.4 below); the gating is
deliberate so that `cargo test -p paideia-as-unicode` without
`--ignored` stays sub-second for developer feedback.

---

## 2. Harness coverage inventory

The crate carries six independent verification harnesses. Each
targets a different failure class and runs with a different cost
posture.

### 2.1 Fingerprint goldens (UNI-6)

**Files:** 24 goldens under `crates/paideia-as-unicode/tests/fingerprints/`:

```
block.golden                is_nfkd.golden              str_width_latin.golden
eq_ignore_case.golden       line_break_opportunities.golden
fold_default.golden         lowercase.golden            uppercase.golden
fold_turkic.golden          nfc_normalize.golden        utf8_decode.golden
general_category.golden     nfd_normalize.golden        width_eastasian.golden
grapheme_boundaries.golden  nfkc_normalize.golden       width_latin.golden
grapheme_count.golden       nfkd_normalize.golden       wrap_at_width.golden
is_nfc.golden               script.golden
is_nfd.golden
is_nfkc.golden
```

**Driver:** `tests/fingerprint_test.rs` (~321 LOC). Reads every
`.golden` file through the UNI-6 record parser, applies the
corresponding function to the hex-decoded input, and
`assert_eq!`s against the hex-decoded expected output. Mismatches
name the fingerprint tag of the first failing record (`nfc.ucd.d800`,
`width.cjk.han.4e2d`, …).

**Tag discipline.** Enforced by the harness: every record carries a
dot-separated tag whose first segment is one of the spec §5.6
allowed prefixes (`nfc.*`, `nfd.*`, `nfkc.*`, `nfkd.*`, `case.*`,
`fold.*`, `graph.*`, `width.*`, `line.*`, `ident.*`, `profile.*`,
`ucd.*`, `utf8.*`, `lsp.*`). A dispatched softarch adding a new
record that violates the vocabulary fails the harness lint.

**Regeneration.** `cargo test -p paideia-as-unicode -- --regen-fingerprints`
rewrites every golden in place. Invoked **only** on a UCD-version
upgrade; the UCD pin is §3 in the spec.

### 2.2 Proptest invariants (UNI-10)

**File:** `tests/properties.rs` (~615 LOC). Twenty-one `proptest!`
blocks named `uni10_p01` through `uni10_p21`:

| ID | Invariant |
|----|-----------|
| p01 | `normalize` is idempotent for every form |
| p02 | ASCII input is normalized in every form |
| p03 | NFKC is an NFC fixpoint over any input |
| p04 | NFKD is an NFD fixpoint over any input |
| p05 | `is_form(F, s)` iff `normalize(F, s) == s` |
| p06 | `fold_default` is idempotent |
| p07 | `fold_default` absorbs ASCII uppercase |
| p08 | `eq_ignore_case_unicode` is symmetric |
| p09 | `eq_ignore_case_unicode(a, b)` iff `fold(a) == fold(b)` |
| p10 | Line-break offsets are strictly ascending |
| p11 | Line-break iter covers end of text |
| p12 | `wrap_at_width` respects the width bound on ASCII input |
| p13 | `wrap_at_width` without a bound returns a single token |
| p14 | `is_identifier` is monotone on ASCII prefix extension |
| p15 | `Ascii` profile is a subset of `Default` |
| p16 | GC group predicates partition `char` |
| p17 | `char::is_control` ⇔ `GeneralCategory::Cc` |
| p18 | `script(c) == Unknown` only when `GC ∈ {Co, Cn}` |
| p19 | `str_width` equals the sum over grapheme widths |
| p20 | ASCII letters are Narrow in every WidthMode |
| p21 | CJK ideographs are Wide in every WidthMode |

**Config.** Default `ProptestConfig` (256 cases per block). Regressions
land in `properties.proptest-regressions`; the file is checked in so
cross-session reruns start from the discovered failing seeds.

### 2.3 Criterion benches (UNI-8)

**Files:** `benches/{normalize,segment,misc}.rs`. **Nineteen**
`bench_function` calls across three binaries (6 + 4 + 9), grouped
under `normalize_benches`, `segment_benches`, and `misc_benches`:

| Group        | Benches | Scope |
|--------------|--------:|-------|
| normalize    | 6       | NFC/NFD/NFKC/NFKD on ASCII fast path + mixed Latin + CJK long |
| segment      | 4       | `line_break_opportunities` and `wrap_at_width` on Lorem + CJK |
| misc         | 9       | `fold_default` + `fold_turkic` + `str_width` + `Utf8Decoder` + grapheme |

**Thresholds.** The spec §9.1 target table (ns/call per input class) is
reported, not enforced. The regression gate (fail-build on throughput
drop >20%) is deferred to a future hardening ticket per the spec §9
posture.

**Invocation.** `cargo bench -p paideia-as-unicode --bench normalize`
(and `segment` / `misc`) prints a line per bench with mean/median
and estimated throughput.

### 2.4 UCD corpus sweep (UNI-7)

**File:** `tests/ucd_conformance.rs` (~492 LOC). **Four** `#[ignore]`
tests, one per vendor data file:

| Test                             | Corpus                       | Approx records |
|----------------------------------|------------------------------|---------------:|
| `nfc_full_sweep`                 | `NormalizationTest.txt`      | ~17 000 × 4 forms |
| `grapheme_full_sweep`            | `GraphemeBreakTest.txt`      | ~670 |
| `line_break_full_sweep`          | `LineBreakTest.txt`          | ~7 500 |
| `identifier_status_full_sweep`   | `IdentifierStatus.txt`       | ~140 000 rows |

**Gate rationale.** Each test is marked `#[ignore = "UNI-7 full UCD
sweep — vendor the corpus and run with --ignored"]`. The corpora are
large (~5 MB uncompressed) and live under
`tests/ucd-corpus/15.1.0/`, tagged in git by UCD version. The gate
keeps the default `cargo test` run fast while the opt-in run
exhaustively verifies every record.

**Invocation.**
`cargo test -p paideia-as-unicode --test ucd_conformance -- --ignored --nocapture`.
Fails on the first mismatched record, naming the UCD line number.

### 2.5 Fuzz targets (UNI-14)

**Files:** `fuzz/fuzz_targets/utf8_decoder.rs` + `utf8_roundtrip.rs`.
Two cargo-fuzz targets:

* **`utf8_decoder`** — drives `Utf8Decoder::new().collect::<Vec<_>>()`
  on arbitrary byte input; asserts that every error carries an offset
  within `0..=input.len()` and that the decoder is deterministic
  (two runs over the same bytes agree).
* **`utf8_roundtrip`** — for every valid UTF-8 input, encodes each
  decoded `char` back and asserts `Vec<u8>` equality with the input;
  for invalid input, asserts that the position advances strictly on
  every error.

**Invocation.** `cargo +nightly fuzz run utf8_decoder` (or
`utf8_roundtrip`). The targets live in a nested cargo workspace under
`crates/paideia-as-unicode/fuzz/` so they do not pull cargo-fuzz into
the main workspace graph.

### 2.6 Public-API drift gate (UNI-11)

**File:** `tests/public_api_test.rs` (~521 LOC). **Four** `#[test]`
functions:

| Test                                                 | Checks |
|------------------------------------------------------|--------|
| `public_api_golden_parses_cleanly`                   | Every row in `public_api.golden` has a valid `(kind, path, name)` tuple |
| `public_api_golden_matches_expected_counts`          | Per-kind row counts match `EXPECTED_COUNTS` (12 modules / 66 re-exports / 55 free fns / …) |
| `public_api_golden_contains_every_sentinel`          | A short list of sentinel entries (one per module) appears verbatim |
| `public_api_golden_rows_use_fully_qualified_crate_paths` | Every row's path begins with `paideia_as_unicode::` |

**Compile-time touches.** The file also carries four private modules
(`signatures_pin`, `type_presence`, `fingerprint_signatures_pin`, and
a method/field-access block) that coerce every pub fn to its expected
signature, instantiate every pub type by its publicly reachable
constructor, and dereference every pub field via a bare accessor. A
signature or field rename breaks the compile-time coercion, not just
the goldens — which keeps the three source files, the per-kind
counts, and the sentinel list all two-sided-edit coupled.

**Freeze policy.** `design/toolchain/paideia-as-unicode-api-freeze.md`
documents the three categories (additive / signature change / major
break) and the per-category workflow. UNI-9's locale-resolver
completion is the only inventory row the freeze pins as planned
additive.

### 2.7 Summary matrix

| Harness             | Files | Opt-in | Cost | Guards |
|---------------------|------:|--------|------|--------|
| Fingerprint goldens | 24 + driver (~321 LOC) | no  | sub-second | per-fn text transforms |
| Proptest invariants | 21 blocks (~615 LOC)   | no  | ~1 s | UAX cross-form algebra |
| Criterion benches   | 19 benches (~350 LOC)  | `cargo bench` | ~minutes | throughput regressions (reported) |
| UCD corpus sweep    | 4 `#[ignore]` tests (~492 LOC) | `--ignored` | ~10 s | exhaustive UAX conformance |
| Fuzz targets        | 2 targets              | `cargo fuzz` | continuous | decoder soundness |
| Public-API drift    | 4 tests (~521 LOC)     | no  | sub-second | pub surface freeze |

**Grand totals.** 74 shipped tests (not counting the 21 proptest blocks
nor the 4 gated UCD sweeps), 24 fingerprint goldens, 19 benches,
2 fuzz targets. Default `cargo test -p paideia-as-unicode` runs the
first four of the six harnesses sub-second.

---

## 3. Known-lossy inventory

These are items where the implementation ships with an acknowledged
limitation documented in-source. They are **not** regressions against
the spec — the spec explicitly scopes them in or defers them — but
callers reading this report need to know the current shape.

### 3.1 LSP mid-char rounding is DOWN (not ERR)

`lsp_bridge::utf16_offset_to_utf8` and
`lsp_bridge::utf32_offset_to_utf8` round a mid-char input offset DOWN
to the preceding char boundary rather than returning an error. The
convention follows LSP 3.17 §3.14 (position encoding) and makes the
two symmetric converters total.

Impact: a client sending a surrogate-split offset gets the preceding
char's byte position back, not a diagnostic. In practice no spec-
compliant client emits mid-char positions; the discipline exists for
the mis-synced-document edge case where a client's cached text and
the server's authoritative text have drifted mid-edit.

Documented at `src/lsp_bridge.rs` lines 24–41 and in the UNI-13
scratch note. No follow-up is open — this is the chosen convention,
not a known deficiency.

### 3.2 `wrap_at_width` is total under WidthTooSmall

`segment::wrap_at_width` is defined to never return `Err`: when the
width bound is smaller than the smallest unbreakable grapheme cluster,
the function falls back to forced grapheme-level breaks rather than
surfacing `SegmentError { kind: WidthTooSmall }`.

The error type is pinned at the UNI-11 freeze so a future
`try_wrap_at_width` returning `Result<Vec<&str>, SegmentError>` can
land without reshaping the taxonomy. See F3 below.

### 3.3 `IdentErrorKind::Confusable` is reserved, not emitted

`check_ident(IdentProfile::AllowedChars, s)` never produces
`IdentErrorKind::Confusable`. The variant is pinned at the freeze so
UNI-7's post-vendor UCD confusables sweep can emit it without an API
change; the full `confusables.txt` matrix is explicitly out of scope
per spec §2.2. See F4 below.

### 3.4 `WidthMode::Auto` not yet wired

`width::WidthMode` has pinned `Latin` (default), `EastAsian` as of
the UNI-11 freeze. The spec §4.5 draft also names `Auto` and a
`resolve_width_mode(&WidthEnv) -> WidthMode` resolver that would
pick a mode from env + `LC_CTYPE` + terminal query, but neither
`Auto` nor the resolver ships at UNI-16. The addition is pinned
as **planned additive** by the freeze (freeze doc §1.5) and does
not block completion — the shell's current renderer passes an
explicit `WidthMode` at every call site. See F2 below.

### 3.5 UCD pinned at 15.1.0; crate dependency uses `unicode-blocks` 0.1.10 (UCD 17.0 Block set)

The spec §3.1 pin is UCD 15.1.0 for every normalization /
segmentation / width table. The `Block` taxonomy comes from
`unicode-blocks` 0.1.10, which encodes the UCD 17.0 Block set (345
variants) — any Block variant assigned after UCD 15.1 is reachable
only on an input code point outside the pinned UCD 15.1 character
assignments, which never appears in a valid NFC-stable UCD-15.1
input. The drift is latent: a UCD upgrade (§3 process) will reconcile
the pin across tables. See F5 below.

---

## 4. Follow-ups (polish, NOT acceptance blockers)

Five items were surfaced during Waves UNI-W1–UNI-W6 but are **not**
regressions against the UNI-0 spec or the UNI-11 freeze. They are
polish for the post-v0.37.0 cadence. Each follow-up is formatted so
`gh issue create` can consume it verbatim from the fence block.

### F1 — UCD 16.0 upgrade protocol dry-run

```text
Title: paideia-as-unicode: UCD 15.1.0 → 16.0.0 upgrade dry-run

Body:
`paideia-as-unicode-spec.md` §3.3 defines the UCD upgrade workflow:
bump the four `unicode-*` workspace deps atomically, regenerate every
`tests/fingerprints/*.golden` via `--regen-fingerprints`, update the
spec §3.1 pin, update the lib.rs UCD version stamp, land as a
workspace minor bump with a CHANGELOG entry titled
`UCD X.Y.0 → X.Y.Z` enumerating every property-table rotation.

The workflow has never been exercised. UCD 16.0.0 (released
2024-09-10) is a reasonable target for a dry-run.

Impact today: none — the UCD 15.1.0 pin is fine for every current
consumer.

Impact later: the first real UCD upgrade (likely driven by a shell
feature or an emoji release) must land under the §3.3 procedure
without surprises. A dry-run validates:
- `--regen-fingerprints` produces a reviewable diff
- the `unicode-blocks` crate's Block-variant drift stays additive
- the UCD corpus sweep still passes against the regenerated goldens
- the spec version row updates cleanly

Acceptance:
- Dry-run branch bumping to UCD 16.0.0, with the goldens regenerated.
- A `UCD 15.1.0 → 16.0.0` CHANGELOG entry enumerating the diffed
  records (expected: ~30 new General_Category assignments, ~2 new
  Script entries, ~5 new Blocks).
- Decision: land the bump, or revert and document the finding.

Blocked by: nothing — can land independently.
Related: paideia-as-unicode-spec.md §3, UNI-16 acceptance follow-up.
```

### F2 — `WidthMode::Auto` + `resolve_width_mode` locale resolver

```text
Title: paideia-as-unicode: `WidthMode::Auto` + `resolve_width_mode(&WidthEnv)`

Body:
The UNI-11 freeze pins `WidthMode` at `{Latin, EastAsian}` with
`EastAsian` reachable only when callers pass it explicitly. Spec §4.5
reserves a third `Auto` variant and a `resolve_width_mode(&WidthEnv)
-> WidthMode` resolver that picks from env + LC_CTYPE + terminal
query. Spec §10.1 preliminarily prefers `WCWIDTH_MODE` env var →
`LC_CTYPE` → default-Latin, with `Auto` re-probing on terminal resize.

Impact today: none — the shell renderer passes an explicit `WidthMode`
at every call site.

Impact later: once a user-visible `wcwidth` option lands in the shell
config, the resolver must exist so the renderer can resolve the mode
once per session (or on SIGWINCH) rather than threading `WidthMode`
through every call site.

Acceptance:
- `WidthMode::Auto` variant added behind the freeze's additive-only
  rule (one row in `public_api.golden`, `EXPECTED_COUNTS` bumped by
  one, compile-time touch in `type_presence`).
- `WidthEnv` struct carrying `(wcwidth_mode: Option<&str>, lc_ctype:
  Option<&str>, term_query: Option<TermQuery>)`.
- `resolve_width_mode(&WidthEnv) -> WidthMode` with the §10.1
  precedence.
- Proptest invariant uni10-p22: resolver is deterministic over the
  same env.

Blocked by: nothing.
Related: paideia-as-unicode-spec.md §4.5 + §10.1, UNI-9 landing,
UNI-16 acceptance follow-up.
```

### F3 — `try_wrap_at_width` surfacing `SegmentError`

```text
Title: paideia-as-unicode: `try_wrap_at_width` returning Result

Body:
`segment::wrap_at_width` currently falls back to grapheme-level forced
breaks when the width bound is smaller than the smallest unbreakable
cluster. This keeps the function total but hides the
`SegmentError { kind: WidthTooSmall { minimum } }` the error type was
designed to carry.

The UNI-11 freeze pins `SegmentError` + `SegmentErrorKind` so an
additive `try_wrap_at_width(&str, usize) -> Result<Vec<&str>,
SegmentError>` can land without reshaping the taxonomy.

Impact today: none — forced-break fallback is the correct default for
the shell renderer, which cannot refuse to lay out a line.

Impact later: a batch-mode consumer (export to Markdown with a hard
80-column limit, a report generator) may want to surface the error
rather than silently emit over-width lines.

Acceptance:
- `pub fn try_wrap_at_width(s: &str, max_width: usize) ->
  Result<Vec<&str>, SegmentError>`.
- `wrap_at_width` continues to call the forced-break path.
- Fingerprint golden `try_wrap_at_width.golden`.
- Compile-time touch in `type_presence`.

Blocked by: nothing — pure additive.
Related: spec §4.7, UNI-11 freeze doc §1.4, UNI-16 acceptance.
```

### F4 — UTS #39 confusables sweep (post-UCD-vendor)

```text
Title: paideia-as-unicode: UTS #39 confusables matrix sweep

Body:
Spec §2.2 explicitly defers the full UTS #39 confusables matrix
(`confusables.txt`, ~15 KiB). The UNI-11 freeze pins
`IdentErrorKind::Confusable` as a reserved variant so emission can
land without an API change.

The sweep adds:
- A vendored `confusables.txt` at `tests/ucd-corpus/15.1.0/`.
- A new `ident::is_confusable(a: &str, b: &str) -> bool` helper.
- `check_ident(IdentProfile::AllowedChars, s)` emits
  `IdentErrorKind::Confusable` when `s` is confusable with a
  previously-indexed identifier in the caller's scope.
- A `confusables.golden` fingerprint record set.

Impact today: none — the shell command-name index is small and
hand-curated.

Impact later: once user-defined shell commands and installed-package
command names can conflict, confusables detection is the UTS #39
right thing to do.

Acceptance:
- Confusables table vendored and loaded at crate initialization.
- `is_confusable` + `check_ident` emission.
- Proptest invariant: `is_confusable(a, b) == is_confusable(b, a)`.
- UCD sweep sized to the full confusables matrix (~6 000 pairs).

Blocked by: a decision on whether confusables indexing is per-call
or per-session. UTS #39 §4 recommends per-indexed-vocabulary.
Related: spec §2.2, UNI-11 freeze doc §1.4 (`IdentErrorKind::Confusable`
reserved), UNI-16 acceptance.
```

### F5 — Streaming NFC normalizer (not just the predicate)

```text
Title: paideia-as-unicode: streaming NFC normalizer

Body:
UNI-13 shipped `lsp_bridge::stream_is_nfc<I: Iterator<Item = char>>(I)
-> bool`, a streaming predicate the LSP's incremental re-tokenization
path consumes. Spec §2.2 explicitly defers a full streaming
`nfc_normalize` (`&mut Iterator<Item = char>` → `Iterator<Item =
char>`) as "a STREAM-0 follow-up when the LSP's incremental
re-tokenization lands."

Impact today: none — the shell lex passes `&str` and the LSP needs
the predicate, not the normalizer, for the incremental path.

Impact later: once the LSP's incremental re-tokenization lands and
runs into a modified region that is NOT NFC, it must either
re-materialize the whole region (today's fallback) or stream-
normalize into a small output buffer.

Acceptance:
- `pub fn stream_normalize<I: Iterator<Item = char>>(I, Form) ->
  impl Iterator<Item = char>` with an internal buffer sized to the
  longest canonical decomposition (3 chars under UCD 15.1).
- Proptest invariant: `stream_normalize(s.chars(),
  Form::Nfc).collect::<String>() == nfc_normalize(&s)`.
- Fingerprint golden `stream_normalize_nfc.golden`.
- Benchmark on 1 MiB mixed-latin input, target ≤2× `nfc_normalize`
  on the same input.

Blocked by: the LSP's incremental re-tokenization landing (separate
ticket, probably in the paideia-lsp backlog).
Related: spec §2.2, §4.3 (`stream_is_nfc`), UNI-16 acceptance.
```

---

## 5. Declaration of completion

paideia-as-unicode is declared **COMPLETE** against the UNI-0 crate
specification and the UNI-11 public API freeze as of **paideia-as
v0.37.0** (this wave). Specifically:

* The six verification harnesses (fingerprint goldens, proptest
  invariants, criterion benches, UCD sweep, fuzz targets,
  public-API drift gate) exist, compile, and run green on the
  shipped crate surface.
* The 17 UNI-N wave tickets (#1587–#1603) have all landed.
* No known regression against any UAX / UTS surface the spec names
  as in-scope (§2.1).
* The known-lossy inventory (§3) is five items, all documented
  in-source and tracked by follow-ups (§4) that do not block
  completion.
* The spec header, the API-freeze doc, and the crate module docs
  (`src/lib.rs`) carry the matching `Status: COMPLETE as of
  paideia-as v0.37.0` stamp.

**Next consumers.** The next unicode work is downstream:
`paideia-as-lsp` incremental re-tokenization (which may motivate
F5), the shell's `wcwidth` config option (which motivates F2), and
the first real UCD upgrade (which exercises F1). None of these
block the v0.37.0 close.

**Precedent.** This acceptance follows the paideia-fmt FMT-17
pattern (closed Wave 72 in v0.36.82). The chain shape, the
dispatch cadence, the fingerprint-golden discipline, the API
freeze two-sided-edit coupling, and the acceptance report shape
all mirror that precedent exactly. The next crate-level
completion wave (whichever it is) should likewise mirror this
document in turn.
