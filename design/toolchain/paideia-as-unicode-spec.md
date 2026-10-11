# paideia-as-unicode Crate Specification

**Issue:** [#1587](https://github.com/paideia-os/paideia-as/issues/1587)
(UNI-0 — Wave UNI-W1 head of the paideia-as-unicode completion chain).
**Status:** **AUTHORED as of 2026-10-11.** Target: **FROZEN at UNI-11**
(public API audit, [#1598](https://github.com/paideia-os/paideia-as/issues/1598)).
Target close: **COMPLETE at UNI-16 (v0.37.0)** mirroring the
`paideia-fmt` precedent that closed at FMT-17 in v0.36.82.

**Precedent.** This document is the UNI chain's analogue of
[`paideia-fmt-style-spec.md`](paideia-fmt-style-spec.md): the authoritative
reference every subsequent UNI-N ticket cites. Where the fmt spec settled
style questions for a formatter, this one settles the surface, invariants,
conformance posture, and testing discipline of the shell's Unicode substrate.

**Audience.** (a) Future UNI-N implementors deciding a specific contract
without ambiguity; (b) consumers of the crate (`paideia-as-shell-lex`,
`paideia-as-shell-ast`, `paideia-as-shell-completion`, future LSP) when
auditing their call sites; (c) reviewers vetting a UCD upgrade PR against
the fingerprint-golden harness UNI-6/UNI-7 define.

**Scope.** Every rule here is testable: either the public surface
observably exhibits it (behavior), or the test harness enforces it
(fingerprint). Non-testable guidance is omitted. When two rules conflict,
the earlier section wins unless the later section calls out an exception.

**Non-goals.** This document does not specify (a) the implementation of
individual UAX algorithms (that is the Servo-maintained
`unicode-normalization` / `unicode-segmentation` / `unicode-width` /
`unicode-ident` crates); (b) the shell's grammar or token taxonomy (that
is `design/terminal/semantic-shell.md`); (c) the lexer's context-tracking
state machine (that is R221.M4, in `paideia-as-shell-lex`); (d) the
R229 renderer's cell model beyond the width-counts this crate feeds it.

---

## 1. Status

| Field | Value |
|---|---|
| Spec status | **COMPLETE (2026-10-11, UNI-16, v0.37.0)** — all 17 UNI tickets landed; §4 FROZEN; acceptance at [`paideia-as-unicode-acceptance.md`](paideia-as-unicode-acceptance.md) |
| Crate status | **COMPLETE** — UNI-0..UNI-15 shipped; six verification harnesses green; UNI-9 `WidthMode::Auto` + `resolve_width_mode` is the single planned-additive row (follow-up F2) |
| Workspace version at authoring | `0.36.83` (post-FMT-17 line) |
| Workspace version at UNI-11 freeze | `0.37.0` target — bump happens at UNI-16 close, freeze is pre-bump |
| Workspace version at completion | **0.37.0** (minor bump matching the paideia-fmt v0.36.82 cadence) |
| Chain | UNI-0..UNI-16 (17 tickets) |
| Chain head | UNI-0 ([#1587](https://github.com/paideia-os/paideia-as/issues/1587)) — this doc |
| Chain capstone | UNI-16 ([#1603](https://github.com/paideia-os/paideia-as/issues/1603)) — final acceptance report at `design/toolchain/paideia-as-unicode-acceptance.md` |
| Dispatch map | `.plans/scratch/UNICODE-dispatch-map.md` |
| Freeze companion | [`paideia-as-unicode-api-freeze.md`](paideia-as-unicode-api-freeze.md) — enumerates the frozen surface and the deprecation policy |

### Status change — v0.37.0

- **2026-10-11 (UNI-16, Wave UNI-W7).** `paideia-as-unicode` is
  declared **COMPLETE** against this spec and the UNI-11 public API
  freeze. The 17 UNI chain tickets (#1587–#1603) have all landed; the
  six verification harnesses (24 fingerprint goldens, 21 proptest
  invariants, 19 criterion benches, 4 gated UCD-corpus sweeps, 2
  cargo-fuzz targets, 4 public-API drift-gate tests) are green against
  the shipped crate surface; the five known-lossy items are documented
  under follow-ups F1..F5 in the acceptance report. The acceptance
  report lives at
  [`paideia-as-unicode-acceptance.md`](paideia-as-unicode-acceptance.md).
  §12 change-log row added. Every section bar §12 is now locked
  against anything but a UCD-upgrade or additive-freeze update (per
  freeze policy).

- **2026-10-11 (UNI-11, Wave UNI-W6).** §4 **FROZEN** at the inventory
  now mirrored in `crates/paideia-as-unicode/tests/public_api.golden`.
  Enforcement: `cargo test -p paideia-as-unicode --test public_api_test`
  parses the golden, checks per-kind counts against an in-source
  authoritative inventory, verifies every sentinel entry, and
  compile-time-touches every top-level pub item. Audit totals at freeze:
  12 modules (`::lsp_bridge` is the UNI-13 landing in the same wave),
  66 crate-root re-exports, 4 intra-module uses, 11 enums (405
  variants, 345 of them in `ucd::Block`), 8 structs with 14 pub fields,
  55 free functions, 5 inherent methods, 9 trait impls — 589 total
  audit entries. See the freeze companion doc for the authoritative
  surface + the per-category deprecation policy. The UNI-9 locale-
  resolver completion is the only inventory row the freeze pins as
  *planned additive*: addition is a UNI follow-up ticket, lands behind
  a parallel update to the golden + the companion doc + the inventory
  arrays in `tests/public_api_test.rs`, and does not alter any entry
  already in the freeze.

The spec AUTHORED → **FROZEN** transition landed at UNI-11 as described
above. Freezing means every `pub` symbol under `paideia_as_unicode` is
enumerated in `crates/paideia-as-unicode/tests/public_api.golden`
(the UNI-11 deliverable) and any addition or signature change
post-freeze requires a UNI follow-up ticket plus a workspace minor-
version bump. Freezing does NOT lock the UCD version — UCD upgrades
remain a separate discipline (§3).

---

## 2. Scope statement

### 2.1 In scope

| UAX / UTS | Topic | Lands at | Status |
|---|---|---|---|
| **UAX #11** | East-Asian Width | M3 (shipped) + UNI-9 (locale resolver) | partial |
| **UAX #14** | Line Breaking | UNI-3 | planned |
| **UAX #15** | Normalization — all four forms (NFC, NFD, NFKC, NFKD) | M1 (NFC shipped) + UNI-1 (NFD/NFKC/NFKD) | partial |
| **UAX #29** | Text Segmentation (extended grapheme clusters, word boundaries) | M2 (graphemes shipped) + UNI-3 word boundaries if needed for UAX#14 | partial |
| **UAX #31** | Identifiers and Syntax | UNI-4 | planned |
| **UTS #39 §5** | ID security profile — Moderately-Restricted scripts allowed list | UNI-4 | planned |
| **UCD** | Property adapter (General Category, Script, Block) | UNI-5 | planned |
| **UCD** | Conformance corpus harness (five canonical test files) | UNI-7 | planned |
| **RFC 3629** | UTF-8 streaming decoder with per-byte error localization | M1 (shipped) + UNI-14 (fuzz) | partial |
| **LSP** | UTF-16 / UTF-32 column converters | UNI-13 | planned |

### 2.2 Out of scope (deferred to future rounds)

| Topic | Reason | Future ticket |
|---|---|---|
| **UAX #9 (Bidi)** | R229 renderer is monospace LTR-only; shell has no RTL fallback. | Deferred until a shell renderer with Bidi support exists. |
| **UTS #46 (IDNA)** | Not a shell concern; URL/hostname handling lives in a separate crate (none yet). | IDNA-0..IDNA-N follow-up when a URL crate is proposed. |
| **UCA #10 (Collation)** | Needed by the shell-sort command and the completion ranker; nontrivial table size (~500 KiB). Deferred to UCA-0..UCA-N after UNI-16 closes. | UCA follow-up round. |
| **UTS #39 confusables (full)** | Only the Moderately-Restricted allowed-chars profile lands at UNI-4. Full confusables matrix (`confusables.txt`, ~15 KiB) is a follow-up. | CONFUSABLES-0..N follow-up. |
| **Streaming NFC** | Current `nfc_normalize` is `&str → String`. UNI-13 adds a `stream_is_nfc` quick-check for the LSP incremental path, but not a full streaming normalizer. | A STREAM-0 follow-up when the LSP's incremental re-tokenization lands. |
| **Hyphenation (UAX #14 extension / Knuth–Liang)** | UNI-3 lands break *opportunities* only, not optional-hyphen insertion. | HYPH-0 follow-up tied to R229 paragraph layout. |
| **Case detection for script-specific tailorings beyond Turkic** | UNI-2 ships default and Turkic case fold; Lithuanian, Azerbaijani, locale-specific Greek final-sigma special cases are out. | TAILOR-0 follow-up. |

The deferred list is reproduced verbatim in UNI-16's acceptance report §Deferred as `gh issue create`-ready blocks, mirroring the paideia-fmt F1..F5 pattern.

### 2.3 Non-ownership

The crate does NOT own:

* The context-tracking lexer (R221.M4 — lives in `paideia-as-shell-lex`).
* The unified AST (R221.M5 — lives in `paideia-as-shell-ast`).
* Source-span tracking across normalization (R221.M6 — a separate layer
  that *uses* `nfc_normalize` and keeps its own byte-range provenance).
* The `.pds` schema loader (R227).
* Tab-completion ranking (R228 — the ranker consumes `grapheme_count` and
  `str_width` but does not re-export them).
* The R229 renderer's cell model (consumes `str_width` and `Width`).

---

## 3. Unicode version pin

### 3.1 Pin

`paideia-as-unicode` targets **UCD 15.1.0** (released 2023-09-12).

All vendored UCD tables (combining classes, grapheme-break properties,
line-break classes, East-Asian width, identifier status, script+block
assignments) MUST agree on this version. The upstream crates
(`unicode-normalization`, `unicode-segmentation`, `unicode-width`,
`unicode-ident`) are pinned at the workspace root to versions that ship
UCD 15.1 (see `Cargo.toml` workspace deps block).

### 3.2 Pin rationale

* **Reproducibility.** A UCD version change silently alters every
  normalization and segmentation result over characters assigned in the
  new version. Freezing the pin keeps every `.pdx` corpus deterministic
  across builds and releases.
* **Fingerprint stability.** Golden files for conformance tests (§5)
  encode UCD-dependent output. Changing the UCD invalidates every
  golden file simultaneously; batching the invalidation under one
  upgrade PR is cheaper than mysterious drift.
* **Alignment with shell contract.** SH-D9.4 requires that NFC
  normalization is observably stable: two paideia-as installations at
  the same `v<X.Y.Z>` MUST return identical normalization results for
  identical inputs. The UCD pin is what makes this testable.

### 3.3 Upgrade policy

A UCD upgrade is a **standalone ticket**, not folded into a feature
ticket. The upgrade PR:

1. Bumps the four upstream crate versions atomically in the workspace
   `[workspace.dependencies]` block.
2. Updates every `tests/fingerprints/*.golden` file by running
   `cargo test -p paideia-as-unicode -- --regen-fingerprints` (UNI-6
   provides this flag).
3. Updates this spec's §3.1 pin version.
4. Updates `crates/paideia-as-unicode/src/lib.rs` module-doc UCD version
   stamp.
5. Lands as a **minor** workspace version bump (one UCD per minor), with
   a CHANGELOG entry explicitly titled `UCD X.Y.0 → X.Y.Z` enumerating
   every property-table rotation.
6. Requires manual review of every golden-file diff by at least one
   reviewer other than the dispatching main.

The upgrade is NEVER a patch bump: a UCD upgrade is behaviorally
observable and consumer-affecting even when no API changes.

---

## 4. Public API inventory

**Status: FROZEN as of 2026-10-11 (UNI-11, Wave UNI-W6).** The
inventory below is the canonical reading of the pub surface; the
on-disk snapshot the test harness enforces is
`crates/paideia-as-unicode/tests/public_api.golden`, and the
freeze policy (deprecation cadence, UNI-13 additive-only rules,
regen workflow) lives in
[`paideia-as-unicode-api-freeze.md`](paideia-as-unicode-api-freeze.md).

Enumerated by module. Column key: *UAX* = backing standard; *Status* =
`shipped` (lives in the crate today — FROZEN at UNI-11), `UNI-N`
(lands at that wave, additive-only against the freeze).

### 4.1 `paideia_as_unicode::error` (crate root re-exports starred)

| Symbol | UAX | Status |
|---|---|---|
| `Utf8Error` struct* | RFC 3629 | shipped |
| `Utf8Error::offset` field | — | shipped |
| `Utf8Error::reason` field | — | shipped |
| `Utf8ErrorKind` enum* | RFC 3629 | shipped (UNI-11 may rename variants during API freeze if needed; no new variants) |
| `Utf8ErrorKind::InvalidLeadByte` | RFC 3629 §3 | shipped |
| `Utf8ErrorKind::InvalidContinuationByte` | RFC 3629 §3 | shipped |
| `Utf8ErrorKind::UnexpectedEndOfInput` | RFC 3629 §3 | shipped |
| `Utf8ErrorKind::Overlong` | RFC 3629 §3 | shipped |
| `Utf8ErrorKind::Surrogate` | RFC 3629 §3 | shipped |
| `Utf8ErrorKind::OutOfRange` | RFC 3629 §3 | shipped |
| `NormalizeError` struct | UAX #15 | **UNI-1** |
| `NormalizeErrorKind` enum | UAX #15 | **UNI-1** |
| `SegmentError` struct | UAX #14 | **UNI-3** |
| `IdentError` struct | UAX #31 + UTS #39 | **UNI-4** |
| `IdentErrorKind` enum | UAX #31 + UTS #39 | **UNI-4** |

### 4.2 `paideia_as_unicode::utf8`

| Symbol | UAX | Status |
|---|---|---|
| `Utf8Decoder<'a>` struct* | RFC 3629 | shipped |
| `Utf8Decoder::new(&[u8]) -> Self` | — | shipped |
| `Utf8Decoder::position(&self) -> usize` | — | shipped |
| `Utf8Decoder::is_exhausted(&self) -> bool` | — | shipped |
| `impl Iterator<Item=Result<char, Utf8Error>>` | RFC 3629 | shipped |

### 4.3 `paideia_as_unicode::nfc` (will be renamed to `::normalize` at UNI-1)

| Symbol | UAX | Status |
|---|---|---|
| `is_ascii_fast_path_eligible(&str) -> bool`* | — | shipped |
| `is_nfc(&str) -> bool`* | UAX #15 | shipped |
| `nfc_normalize(&str) -> String`* | UAX #15 | shipped |
| `is_nfd(&str) -> bool` | UAX #15 | **UNI-1** |
| `nfd_normalize(&str) -> String` | UAX #15 | **UNI-1** |
| `is_nfkc(&str) -> bool` | UAX #15 | **UNI-1** |
| `nfkc_normalize(&str) -> String` | UAX #15 | **UNI-1** |
| `is_nfkd(&str) -> bool` | UAX #15 | **UNI-1** |
| `nfkd_normalize(&str) -> String` | UAX #15 | **UNI-1** |
| `Form` enum (`NFC`, `NFD`, `NFKC`, `NFKD`) | UAX #15 | **UNI-1** |
| `normalize(Form, &str) -> String` | UAX #15 | **UNI-1** |
| `is_normalized(Form, &str) -> bool` | UAX #15 | **UNI-1** |
| `stream_is_nfc(impl Iterator<Item=char>) -> IsNormalized` | UAX #15 | **UNI-13** (LSP incremental support) |

### 4.4 `paideia_as_unicode::grapheme`

| Symbol | UAX | Status |
|---|---|---|
| `GraphemeBoundaries<'a>` struct* | UAX #29 | shipped |
| `grapheme_boundaries(&str) -> GraphemeBoundaries<'_>`* | UAX #29 | shipped |
| `grapheme_count(&str) -> usize`* | UAX #29 | shipped |
| `grapheme_advance(&str, usize) -> usize`* | UAX #29 | shipped |
| `grapheme_retreat(&str, usize) -> usize`* | UAX #29 | shipped |

### 4.5 `paideia_as_unicode::width`

| Symbol | UAX | Status |
|---|---|---|
| `Width` enum* (`Zero`, `Narrow`, `Ambiguous`, `Wide`) | UAX #11 | shipped |
| `Width::columns_latin(self) -> usize` | UAX #11 | shipped |
| `Width::columns_east_asian(self) -> usize` | UAX #11 | shipped |
| `width(char) -> Width`* | UAX #11 | shipped |
| `str_width(&str) -> usize`* | UAX #11 | shipped |
| `grapheme_width(&str) -> usize`* | UAX #11 | shipped |
| `WidthMode` enum (`Latin`, `EastAsian`, `Auto`) | UAX #11 | **UNI-9** |
| `resolve_width_mode(&WidthEnv) -> WidthMode` | UAX #11 | **UNI-9** |
| `str_width_in(WidthMode, &str) -> usize` | UAX #11 | **UNI-9** |

### 4.6 `paideia_as_unicode::case` (new module at UNI-2)

| Symbol | UAX | Status |
|---|---|---|
| `to_lower(&str) -> String` | UAX #44 case mappings | **UNI-2** |
| `to_upper(&str) -> String` | UAX #44 case mappings | **UNI-2** |
| `case_fold(&str) -> String` (default; non-Turkic) | UAX #44 §5 | **UNI-2** |
| `case_fold_turkic(&str) -> String` (Turkic I-dotless) | UAX #44 §5 | **UNI-2** |
| `eq_ignore_case_unicode(&str, &str) -> bool` | UAX #44 | **UNI-2** |

### 4.7 `paideia_as_unicode::segment` (new module at UNI-3)

| Symbol | UAX | Status |
|---|---|---|
| `LineBreak` enum (`Mandatory`, `Allowed`, `Prohibited`) | UAX #14 | **UNI-3** |
| `line_break_opportunities(&str) -> LineBreakIter<'_>` | UAX #14 | **UNI-3** |
| `wrap_at_width(&str, usize) -> Vec<&str>` | UAX #14 | **UNI-3** |
| `wrap_at_width_in(WidthMode, &str, usize) -> Vec<&str>` | UAX #11 + #14 | **UNI-3** |

### 4.8 `paideia_as_unicode::ident` (new module at UNI-4)

| Symbol | UAX | Status |
|---|---|---|
| `is_xid_start(char) -> bool` | UAX #31 | **UNI-4** |
| `is_xid_continue(char) -> bool` | UAX #31 | **UNI-4** |
| `is_valid_ident(&str) -> bool` | UAX #31 | **UNI-4** |
| `IdentProfile` enum (`Ascii`, `UnicodeDefault`, `ModeratelyRestricted`) | UAX #31 + UTS #39 | **UNI-4** |
| `check_ident(IdentProfile, &str) -> Result<(), IdentError>` | UTS #39 §5 | **UNI-4** |

### 4.9 `paideia_as_unicode::ucd` (new module at UNI-5)

| Symbol | UAX | Status |
|---|---|---|
| `general_category(char) -> GeneralCategory` | UAX #44 §5.7 | **UNI-5** |
| `GeneralCategory` enum (30 variants) | UAX #44 §5.7 | **UNI-5** |
| `script(char) -> Script` | UAX #24 | **UNI-5** |
| `Script` enum (~160 variants per UCD 15.1) | UAX #24 | **UNI-5** |
| `block(char) -> Block` | UAX #44 | **UNI-5** |
| `Block` enum (~320 variants per UCD 15.1) | UAX #44 | **UNI-5** |

### 4.10 `paideia_as_unicode::lsp_bridge` (new module at UNI-13)

Landed as `::lsp_bridge` rather than the §4-draft name `::lsp` to make
the module's role at the LSP/encoding boundary explicit (and to leave
`::lsp` free for a future higher-level LSP binding crate).

| Symbol | UAX | Status |
|---|---|---|
| `utf8_offset_to_utf16(&str, usize) -> usize` | LSP position encoding | shipped (UNI-13) |
| `utf16_offset_to_utf8(&str, usize) -> usize` | LSP position encoding | shipped (UNI-13) |
| `utf8_offset_to_utf32(&str, usize) -> usize` | LSP position encoding | shipped (UNI-13) |
| `utf32_offset_to_utf8(&str, usize) -> usize` | LSP position encoding | shipped (UNI-13) |
| `stream_is_nfc<I: Iterator<Item = char>>(I) -> bool` | UAX #15 | shipped (UNI-13) |

### 4.11 Starred-symbol freeze

**Freeze: landed at UNI-11 on 2026-10-11.** Every symbol marked with
`*` above is a crate-root re-export (`pub use` in `lib.rs`). The
UNI-11 freeze guarantees:

* Every shipped symbol in §§4.1–4.9 remains callable with its
  current signature at every post-UNI-11 release; `tests/public_api.golden`
  is the canonical byte-level snapshot and
  `tests/public_api_test.rs` the drift gate.
* Every post-freeze planned symbol (UNI-9 locale-resolver fns in §4.5
  marked `str_width_in`-style, UNI-13 LSP column converters in §4.10)
  is **additive-only** — a new symbol never displaces or renames one
  the freeze pins.
* The `nfc` module keeps `is_ascii_fast_path_eligible`, `is_nfc`, and
  `nfc_normalize` as re-exports from `::norm`. The deprecation marker
  tracking the `nfc → normalize` migration is a UNI-16 close item;
  the shim is **frozen** at UNI-11 and its removal is a UNI-16-plus
  minor-version bump.
* Variant additions on `#[non_exhaustive]` enums are a UNI follow-up
  ticket post-freeze. Consumers may still use exhaustive matches on
  the current variant set, but the enum shape is pinned at the
  freeze line.

---

## 5. Fingerprint discipline

### 5.1 What fingerprinting means in this crate

Per `feedback_workerbee_verify_claims.md`, every text-processing claim
needs to leave a byte-level audit trail the next session can rerun. The
UNI chain borrows the pattern paideia-fmt landed at FMT-6: for every
public text-transforming function, store a **golden file** of
`<input> ⇒ <output>` pairs at `tests/fingerprints/<fn>.golden`.

A conformance test reads the golden file, applies the function to every
input, and `assert_eq!`s against the expected output. On any mismatch,
the test fails; the fingerprint tag names the specific pair.

### 5.2 File layout

```
crates/paideia-as-unicode/tests/fingerprints/
  nfc_normalize.golden
  nfd_normalize.golden
  nfkc_normalize.golden
  nfkd_normalize.golden
  case_fold.golden
  case_fold_turkic.golden
  to_lower.golden
  to_upper.golden
  grapheme_boundaries.golden
  grapheme_count.golden
  line_break_opportunities.golden
  wrap_at_width.golden
  str_width.golden
  grapheme_width.golden
  width_char.golden
  check_ident_ascii.golden
  check_ident_default.golden
  check_ident_moderately_restricted.golden
  general_category.golden
  script.golden
  block.golden
  utf8_decoder.golden
  utf16_col_convert.golden
  public_api.golden   # UNI-11: the full API freeze listing
```

### 5.3 Golden-file format

Each file is UTF-8, LF line endings, one record per line, with records
separated into blocks by blank lines. The format inside a record:

```
TAG: <fingerprint-tag>
IN:  <hex-encoded input bytes>
OUT: <hex-encoded expected output bytes>
```

A tag is a short dot-separated string naming the invariant and a corpus
origin, e.g. `nfc.ucd.d800`, `width.cjk.han.4e2d`, `graph.emoji.family`.
Tags are stable across UCD upgrades — a tag names *what is being
tested*, not *what the current UCD table says*. Hex encoding (rather
than literal UTF-8) sidesteps editor-driven normalization: a git-diff
of a `.golden` file on a UCD upgrade is a pure hex diff and reviewers
can decode byte-exact changes.

A record whose `IN`/`OUT` columns span multiple lines uses a continuation
convention: `|<hex>` on the following lines is appended to the prior
column. The UNI-6 harness implements the parser and the generator.

### 5.4 Regeneration workflow

On a UCD upgrade (and only then):

```
cargo test -p paideia-as-unicode -- --regen-fingerprints
```

This flag — provided by UNI-6's test harness — writes every golden file
in place, including the fingerprint tag of every record. The upgrade PR
MUST contain the resulting diff. A reviewer checks each diffed line
against the UCD changelog: a record whose `OUT` changed implies a UCD
property-table rotation that must be present in Unicode's own
`UCDnnn.html` release notes, otherwise the change is a bug (either in
the upstream crate or in paideia-as-unicode's adapter).

### 5.5 Interaction with the UCD corpus sweep

UNI-7 runs the crate against the full UCD conformance test files:

* `NormalizationTest.txt` — ~17 000 records, four normalization forms each.
* `GraphemeBreakTest.txt` — ~670 records.
* `LineBreakTest.txt` — ~7 500 records.
* `WordBreakTest.txt` — ~1 950 records (consumed only if UAX#29 word
  boundaries ship as part of UAX#14 wrap infrastructure).
* `IdentifierStatus.txt` — ~140 000 code-point rows.

These corpora are large and vendored separately under
`tests/ucd-corpus/<version>/`. They do NOT live under `fingerprints/`:
their fail-mode is any mismatch, their records are not individually
tagged, and their regeneration path is "re-download from unicode.org
and bump the version directory". The fingerprint golden files
(§5.2) are a hand-curated subset chosen to catch specific regression
classes with small, reviewable diffs; the UCD corpus is the
exhaustive conformance guarantee.

### 5.6 Fingerprint-tag discipline (dispatch rule)

Every softarch dispatched for a UNI ticket that adds or changes a
text-processing function MUST:

1. Add a fingerprint tag string in the function's conformance test.
2. Add a corresponding record to the `<fn>.golden` file (or create the
   file).
3. Choose tags from a short vocabulary: `nfc.*`, `nfd.*`, `nfkc.*`,
   `nfkd.*`, `case.*`, `fold.*`, `graph.*`, `width.*`, `line.*`,
   `ident.*`, `profile.*`, `ucd.*`, `utf8.*`, `lsp.*`. Second segment
   names the input class (`ascii`, `latin1`, `cjk.han`, `emoji.family`,
   `zwj`, `cf`, `hex`). Third segment, when present, pins the specific
   code-point in hex.

Tags whose prefix the dispatch rule disallows fail CI (UNI-6 enforces
the lint in the harness binary).

---

## 6. Error taxonomy

### 6.1 Shipped: UTF-8 decoding

Defined in `src/error.rs` (see §4.1). Every variant carries a byte
offset relative to the current decoder input slice; streaming callers
accumulate a base offset externally.

### 6.2 Added at UNI-1: normalization

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizeError {
    pub offset: usize,          // byte offset of the first offending char
    pub kind: NormalizeErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizeErrorKind {
    /// Input contained an unassigned code point whose normalization is
    /// undefined under the pinned UCD version. Rare under UCD 15.1
    /// (every code point has a canonical class); surfaced for
    /// forward-compatibility with Private Use Area ambiguities.
    Unassigned,
    /// Decomposition yielded a scalar that is not a valid Unicode code
    /// point. Should never fire against RFC-3629-valid input; serves
    /// as a defensive assert for upstream crate bugs.
    DecompositionProducedInvalid,
}
```

The crate's `nfc_normalize` and the UNI-1 siblings return `String` (not
`Result`) under the current invariant that every well-formed `&str`
normalizes. The error struct is defined for the streaming helper
`stream_is_nfc` (UNI-13) and for a future `try_normalize` that may
expose the Private Use Area edge case.

### 6.3 Added at UNI-3: segmentation

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentError {
    pub offset: usize,
    pub kind: SegmentErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentErrorKind {
    /// `wrap_at_width` was asked to wrap at a width smaller than the
    /// smallest unbreakable cluster. Caller must widen or allow
    /// overrun.
    WidthTooSmall { minimum: usize },
}
```

Line-break iteration itself is total (every `&str` yields a possibly
empty sequence of opportunities); only `wrap_at_width` can fail.

### 6.4 Added at UNI-4: identifier validation

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentError {
    pub offset: usize,          // byte offset of the first offending char
    pub kind: IdentErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentErrorKind {
    /// First character fails UAX#31 `XID_Start`.
    InvalidStart,
    /// A non-first character fails UAX#31 `XID_Continue`.
    InvalidContinue,
    /// Identifier is empty.
    Empty,
    /// Character is not in the UTS #39 Moderately-Restricted allowed
    /// profile under the active `IdentProfile`.
    DisallowedByProfile,
    /// Identifier mixes scripts in a way UTS #39 §5 forbids.
    MixedScript,
}
```

### 6.5 Error-taxonomy invariants

* Every error struct exposes a byte `offset` referring to the input
  slice.
* Every error struct derives `Clone + Copy (where sized) + Debug +
  PartialEq + Eq`.
* Every error type implements `fmt::Display` with a stable
  one-line format (no trailing newline, no ANSI).
* Every error type implements `std::error::Error`.
* No error variant is removed post-UNI-11; new variants are additive and
  require a workspace minor bump (consumers may rely on exhaustive
  matching).

---

## 7. Consumer contract

### 7.1 Current consumers

| Consumer crate | Uses | Depends on symbols |
|---|---|---|
| `paideia-as-shell-lex` | UTF-8 decode of source bytes, NFC-normalize command names, grapheme-advance for cursor spans | `Utf8Decoder`, `Utf8Error`, `nfc_normalize`, `grapheme_boundaries`, `grapheme_count` |
| `paideia-as-shell-ast` | NFC-normalize record-field and command names, grapheme-count for diagnostic column arithmetic | `nfc_normalize`, `grapheme_count` |
| `paideia-as-shell-completion` | NFC-normalize candidate names, grapheme-count for display-column math, str_width for prefix layout | `nfc_normalize`, `grapheme_count`, `str_width`, `grapheme_width` |

The three current consumers all land their imports through the crate
root re-exports (`paideia_as_unicode::{Utf8Decoder, nfc_normalize,
grapheme_count, str_width, …}`). None reaches into a submodule
directly. The crate guarantees that pattern remains valid.

### 7.2 Planned consumer: LSP (UNI-13)

The LSP server (`paideia-as-lsp`, scheduled post-UNI chain) needs the
column converters §4.10 defines. The LSP consumes positions as
UTF-16 code units (default per the LSP spec); paideia-as internals are
UTF-8 byte offsets; both must be convertible without allocation.

The LSP will also need `stream_is_nfc` (UNI-13 §4.3) for its
incremental re-tokenization path: on each delta the server verifies
that the modified region's chars form NFC without re-materializing the
whole document.

### 7.3 Consumer-side discipline

Consumers MUST:

* Call `nfc_normalize` at every IPC boundary (SH-D9.4).
* Call `is_ascii_fast_path_eligible` before storing a normalized key, to
  avoid a redundant `.to_owned()` when the input was already ASCII.
* Use `grapheme_count` for user-facing character counts, NEVER
  `.chars().count()` (which counts code points and misreports every
  combining-mark cluster).
* Use `str_width` for cursor-column math, NEVER `grapheme_count`
  (which miscounts every CJK and emoji by 2x).
* Use the crate-root re-exports (§4.11), not submodule paths.

Consumers MUST NOT:

* Depend on `unicode-normalization`, `unicode-segmentation`,
  `unicode-width`, or `unicode-ident` directly. Every UCD-table-backed
  operation goes through this crate so the UCD pin (§3) applies
  uniformly.
* Match on `#[non_exhaustive]` enum variants without a `_` arm
  (the shipped enums are not currently `non_exhaustive`, but UNI-11
  may add the attribute; exhaustive matches that work now will keep
  working, and `_` arms future-proof).

---

## 8. UAX conformance claims

Per standard, this is the test file consumed and the harness location.
Every claim is auditable: a failing conformance run names the exact
record that fails.

| UAX / UTS | Conformance file | Harness | Status |
|---|---|---|---|
| **RFC 3629** | hand-rolled corpus of 100+ pathological byte sequences | `tests/utf8_decoder.rs` (shipped) + UNI-7 extension to Markus Kuhn's `UTF-8-test.txt` | partial |
| **UAX #11** | `EastAsianWidth.txt` property enumeration | `tests/width_conformance.rs` (shipped) + UNI-7 full-table sweep | partial |
| **UAX #14** | `LineBreakTest.txt` (~7 500 records) | `tests/line_break_conformance.rs` (UNI-3) + UNI-7 sweep | **UNI-3 / UNI-7** |
| **UAX #15** | `NormalizationTest.txt` (~17 000 records × 4 forms) | `tests/nfc_conformance.rs` (shipped, NFC only) + UNI-1 (NFD/NFKC/NFKD) + UNI-7 (full sweep) | partial |
| **UAX #29** (grapheme) | `GraphemeBreakTest.txt` (~670 records) | `tests/grapheme_conformance.rs` (shipped) + UNI-7 sweep | partial |
| **UAX #29** (word) | `WordBreakTest.txt` (~1 950 records) | UNI-7 sweep only (word iterator not currently exported) | **UNI-7** |
| **UAX #31** | hand-rolled XID_Start/XID_Continue probe set | UNI-4 + UNI-7 (`IdentifierStatus.txt`) | **UNI-4 / UNI-7** |
| **UTS #39** | `IdentifierStatus.txt` (~140 000 rows) + `confusables.txt` (deferred) | UNI-4 (status) + UNI-7 (full sweep); confusables out of scope | **UNI-4 / UNI-7** |

Corpora are vendored at `tests/ucd-corpus/15.1.0/` and tagged in git.
The harness refuses to run against a corpus version that disagrees with
the §3.1 pin.

---

## 9. Benchmarking posture

Benchmarks land at UNI-8 under `benches/` using
[`criterion`](https://crates.io/crates/criterion). Each benchmark emits
a throughput number (ns/call for scalar operations, MB/s for stream
operations) and a p99 tail latency.

### 9.1 Target thresholds

| Function | Input class | Target | Rationale |
|---|---|---|---|
| `nfc_normalize` | ASCII (fast path) | **≤100 ns/call** at 32-byte input | Lands in R222.M5 ≤50 µs command-lookup budget. |
| `nfc_normalize` | Latin-1 (quick-check pass) | **≤500 ns/call** at 32-byte input | Common Latin command name. |
| `nfc_normalize` | NFC-worst-case (combining marks) | **≤1 µs/char** at 32-char input | Loose but observed-achievable with `unicode-normalization`. |
| `nfd_normalize` / `nfkc_normalize` / `nfkd_normalize` | NFC-worst-case | **≤2 µs/char** | Decomposition is 2× composition cost. |
| `is_nfc` | ASCII | **≤20 ns/call** at 32-byte input | Byte-scan only, no table hits. |
| `grapheme_count` | ASCII | **≤2 ns/byte** | Table-skip inner loop. |
| `grapheme_count` | CJK | **≤10 ns/byte** | Per-char table hit, cache-warm. |
| `str_width` | ASCII (no escapes) | **≤2 ns/byte** | Byte-scan + grapheme-count path. |
| `str_width` | CJK | **≤15 ns/byte** | Per-cluster width lookup. |
| `Utf8Decoder::next` | ASCII | **≤3 ns/call** | Byte-dispatch, no table. |
| `Utf8Decoder::next` | 4-byte emoji | **≤8 ns/call** | Four-byte decode + validate. |

Thresholds are reported, not enforced, at UNI-8. A regression gate
(fail-build on throughput drop >20%) is deferred to a future hardening
ticket.

### 9.2 Measurement discipline

* Benchmarks run on the `main` branch's HEAD at every UNI-N landing to
  catch regressions early.
* Inputs are seeded (not timestamp-derived) so cross-session comparison
  is apples-to-apples.
* Reports land at `.plans/scratch/bench-UNI-N.md` per wave; UNI-16
  consolidates into the acceptance report.
* Comparisons across kernels / CPU families are documented, not
  normalized away. A number is only comparable to another number taken
  on the same silicon.

---

## 10. Open questions

Questions this spec deliberately leaves open; each will be resolved at
the UNI-N wave noted.

### 10.1 `WidthMode` locale resolver policy

**Resolver signature.** The `resolve_width_mode(&WidthEnv) -> WidthMode`
helper (UNI-9 §4.5) picks `Latin`, `EastAsian`, or `Auto` from
environment inputs. Open: which inputs, in which priority order?

**Candidates:**

* **A.** `WCWIDTH_MODE` env var (`latin` / `east-asian` / `auto`),
  matching `bat` / `hyperfine` convention. Simple, explicit, 1 bit of
  configuration.
* **B.** `LC_CTYPE` glibc locale (e.g. `ja_JP.UTF-8`), matching `wcwidth(3)`.
  Legacy POSIX convention; broad OS coverage; subject to glibc version
  drift.
* **C.** Terminal query (CSI DECSET / cursor-position probe). Robust
  but blocks the renderer on an IO roundtrip at startup.

**Preliminary preference:** **A then B then default-Latin**, with a
`WidthMode::Auto` runtime variant that re-probes on terminal resize.
UNI-9 picks and documents the exact precedence.

### 10.2 Case-folding Turkic flag default

**Question.** When a caller calls `case_fold` without specifying the
Turkic variant, which casing applies to `I` (U+0049) vs `İ` (U+0130)?

**Candidates:**

* **A.** Default is non-Turkic (`case_fold` for Turkic callers requires
  `case_fold_turkic`). Matches ICU / Java `toLowerCase(Locale.ROOT)`.
* **B.** Default resolves via the same `LC_CTYPE` resolver `WidthMode`
  uses; Turkic auto-detects.

**Preliminary preference:** **A.** The shell's command-name index is
locale-free; mixing in Turkic flipping would make `case_fold` nondeterministic
across installations. Shell users who need Turkic folding call
`case_fold_turkic` explicitly. UNI-2 confirms.

### 10.3 UCD data embedding posture

**Question.** The upstream crates compile the UCD tables into static
RODATA (~300 KiB in aggregate). Should paideia-as-unicode:

* **A.** Keep the current compile-in posture. Zero IO at startup;
  larger binary.
* **B.** Lazy-load tables from a `.pds`-adjacent `ucd.bin` on first
  use. Smaller base binary; adds a `KIND_UCD` loader concern to the
  kernel.
* **C.** Compile-in for the shell; load-from-disk for the LSP server
  (whose cold-start latency matters more than binary size).

**Preliminary preference:** **A** uniformly. 300 KiB RODATA is
negligible against the shell's working set; load-from-disk would
require a new file-format and bootstrap concern. UNI-5 confirms and the
decision is pinned by UNI-11's API freeze.

### 10.4 `#[non_exhaustive]` on error enums

**Question.** Should `Utf8ErrorKind`, `NormalizeErrorKind`,
`SegmentErrorKind`, `IdentErrorKind` be `#[non_exhaustive]` at the
UNI-11 freeze?

**Trade-off.** `#[non_exhaustive]` lets us add a variant without a
breaking change, but forces every consumer to add a `_ =>` arm.
Current consumers all exhaustive-match.

**Preliminary preference:** **Not non-exhaustive.** We commit to no new
variants post-freeze without a workspace minor bump. New variants are
rare (none expected) and the exhaustive-match discipline is more
important than the escape hatch. UNI-11 confirms.

### 10.5 Spec vs implementation drift detection

**Question.** How does CI catch when §4's inventory drifts from the
actual crate surface?

**Preliminary preference:** The UNI-11 `public_api.golden` file
(§5.2) is diffed against a runtime enumeration of
`paideia_as_unicode`'s public symbols on every build. A mismatch fails
the build with a diff showing which symbol is undeclared or
unimplemented. UNI-11 implements. This spec's §4 is cross-linked from
`public_api.golden` so a reviewer updating one updates the other.

---

## 11. Cross-references

* **Dispatch map.** `.plans/scratch/UNICODE-dispatch-map.md` — the
  wave-grouping, dependency DAG, and encoder-pitfall checklist for
  every UNI-N dispatch.
* **Shell contract.** `design/terminal/semantic-shell.md` §9 (SH-D9
  — unicode handling) §10.4 (NFC at IPC boundaries).
* **FMT precedent.** `design/toolchain/paideia-fmt-style-spec.md` (the
  specification analogue), `design/toolchain/paideia-fmt-acceptance.md`
  (the acceptance report analogue). UNI-16 will produce
  `design/toolchain/paideia-as-unicode-acceptance.md`.
* **Reserved-word policy.** `design/toolchain/reserved-word-policy.md`
  (UAX #31 identifier predicates must align; UNI-4 verifies).
* **Loop shape.** `feedback_paideia_as_loop_shape.md` — the
  softarch → main-build → debugger-every-iteration shape every UNI-N
  wave follows.
* **CHANGELOG scratch pattern.** `feedback_paideia_as_parallel_changelog.md`
  — each UNI-N wave writes `.plans/scratch/CHANGELOG-UNI-N-<slug>.md`;
  main consolidates at UNI-16.
* **Closing-keyword discipline.** `feedback_github_closing_keywords.md`,
  `feedback_github_closes_keyword_syntax.md` — every UNI-N commit uses
  `Closes paideia-os/paideia-as#NNNN.` (or bare `#NNNN` same-repo),
  one keyword per issue.
* **Fingerprint rationale.** `feedback_workerbee_verify_claims.md` —
  the "byte-level audit trail" discipline this crate inherits.
* **Encoder pitfalls.** `feedback_pdx_encoder_pitfalls.md` — operative
  only for UNI-12 (which may touch `.pdx` fixtures in shell-lex /
  shell-ast / shell-completion).

---

## 12. Change log

| Date | Author | Change |
|---|---|---|
| 2026-10-11 | UNI-0 softarch | Document authored (status AUTHORED). Enumerates UNI-1..UNI-16 scope. |
| 2026-10-11 | UNI-11 softarch | §4 FROZEN at the inventory mirrored in `tests/public_api.golden`. §1 status table updated. |
| 2026-10-11 | UNI-16 softarch | **Status: COMPLETE** at paideia-as v0.37.0. §1 Status row updated; §Status-change block added. Acceptance report at `paideia-as-unicode-acceptance.md`. Every section bar §12 locked. |

Subsequent UNI-N waves append a row with (date, ticket, change) when
they land a symbol or alter a claim. UNI-11 adds a `status: FROZEN`
row and locks §4. UNI-16 adds a `status: COMPLETE` row and locks every
other section bar §12 itself.
