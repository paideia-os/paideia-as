# paideia-as#1600 — UNI-13: LSP position-encoding bridge

**Scope:** add the five converters paideia-lsp needs to speak any
`PositionEncodingKind` on the wire — four UTF-8 ↔ UTF-16 / UTF-32
column converters plus a streaming NFC predicate — then wire the
paideia-lsp Backend's position-to-offset / offset-to-position choke
points through them so the server defaults to the LSP 3.17 UTF-16
convention end-to-end.

## Added

- `crates/paideia-as-unicode/src/lsp_bridge.rs` (**new**, ~155 LOC)
  - `utf8_offset_to_utf16(s, i) -> usize`
  - `utf16_offset_to_utf8(s, i) -> usize`
  - `utf8_offset_to_utf32(s, i) -> usize`
  - `utf32_offset_to_utf8(s, i) -> usize`
  - `stream_is_nfc<I: Iterator<Item = char>>(iter) -> bool`
  - Mid-char rounding: offsets that fall strictly inside a code-unit
    sequence (multi-byte UTF-8, surrogate pair) round DOWN to the
    preceding char boundary per LSP 3.17 §3.14.
  - ASCII fast path: `s.is_ascii()` short-circuits to
    `offset.min(s.len())` on every encoding-pair converter, so the
    paideia-lsp hot path (ASCII identifiers) pays zero Unicode-table
    cost post-UNI-13.

- `crates/paideia-as-unicode/tests/lsp_bridge_test.rs` (**new**, 18
  fingerprinted `uni13-lsp-NN` tests): ASCII round-trip, BMP non-ASCII
  (é), astral emoji (😀) with mid-surrogate round-down, inverse
  symmetry across BMP + astral, UTF-32 codepoint count, boundary cases
  (offset 0 / offset == s.len() / mid-char round-down), four
  `stream_is_nfc` cases (hello, decomposed cafe, empty iterator,
  precomposed cafe) plus a parity test asserting
  `stream_is_nfc(s.chars()) == is_nfc(s)` across the full
  cross-section.

- `crates/paideia-lsp/tests/position_encoding.rs` (**new**, 5 fingerprinted
  `uni13-lsp-pos-NN` tests exercising the LSP Backend's
  `position_to_offset` end-to-end): ASCII single-line, BMP non-ASCII
  after é, astral emoji after + mid surrogate, multi-line with é on
  line 1, character past line end clamped to line.len() (pre-UNI-13
  silently walked into the next line).

## Changed

- `crates/paideia-as-unicode/src/lib.rs`
  - New `pub mod lsp_bridge;`
  - Re-exports: `utf8_offset_to_utf16`, `utf16_offset_to_utf8`,
    `utf8_offset_to_utf32`, `utf32_offset_to_utf8`, `stream_is_nfc`.

- `crates/paideia-lsp/Cargo.toml`
  - New `paideia-as-unicode = { path = "../paideia-as-unicode" }` dep.
    No workspace-level dep change (paideia-as-unicode is already a
    workspace member).

- `crates/paideia-lsp/src/document.rs`
  - `position_to_offset`: UTF-16 column → UTF-8 byte offset within the
    resolved line now delegates to
    `paideia_as_unicode::utf16_offset_to_utf8`. The intra-line slice
    trims at the next `'\n'` so a `character` past the line end
    clamps to the line boundary instead of walking into the following
    line (silent bug pre-UNI-13).
  - Pre-UNI-13 behaviour: `position.character` was interpreted as a
    codepoint count — correct for ASCII, off-by-one for every
    multi-byte line. All existing tests are ASCII, so they are
    byte-for-byte unchanged.

- `crates/paideia-lsp/src/navigation.rs`
  - `offset_to_position`: UTF-8 byte offset → UTF-16 code-unit
    `character` emission now delegates to
    `paideia_as_unicode::utf8_offset_to_utf16` over the intra-line
    slice. The forward walk now also rounds a mid-char `byte_offset`
    DOWN to the preceding char boundary so the trailing slice never
    panics on a non-boundary input.

## Semantics

- `stream_is_nfc(s.chars()) == is_nfc(s)` for every `s: &str`. The
  streaming form takes any `Iterator<Item = char>` so it can validate
  `didChange` edits before an owning `String` is formed. `IsNormalized::Maybe`
  is treated conservatively as "not NFC", matching the existing
  `is_nfc` quick-check policy.

- Inverse symmetry at char boundaries:
  `utf16_offset_to_utf8(s, utf8_offset_to_utf16(s, i)) == i`
  for every `i` that is itself a UTF-8 char boundary. For a mid-char
  `i` the composition returns the preceding char boundary.

## Not run

Build + test not invoked (sub-agent policy — builds are main-only).
Main should run `bash tools/build.sh` and the paideia-as-unicode /
paideia-lsp test suites, and re-invoke this agent with the error
tail on failure.

Closes paideia-as#1600.
