# paideia-as#1562 (FMT-1a) — Lexer-side trivia harvest wrapper

**Wave 61 (paideia-fmt completion sprint), 2026-10-10.**

## Summary

Lands the lexer-side half of the formatter's comment-carriage chain.
Adds `paideia_fmt::cst`, a thin wrapper over `paideia_as_lexer::Lexer`
that drives `next_token` + `take_trivia` in lockstep and packages the
result as a flat `TriviaStream { tokens, leading_trivia,
trailing_trivia }`. No lexer behavior changes — byte spans and
diagnostics route through unchanged.

Chain head for FMT-1b (parser-side attachment), FMT-3 (walker
fallback), and FMT-9 (doc-comment attachment).

## Files touched

* `crates/paideia-fmt/src/cst.rs` (new, 274 LOC incl. tests)
    * `pub struct TriviaStream` with the exact fields the issue
      specifies: `tokens: Vec<Token>`, `leading_trivia:
      Vec<Vec<Trivia>>`, `trailing_trivia: Vec<Trivia>`. `Clone`,
      `Debug`, `Default`.
    * Convenience accessors `is_empty` and `len`.
    * `pub fn tokenize_with_trivia(file, source, sink) ->
      TriviaStream` — loops `Lexer::next_token` + `Lexer::take_trivia`,
      pushes real tokens with their leading trivia, and on
      `TokenKind::Eof` returns, routing EOF's leading trivia into
      `trailing_trivia`.
    * Non-exhaustive-match discipline for `TokenKind` (sibling
      crate's enum is `#[non_exhaustive]`): only `Eof` terminates;
      everything else — current or future — is a real token.
    * Full module and item doc comments covering the EOF-trivia
      decision, invariants, and panic-free contract.
    * Six `#[cfg(test)]` tests:
        1. `zero_comment_source_has_no_comment_trivia`
        2. `line_comment_attaches_as_leading`
        3. `nested_block_comment_surfaces_as_single_trivia`
        4. `blank_line_runs_preserved`
        5. `trailing_trivia_captured_at_eof`
        6. `trivia_only_file_has_zero_tokens`
        7. `shebang_prefix_is_not_recognized_as_trivia` (documents
           lexer gap for FMT-9's attention — see below).

* `crates/paideia-fmt/src/lib.rs`
    * `pub mod cst;` wire-in; no other changes.

* `crates/paideia-fmt/Cargo.toml`
    * Added path deps `paideia-as-lexer` and
      `paideia-as-diagnostics` (both previously absent — the crate
      only exposed string-rewriting glyph helpers).

* `.plans/scratch/CHANGELOG-1562-fmt-trivia-stream.md` — this file.

## EOF trailing-trivia decision

`TokenKind::Eof` is **not** materialized in `tokens`. Trivia that
would have been EOF's leading trivia lands in `trailing_trivia`
instead. Rationale:

1. Formatters need to preserve end-of-file comments and the empty
   bucket "the EOF token itself has no lexeme" would otherwise
   require special-casing at every consumer.
2. Keeps `tokens.len() == leading_trivia.len()` as a clean, always-
   true invariant.
3. Natural model — "all real tokens, plus whatever runs after the
   last one".

The decision is documented in both the module header and the
`tokenize_with_trivia` doc comment under an "EOF discipline" heading.

## Lexer API gap discovered: shebang handling

`#!/usr/bin/env paideia-as` prefixes are not recognised by
`paideia_as_lexer::scan_comment` (which matches `//` and `/*` only).
A leading `#` falls through to the operator / unrecognized-byte path
and emits `E0011`. FMT-9 (doc-comment attachment) will need to
either (a) extend the lexer with a shebang-aware trivia class or
(b) handle `#!` at the formatter's input layer before invoking
`tokenize_with_trivia`. Flagged in `cst::tests` via
`shebang_prefix_is_not_recognized_as_trivia`.

No other `Lexer::take_trivia` surface issues — the API is sufficient
for this wrapper as-is.

## Build

Not run per sub-agent discipline. Main is expected to invoke
`bash tools/build.sh` and re-dispatch with the error tail on failure.
