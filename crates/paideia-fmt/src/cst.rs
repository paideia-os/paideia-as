//! Trivia-carrying token stream.
//!
//! The paideia-as lexer accumulates whitespace and comments as
//! [`Trivia`] between tokens; downstream clients retrieve the run via
//! [`Lexer::take_trivia`] after each [`Lexer::next_token`]. This module
//! packages that discipline into a single pass that yields a flat
//! stream of real tokens along with each token's leading trivia and a
//! single trailing-trivia bucket for whatever sits between the final
//! real token and EOF.
//!
//! The abstraction is a loss-less, lexer-faithful wrapper — byte spans
//! originate from the lexer unchanged, no normalization is applied,
//! and diagnostics are forwarded to the caller's sink verbatim. It is
//! the ground truth that the FMT-1b (parser-side attachment),
//! FMT-3 (walker fallback), and FMT-9 (doc-comment attachment) sprints
//! will consume.
//!
//! # EOF discipline
//!
//! The [`TokenKind::Eof`] sentinel is **not** emitted into
//! [`TriviaStream::tokens`]. Trivia that would otherwise have been the
//! leading trivia of the EOF token (e.g., a trailing `// end of file`
//! comment, blank lines after the last declaration) is instead stored
//! in [`TriviaStream::trailing_trivia`]. This matches the natural
//! formatter mental model — "the tokens, plus whatever runs after the
//! last token" — and avoids a dangling empty-span sentinel in
//! `tokens[last]`. See [`tokenize_with_trivia`] for details.
//!
//! # Invariants
//!
//! * `tokens.len() == leading_trivia.len()` always.
//! * `leading_trivia[i]` is exactly the trivia the lexer surfaced
//!   immediately before `tokens[i]`; no splitting or re-attachment is
//!   performed at this layer.
//! * `trailing_trivia` captures the trivia preceding EOF; if the
//!   source has zero real tokens (empty or trivia-only file) it holds
//!   the entire file's trivia.
//! * All trivia spans and token spans are copies of what the lexer
//!   produced — no re-spanning.

use paideia_as_diagnostics::{DiagnosticSink, FileId};
use paideia_as_lexer::{Lexer, SourceText, Token, TokenKind, Trivia};

/// A flat stream of real tokens with per-token leading trivia and a
/// single trailing bucket for post-last-token / pre-EOF trivia.
///
/// See the module docs for the full contract; the short version is
/// `tokens[i]` is preceded by `leading_trivia[i]`, and
/// `trailing_trivia` holds whatever came after the last real token.
#[derive(Clone, Debug, Default)]
pub struct TriviaStream {
    /// Real (non-EOF) tokens, in source order.
    pub tokens: Vec<Token>,
    /// Trivia preceding each token in [`Self::tokens`]; same length as
    /// `tokens`. Index-aligned with `tokens`: `leading_trivia[i]`
    /// precedes `tokens[i]`.
    pub leading_trivia: Vec<Vec<Trivia>>,
    /// Trivia between the last real token and EOF. Empty when the
    /// source ends immediately after a real token.
    pub trailing_trivia: Vec<Trivia>,
}

impl TriviaStream {
    /// Returns `true` iff the stream has no real tokens and no
    /// trailing trivia (i.e., the input had no lexable content at
    /// all, which can happen for an empty file surviving validation).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.trailing_trivia.is_empty()
    }

    /// Number of real tokens held by the stream (excludes the EOF
    /// sentinel, which this abstraction never materializes).
    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }
}

