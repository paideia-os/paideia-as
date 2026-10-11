# paideia-as-unicode Public API Freeze — UNI-11

**Issue:** [#1598](https://github.com/paideia-os/paideia-as/issues/1598)
(UNI-11 — Wave UNI-W6).
**Spec:** [`paideia-as-unicode-spec.md`](paideia-as-unicode-spec.md) §4.
**Golden snapshot:** `crates/paideia-as-unicode/tests/public_api.golden`.
**Drift gate:** `crates/paideia-as-unicode/tests/public_api_test.rs`.
**Status:** **FROZEN** at 2026-10-11 against workspace version `0.37.0`
(pre-bump; the bump itself lands at UNI-16 close).

This document is the companion to the spec's §4 inventory and the
on-disk golden file. The spec says *what the inventory contains*; the
golden file says *exactly which symbol lives on the pub surface at the
byte level*; this file says *what changes are allowed to that surface
and under what discipline*.

Three audiences:

1. **UNI-N implementors** writing a follow-up after UNI-11 (UNI-9 width
   locale resolver completion, UNI-13 LSP column converters, UNI-15/16
   hardening). This doc tells you what you may and may not change
   without a UNI follow-up ticket and a minor-version bump.
2. **Consumers** of the crate (`paideia-as-shell-lex`,
   `paideia-as-shell-ast`, `paideia-as-shell-completion`, future
   `paideia-as-lsp`). This doc tells you which symbols you may rely on
   never silently moving.
3. **Reviewers** of a PR that touches `src/*.rs` or the golden file.
   This doc tells you the two-sided-edit rule and the regen workflow.

---

## 1. Frozen surface, by category

The byte-level enumeration is `tests/public_api.golden`. The audit
totals at freeze line are:

| Category | Count | Where defined |
|---|---:|---|
| Modules (`pub mod` in `lib.rs`) | 12 | `src/lib.rs` |
| Crate-root re-exports (`pub use ...::X;`) | 66 | `src/lib.rs` |
| Intra-module re-exports (`pub use` inside a module) | 4 | `src/nfc.rs`, `src/ucd.rs` |
| Pub structs | 8 | `src/{error,fingerprint,grapheme,ident,segment,utf8}.rs` |
| Pub struct fields | 14 | — |
| Pub enums | 11 | `src/{case,error,ident,norm,segment,ucd,width}.rs` |
| Pub enum variants | 405 | 345 in `ucd::Block`, 30 in `ucd::GeneralCategory`, 30 elsewhere |
| Pub free functions | 55 | across all modules |
| Pub inherent methods | 5 | `utf8::Utf8Decoder` (3), `width::Width` (2) |
| Trait impls on crate types | 9 | `Display`/`Error` on error types, `Iterator` on `Utf8Decoder` and `GraphemeBoundaries`, `From` on `GeneralCategory` |

**Total audit entries: 589.** Each row in the golden file corresponds
to one entry here. The UNI-13 LSP bridge (new module `::lsp_bridge`
with five pub fns and five crate-root re-exports) is included in the
freeze.

### 1.1 Modules

Twelve top-level modules, all pub, no sub-modules:

```
paideia_as_unicode::case
paideia_as_unicode::error
paideia_as_unicode::fingerprint
paideia_as_unicode::grapheme
paideia_as_unicode::ident
paideia_as_unicode::lsp_bridge     # UNI-13 LSP position-encoding bridge
paideia_as_unicode::nfc            # shim module; re-exports from ::norm
paideia_as_unicode::norm
paideia_as_unicode::segment
paideia_as_unicode::ucd
paideia_as_unicode::utf8
paideia_as_unicode::width
```

### 1.2 Shapes pinned at the freeze

The surface listing is in the spec §4 and the golden file; here are the
shape-level invariants the freeze codifies on top of that enumeration.

* **`Utf8Decoder<'a>`** — three inherent methods (`new`, `position`,
  `is_exhausted`) plus `Iterator<Item = Result<char, Utf8Error>>`. The
  signature on `new` is `fn(&'a [u8]) -> Self` — the lifetime is tied
  to the input slice.
* **`Utf8Error`** — plain data struct with pub `offset: usize`,
  `reason: Utf8ErrorKind`. `#[derive(Clone, Copy, Debug, PartialEq,
  Eq)]`. Pub constructor-friendly: external callers may build their
  own `Utf8Error` values (the fingerprint harness relies on this).
* **`Utf8ErrorKind`** — six variants (`InvalidLeadByte`,
  `InvalidContinuationByte`, `UnexpectedEndOfInput`, `Overlong`,
  `Surrogate`, `OutOfRange`). Enum is NOT `#[non_exhaustive]`: the
  RFC 3629 taxonomy is finite and we treat these six as exhaustive.
  **No new variants** may be added without a major-version bump.
* **`GraphemeBoundaries<'a>`** — opaque iterator struct; the pub
  surface is only `Iterator<Item = usize>`. Fields are private;
  construct via `grapheme_boundaries(&str)`.
* **`Form`** (UAX #15) — `#[non_exhaustive]`, variants `Nfc` (default),
  `Nfd`, `Nfkc`, `Nfkd`. Dispatch wraps `nf*_normalize` 1:1 with
  the SH-D9.4 fast-path contract: ASCII returns an owned copy without
  touching UCD tables.
* **`CaseFoldLocale`** — `#[non_exhaustive]`, variants `Default`
  (default), `Turkic`. Lithuanian / Azerbaijani locale additions are
  reserved; they are additive-only and require a UNI follow-up.
* **`WidthMode`** — `#[non_exhaustive]`, variants `Latin` (default),
  `EastAsian`. `WindowsConsole`/`Legacy` variants are reserved.
* **`Width`** — four variants (`Zero`, `Narrow`, `Ambiguous`, `Wide`).
  NOT `#[non_exhaustive]`: UAX #11 defines six properties collapsing
  to four rendered widths; the taxonomy is finite.
* **`LineBreak`** — `#[non_exhaustive]`, variants `Mandatory`,
  `Allowed` (default), `Prohibited`. `Prohibited` is reserved for a
  future per-byte query API; `line_break_opportunities` currently
  never emits it.
* **`LineBreakOpportunity`** — `#[non_exhaustive]` struct with pub
  `byte_offset: usize`, `kind: LineBreak`. External callers may
  destructure via `_` rest patterns; direct construction is **not**
  part of the pub surface (`#[non_exhaustive]` on a struct bars that).
* **`SegmentError`** — `#[non_exhaustive]` struct with pub
  `offset: usize`, `kind: SegmentErrorKind`. Produced by
  `wrap_at_width` — but today the function never returns an error
  (the forced-break fallback keeps it total). The type is pinned at
  the freeze so UNI-15+ hardening can add `try_wrap_at_width` without
  reshaping the error taxonomy.
* **`SegmentErrorKind`** — `#[non_exhaustive]`, variant
  `WidthTooSmall { minimum: usize }`. Future failure modes land as
  additions behind `#[non_exhaustive]`.
* **`IdentProfile`** — `#[non_exhaustive]`, variants `Ascii`,
  `Default` (default), `AllowedChars`. `ModeratelyRestricted` and
  `MinimallyRestricted` are reserved for UNI-7's confusables sweep.
* **`IdentErrorKind`** — `#[non_exhaustive]`, variants `BadStart`,
  `BadContinue`, `NotNfc`, `MixedScript { scripts: Vec<Script> }`,
  `Confusable`. `Confusable` is reserved; `check_ident` does not emit
  it until UNI-7 lands the UCD vendor sweep.
* **`IdentError`** — plain struct with pub `offset: usize`,
  `kind: IdentErrorKind`. NOT `#[non_exhaustive]` on the struct — new
  payload grows on `IdentErrorKind`, not on `IdentError`.
* **`GeneralCategory`** — `#[non_exhaustive]`, 30 UCD two-letter short
  variants (`Lu`, `Ll`, …, `Cn`). Also carries `impl From<unicode_properties::GeneralCategory>`.
* **`Script`** — re-exported from `unicode_script::Script`. Upstream
  is already `#[non_exhaustive]`; our freeze does not re-pin that
  (we track its UCD cadence via the normal workspace-dependency pin).
* **`Block`** — `#[non_exhaustive]`, 345 variants per UCD 17.0 (via
  `unicode-blocks` 0.1.10) plus `Unassigned`. UCD-cadence additions
  are a §3 process, not a UNI follow-up.
* **`FingerprintCase`** / **`FingerprintRecord`** — plain data structs
  from UNI-6. Pub fields `tag`, `input`, `expected`. NOT
  `#[non_exhaustive]`. Public construction is a load-bearing API —
  every per-function fingerprint test file instantiates these in a
  static array.

### 1.3 Modules whose shape is unchanged at freeze

* **`::utf8`** — RFC 3629 decoder, three inherent methods + Iterator.
* **`::grapheme`** — UAX #29 boundary iterator + four accessor fns.
* **`::case`** — UAX #44 fold + locale enum + five fns.
* **`::fingerprint`** — golden-file harness; two structs, six fns.
* **`::nfc`** — shim re-exporting three fns from `::norm`.

### 1.4 Modules reserved for additive-only growth

* **`::width`** — the UNI-9 locale-resolver sketch is at §4.5 (`WidthMode`
  variants, `width_in`, `str_width_in`, `grapheme_width_in` landed in
  the freeze; a future `resolve_width_mode(&WidthEnv) -> WidthMode`
  function is additive).
* **`::ucd`** — General Category / Script / Block all live in the
  freeze; UNI-7's confusables / IdentifierStatus adds additive fns
  under this module (e.g., `is_identifier_status(c, Status) -> bool`).
* **`::ident`** — `IdentErrorKind::Confusable` is reserved; UNI-7's
  wiring enables emission without an API change.
* **`::segment`** — `SegmentErrorKind::WidthTooSmall` is reserved;
  UNI-15 can add `try_wrap_at_width` returning `Result<..., SegmentError>`.

### 1.5 Modules **not yet landed** (post-freeze additive)

As of the UNI-11 freeze line all planned chain modules exist in-tree.
Spec §4.10's `::lsp` draft was superseded by the `::lsp_bridge`
module that actually landed under UNI-13 (`paideia-as#1600`); it is
part of the freeze (§1.1 twelfth module).

The UNI-9 width-locale resolver completion (`WidthMode::Auto`,
`resolve_width_mode`, `WidthEnv`) is the only inventory row the freeze
pins as **planned additive**. Its addition proceeds under §2.1 rules.

---

## 2. Post-freeze change policy

Changes to the pub surface split into three categories. Each requires
a specific procedure.

### 2.1 Additive change (new pub item, no breakage)

**Allowed under a UNI follow-up ticket, no major-version bump.**

Procedure:

1. Open the UNI-N ticket on paideia-as#.
2. Implement the new pub item in `src/<module>.rs`. Re-export at
   crate root if the symbol is user-facing (follow the existing
   `pub use` block layout in `src/lib.rs`).
3. Add exactly one row per new entry to
   `crates/paideia-as-unicode/tests/public_api.golden`, placed under
   the correct `# == paideia_as_unicode::<module> ==` section
   header; keep rows in the same order the module source declares
   them in.
4. Update `EXPECTED_COUNTS` in
   `crates/paideia-as-unicode/tests/public_api_test.rs` so the per-
   kind counts match the new golden.
5. If the new item is the first of its kind in its module, add a
   sentinel entry to `SENTINEL_ENTRIES` in the same file.
6. Add a compile-time touch for the new fn / method / enum variant /
   struct field inside the `compile_time_touches` module in the test
   file. Function pointers for fns, closures or trampolines for
   methods, type-presence `let _ = Enum::NewVariant;` for variants,
   borrowed accessors for `#[non_exhaustive]` struct fields.
7. Update spec §4 to add the row to the correct inventory sub-section,
   marked `Status: UNI-N`.
8. Update this companion doc's §1 table (`Count` column) and §1.5
   (if landing a reserved new module).
9. Add an entry to the workspace CHANGELOG under the version at
   landing time.

The test harness enforces (3)–(6) at `cargo test`-time. The spec /
companion-doc / CHANGELOG edits (7)–(9) are review-time disciplines.

### 2.2 Signature or behavior change on a frozen symbol

**Requires a UNI follow-up ticket AND a minor-version bump.** Signature
changes are near-breaking; even if the Rust compiler accepts them under
coherence, downstream call-sites may re-dispatch unexpectedly.

Procedure:

1. Open the UNI-N ticket with a rationale naming each affected call
   site in `paideia-as-shell-lex`, `paideia-as-shell-ast`,
   `paideia-as-shell-completion`, and any downstream consumer (search
   with `rg -t rust 'paideia_as_unicode::<symbol>'`).
2. Land the signature change behind a `#[deprecated]` alias retaining
   the old shape when possible. If the change is strictly semantic
   (same signature, new behavior), bump the minor version at the same
   PR.
3. Regenerate the golden row, update `EXPECTED_COUNTS` only if the
   kind changes, update sentinels only if the path changes.
4. Add a `## Status change — vX.Y.Z` block to spec §1 noting the
   change.
5. Document the behavior delta in this companion doc under a new §3
   entry (see §3).

### 2.3 Removal of a frozen symbol

**Requires a UNI follow-up ticket AND a major-version bump.** The crate
has not shipped a major-version-bumped surface change yet; the first
such change opens a `v1.0.0` discussion.

Procedure:

1. Open the UNI-N ticket as a `semver-break`-labelled issue.
2. Deprecate the symbol under `#[deprecated(since = "0.X.Y", note =
   "replaced by …; removal targets vA.B.0")]` for at least one
   workspace release.
3. Only after the deprecation cycle, remove the symbol, delete its
   golden row, update `EXPECTED_COUNTS` / sentinels / compile-time
   touches in `public_api_test.rs`, update spec §4, and add a §3
   entry here.

---

## 3. Post-freeze change log

This section accumulates one block per post-freeze change under a
`### <date> — <UNI-N> — <kind: additive|signature|removal>` header.
Each block names the symbol, the ticket, the version at landing, and
links to the PR.

*(No entries yet — this section opens at the first post-UNI-11 change.)*

---

## 4. Enforcement model

Four complementary mechanisms keep the freeze honest:

1. **The golden file is in-tree.** `crates/paideia-as-unicode/tests/public_api.golden`
   is tracked by git; any edit shows up in `git diff` and in PR review.
2. **The drift gate parses that file at `cargo test`-time.**
   `tests/public_api_test.rs` has four `#[test]`s:
   `public_api_golden_parses_cleanly` (format),
   `public_api_golden_matches_expected_counts` (per-kind counts),
   `public_api_golden_contains_every_sentinel` (presence of named
   entries), and
   `public_api_golden_rows_use_fully_qualified_crate_paths` (every row
   begins `paideia_as_unicode::` or is the bare crate root).
3. **Compile-time touches.** The `compile_time_touches` module in the
   test file references every pub fn / method / variant / field with
   fn-pointer coercions or borrowed accessors. If source renames a
   symbol or changes a signature without regenerating the golden +
   updating the touches, the test file fails to compile.
4. **Spec + companion-doc parity.** This doc's §1 table and spec §4
   are the two human-readable mirrors of the golden. PR review
   enforces that both update together.

**Line-end normalization.** The golden file survives `autocrlf`,
editor fingerprints, and `sed -i`-style whole-file rewrites: the
parser strips `\r` and trailing whitespace per line before any
comparison, and skips `#`-prefixed comment lines + blank lines.
That keeps the file portable and keeps the test from flaking under
Windows-style line endings.

**Regeneration.** There is deliberately no runtime regen mode — the
freeze discipline is two-sided (golden + inventory in the test file)
precisely so no single-file edit can quietly change the surface. A
reviewed addition updates both files in the same PR; the drift gate
then passes.

---

## 5. Cross-references

* [`paideia-as-unicode-spec.md`](paideia-as-unicode-spec.md) — the
  authoritative design doc. §4 is the inventory this file mirrors.
* `crates/paideia-as-unicode/tests/public_api.golden` — the byte-level
  frozen surface.
* `crates/paideia-as-unicode/tests/public_api_test.rs` — the drift gate.
* `.plans/scratch/UNICODE-dispatch-map.md` — the UNI-N wave dispatch map.
* `paideia-fmt-style-spec.md` — the precedent UNI chain follows for
  freeze cadence (FMT-17 → `v0.36.82`).
