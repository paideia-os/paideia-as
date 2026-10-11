# CHANGELOG scratch — UNI-3 (paideia-as#1590)

## UAX #14 Line Breaking + `wrap_at_width`

**Target:** `crates/paideia-as-unicode/src/segment.rs` (new, ~440 LOC) +
tests + workspace Cargo.toml + crate Cargo.toml.

**Issue:** paideia-as#1590.
**Wave:** UNI-W2.
**Shape:** softarch (implement, no build) — main will build after
batching UNI-W2's three waves per the parallel-dispatch cadence.

### What landed

- `paideia_as_unicode::segment` module (new) owns the UAX #14 surface
  the semantic-shell R229 renderer consumes for soft-wrap layout. It
  composes with UNI-W1's `str_width` + grapheme-cluster segmentation —
  does not re-implement UAX #11 column math.
- New types:
  - `LineBreak` (`Mandatory`, `Allowed`, `Prohibited`) — `#[non_exhaustive]`,
    `#[derive(Default)]`, `#[default] Allowed`. `Prohibited` is reserved
    for a future per-byte query API (not emitted today).
  - `LineBreakOpportunity { byte_offset: usize, kind: LineBreak }` —
    `#[non_exhaustive]`. Offsets follow `unicode-linebreak`'s native
    convention (byte index of character following the break = start of
    next line were a break taken here); the EOS sentinel always sits
    at `byte_offset == s.len()` with `kind == Mandatory`.
  - `SegmentError { offset, kind }` + `SegmentErrorKind { WidthTooSmall { minimum } }` —
    both `#[non_exhaustive]`; `Display` + `std::error::Error` per the
    UNI-0 §6.5 error-taxonomy invariants. Reserved for a future
    `try_wrap_at_width` strict variant; `wrap_at_width` is total today
    via a grapheme-level forced-break fallback.
- New functions:
  - `line_break_opportunities(&str) -> Vec<LineBreakOpportunity>` —
    enumerate every UAX #14 opportunity in the input.
  - `next_line_break(&str, from: usize) -> Option<(usize, LineBreak)>` —
    streaming helper for incremental layout; returns the first
    opportunity at offset `≥ from`.
  - `wrap_at_width(&str, max_width) -> Vec<&str>` — greedy word-wrap
    respecting mandatory breaks, taking Allowed breaks greedily to fit
    `max_width` columns (UAX #11 via `str_width`), trimming trailing
    whitespace at wrap boundaries, and falling back to a grapheme-level
    forced break when a single unbreakable run exceeds `max_width`
    (the "aaaa" ASCII-fast-path case).
- `src/lib.rs` re-exports the six public names at the crate root.

### Dependency

- `unicode-linebreak = "0.1"` (Axel Forsman, Apache-2.0; UCD-table-backed
  UAX #14 r51 state machine; same UCD-vendor family as the three
  `unicode-*` crates already pinned). Added at the workspace level and
  in the `paideia-as-unicode` crate manifest. Pinned to the stable
  0.1.x line per the dispatch-map §5 "pins at the stable 0.1.x" rule.

### Tests

22 fingerprinted `uni3-seg-NN` tests at
`crates/paideia-as-unicode/tests/segment_test.rs`:

| Tag | Scope |
|---|---|
| uni3-seg-01 | LF mid-string yields Mandatory @ byte 4 for "foo\\nbar" |
| uni3-seg-02 | CRLF collapses to a single Mandatory @ byte 5 for "foo\\r\\nbar" |
| uni3-seg-03 | URL has no mid-string Mandatory |
| uni3-seg-04 | Space yields Allowed @ byte 6 for "hello world" |
| uni3-seg-05 | CJK ideographs allow break between every pair |
| uni3-seg-06 | Empty input yields empty Vec |
| uni3-seg-07 | `wrap_at_width("hello world", 5)` → `["hello", "world"]` |
| uni3-seg-08 | Greedy fit: 3-word wrap at width 10 |
| uni3-seg-09 | Mandatory break honored even when max_width would fit |
| uni3-seg-10 | CJK wrap at ideograph-width-2 |
| uni3-seg-11 | ASCII fast path character chunk ("aaaa", 2) |
| uni3-seg-12 | Empty-input wrap → empty Vec |
| uni3-seg-13 | Whole input fits in a single line |
| uni3-seg-14 | `max_width == 0` coerced to 1 |
| uni3-seg-15 | Opportunity offsets strictly ascending; EOS sentinel at s.len() |
| uni3-seg-16 | Wrap idempotent under join + rewrap |
| uni3-seg-17 | `next_line_break("ab cd ef", 0)` → first Allowed @ byte 3 |
| uni3-seg-18 | `next_line_break` advances past a prior opportunity |
| uni3-seg-19 | `next_line_break` returns EOS Mandatory from near-end; None past end |
| uni3-seg-20 | Streaming empty-input yields None |
| uni3-seg-21 | `LineBreak::default() == Allowed` (verifies `#[default]`) |
| uni3-seg-22 | Every wrapped line has `str_width(line) ≤ max_width` |

### Offset convention note

The task-dispatch examples for `next_line_break("ab cd ef", 0)`
suggested `(2, Allowed)` and `(5, Allowed)`. These are inconsistent
with the `"hello world"` → `(6, Allowed)` convention listed in the
same dispatch (byte 6 = position of `'w'`, i.e. the position *after*
the break). The `unicode-linebreak` crate uses the latter convention
natively (`"the byte position after a break opportunity"`), and the
module docs + tests here follow it: for `"ab cd ef"` the first Allowed
is at byte 3 (position of `'c'`). The behavioral contract is
unchanged — the mapping between the two conventions is `native - 1`
on single-space breaks — and the test values are self-consistent with
the `"hello world"` example.

### Encoder-pitfall discipline

- `#[forbid(unsafe_code)]` honored (no `unsafe` added).
- `#[warn(missing_docs)]` honored (every `pub` item has `///` docs).
- `#[non_exhaustive]` on both enums + both structs the task names.
- `str_width` + `grapheme_width` consumed from UNI-W1's `width`
  module; no re-implementation of UAX #11.
- Fingerprint tags all `uni3-seg-NN` per the dispatch-rule vocabulary.

### Closing keyword

Closes paideia-as#1590.