/// Drive the paideia-as lexer across `source` and package the result
/// into a [`TriviaStream`].
///
/// The function wraps [`Lexer::new`] + [`Lexer::next_token`] in a
/// loop and, after each `next_token`, calls [`Lexer::take_trivia`] to
/// capture that token's leading trivia. The EOF sentinel is not
/// pushed into [`TriviaStream::tokens`]; its leading trivia becomes
/// [`TriviaStream::trailing_trivia`] (see the module-level "EOF
/// discipline" note).
///
/// Diagnostics are forwarded to `sink` as the lexer emits them — if
/// the sink's bail policy fires, the lexer short-circuits to EOF and
/// this function returns the stream collected so far, honoring the
/// lexer's "stop emitting diagnostics after overflow" contract
/// without discarding already-collected tokens.
///
/// # Arguments
///
/// * `file`   — the [`FileId`] stamped into each emitted span.
/// * `source` — a validated [`SourceText`] (BOM-stripped, UTF-8).
/// * `sink`   — diagnostic sink; errors from the lexer land here.
///
/// # Panics
///
/// Never. All error paths route through `sink`.
pub fn tokenize_with_trivia(
    file: FileId,
    source: &SourceText,
    sink: &mut dyn DiagnosticSink,
) -> TriviaStream {
    let mut lexer = Lexer::new(file, source);
    let mut tokens: Vec<Token> = Vec::new();
    let mut leading_trivia: Vec<Vec<Trivia>> = Vec::new();

    loop {
        let tok = lexer.next_token(sink);
        // take_trivia() returns the trivia accumulated BEFORE the
        // just-returned token (whitespace/comments the lexer consumed
        // on its way to the next non-trivia byte). For a non-EOF
        // token this is leading trivia; for the EOF sentinel it is
        // the file's trailing trivia.
        let preceding = lexer.take_trivia();

        // Non-exhaustive guard: `TokenKind` is marked
        // `#[non_exhaustive]` by paideia-as-lexer. Only `Eof`
        // terminates the loop; every other variant — present or
        // future — is treated as a real token.
        if matches!(tok.kind, TokenKind::Eof) {
            return TriviaStream {
                tokens,
                leading_trivia,
                trailing_trivia: preceding,
            };
        }

        tokens.push(tok);
        leading_trivia.push(preceding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::VecSink;
    use paideia_as_lexer::TriviaKind;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    fn run(src: &str) -> (TriviaStream, VecSink) {
        let st = SourceText::from_bytes(file_id(), src.as_bytes())
            .expect("test source should validate as UTF-8");
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &st, &mut sink);
        (stream, sink)
    }

    /// Zero-trivia source: `let x` has no comments or inter-token
    /// whitespace runs beyond the single ascii space, which the
    /// lexer still surfaces as a whitespace trivia between `let`
    /// and `x`. We assert the invariant that `leading_trivia` is
    /// index-aligned and that no comment trivia appears.
    #[test]
    fn zero_comment_source_has_no_comment_trivia() {
        let (stream, sink) = run("let x");
        assert_eq!(sink.error_count(), 0);
        assert_eq!(stream.tokens.len(), 2);
        assert_eq!(stream.leading_trivia.len(), stream.tokens.len());
        assert_eq!(stream.tokens[0].kind, TokenKind::KwLet);
        assert_eq!(stream.tokens[1].kind, TokenKind::Ident);
        // `let` has no preceding trivia; `x` has one whitespace run.
        assert!(stream.leading_trivia[0].is_empty());
        assert!(
            stream
                .leading_trivia
                .iter()
                .flatten()
                .all(|t| !matches!(
                    t.kind,
                    TriviaKind::LineComment
                        | TriviaKind::BlockComment
                        | TriviaKind::DocLineComment
                        | TriviaKind::DocBlockComment
                ))
        );
        // Trailing trivia is empty — source ends right after `x`.
        assert!(stream.trailing_trivia.is_empty());
    }

    /// Single `// line` comment preceding the first token is
    /// attached as leading trivia of that token.
    #[test]
    fn line_comment_attaches_as_leading() {
        let (stream, sink) = run("// hello\nlet x");
        assert_eq!(sink.error_count(), 0);
        assert_eq!(stream.tokens[0].kind, TokenKind::KwLet);
        let leading = &stream.leading_trivia[0];
        assert!(
            leading.iter().any(|t| t.kind == TriviaKind::LineComment),
            "expected line comment in leading trivia of first token; got {:?}",
            leading
        );
    }

    /// The lexer explicitly supports nested `/* … /* … */ … */`
    /// per scan_comment tests; the stream wrapper must surface the
    /// outer block comment as a single `BlockComment` trivia.
    #[test]
    fn nested_block_comment_surfaces_as_single_trivia() {
        let (stream, sink) = run("/* outer /* inner */ outer */ let x");
        assert_eq!(sink.error_count(), 0);
        let leading = &stream.leading_trivia[0];
        let block_count = leading
            .iter()
            .filter(|t| t.kind == TriviaKind::BlockComment)
            .count();
        assert_eq!(
            block_count, 1,
            "nested comment should be ONE block-comment trivia, not multiple"
        );
    }

    /// Blank-line runs preserve one `Newline` trivia per LF and
    /// (where applicable) whitespace runs between them. We assert
    /// the Newline count rather than exact ordering because the
    /// lexer may interleave `Whitespace` runs.
    #[test]
    fn blank_line_runs_preserved() {
        let (stream, sink) = run("let\n\n\nx");
        assert_eq!(sink.error_count(), 0);
        assert_eq!(stream.tokens.len(), 2);
        // Three newlines sit between `let` and `x`.
        let newlines = stream.leading_trivia[1]
            .iter()
            .filter(|t| t.kind == TriviaKind::Newline)
            .count();
        assert_eq!(newlines, 3);
    }

    /// Trivia at EOF — comments and whitespace after the last real
    /// token — lands in `trailing_trivia`, not in a dangling EOF
    /// token (which this stream never materializes).
    #[test]
    fn trailing_trivia_captured_at_eof() {
        let (stream, sink) = run("let x // trailing\n");
        assert_eq!(sink.error_count(), 0);
        assert_eq!(stream.tokens.len(), 2);
        // The trailing `// trailing` + newline must sit in
        // trailing_trivia, NOT in any token's leading_trivia.
        assert!(
            stream
                .trailing_trivia
                .iter()
                .any(|t| t.kind == TriviaKind::LineComment),
            "expected trailing line comment in trailing_trivia; got {:?}",
            stream.trailing_trivia
        );
    }

    /// A trivia-only file (whitespace + comment, no real tokens)
    /// yields an empty `tokens` and a populated `trailing_trivia`.
    /// Guards the "all trivia routes to trailing when there are
    /// zero tokens" edge case.
    #[test]
    fn trivia_only_file_has_zero_tokens() {
        let (stream, sink) = run("// nothing but a comment\n");
        assert_eq!(sink.error_count(), 0);
        assert!(stream.tokens.is_empty());
        assert!(stream.leading_trivia.is_empty());
        assert!(
            stream
                .trailing_trivia
                .iter()
                .any(|t| t.kind == TriviaKind::LineComment)
        );
    }

    /// Shebang handling: the paideia-as lexer does NOT currently
    /// treat `#!` as a special prefix (`scan_comment` only matches
    /// `//` and `/*`), so a leading `#!` would hit the operator /
    /// unrecognized-byte paths. This test documents the current
    /// behavior so FMT-9 (doc comments) can decide whether to
    /// address the shebang gap at lexer level or in the formatter.
    #[test]
    fn shebang_prefix_is_not_recognized_as_trivia() {
        // We only assert that `tokenize_with_trivia` returns
        // *without panicking* on a shebang-prefixed source. A
        // diagnostic (E0011 unrecognized character) is expected
        // because `#` is not an operator in paideia-as.
        let (_, sink) = run("#!/usr/bin/env paideia-as\nlet x\n");
        // The exact error count depends on lexer policy; we only
        // require that execution completes.
        let _ = sink.error_count();
    }
}
