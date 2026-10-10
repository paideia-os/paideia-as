//! Comment extraction helper — FMT-15 (paideia-as#1579).
//!
//! Lexes a `.pdx` source string and returns every comment-flavoured
//! trivia it carries, in document order, with the raw text slice
//! preserved verbatim. This is the ground-truth input to the FMT-15
//! comment-preservation corpus harness
//! (`tests/corpus_comment_preservation.rs`): we extract comments from
//! the original source, format the source, extract comments from the
//! output, and assert the two lists match text-for-text and in order.
//!
//! # Scope
//!
//! * **Comment kinds covered.** All four [`TriviaKind`] variants that
//!   represent user-authored comments: [`TriviaKind::LineComment`],
//!   [`TriviaKind::BlockComment`], [`TriviaKind::DocLineComment`],
//!   [`TriviaKind::DocBlockComment`]. Whitespace and newline trivia are
//!   deliberately *not* collected — the style spec (§9) only promises
//!   that *comment* text survives round-trips, not inter-token
//!   whitespace runs.
//! * **Verbatim text.** Each [`CommentToken`] holds the raw source
//!   slice, markers included (`//` / `///` / `/*` / `*/`). No reflow,
//!   no trim, no normalisation. Mirrors style-spec §9.2 wording-
//!   preservation guarantee.
//! * **Ordered.** The vec is in source-byte order: leading-trivia runs
//!   per real token followed by the file-trailing bucket. This matches
//!   how a human reader walks the file and lets the harness assert
//!   positional equality (not just set equality).
//! * **No parse required.** We re-use the FMT-1a
//!   [`crate::cst::tokenize_with_trivia`] pass, which is lex-only. The
//!   harness therefore works even on files whose structural formatter
//!   is still in the FMT-3 byte-identical fallback path.
//!
//! # Error handling
//!
//! Pre-lex failures (invalid UTF-8, lone CR) return an empty vec rather
//! than panicking — the corpus harness has its own policy on files that
//! fail validation and will skip them explicitly. Lex-time diagnostics
//! (unterminated block comment, bad numeric literal, etc.) are
//! swallowed into a local [`VecSink`] and *do not* short-circuit
//! collection: whatever trivia the lexer emitted before the error is
//! still returned, which is the correct behaviour for a comment-only
//! inspector.
//!
//! # Why re-lex instead of reusing the formatter's `TriviaStream`?
//!
//! The formatter's internal `TriviaStream` is private to a single
//! `format` call and threaded through [`crate::walker::FormatContext`].
//! The corpus harness needs comments from *two independent sources*
//! (the input and the formatted output), neither of which has that
//! context handy. A free function that takes `&str` and returns
//! `Vec<CommentToken>` is the minimal, decoupled surface — it does not
//! depend on the parser, the AST, or the formatter options.

use paideia_as_diagnostics::{DiagnosticSink, FileId, VecSink};
use paideia_as_lexer::{SourceText, TriviaKind};

use crate::cst::tokenize_with_trivia;

/// One comment trivia extracted from a source string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentToken {
    /// Which flavour of comment this is (line / block / doc-line /
    /// doc-block). Carried through so a mismatch report can call out a
    /// kind change (e.g., `///` demoted to `//`) and so the harness can
    /// treat the four kinds uniformly without re-inspecting the text.
    pub kind: TriviaKind,
    /// Byte offset into the *validated* source text (post-BOM-strip)
    /// where this trivia begins. Primarily a debugging aid for diff
    /// reports; equality is driven by [`text`](Self::text), not offset.
    pub byte_offset: u32,
    /// The raw source bytes covered by this trivia, verbatim. Includes
    /// the comment markers (`//`, `///`, `/*`, `*/`). Preserves
    /// embedded whitespace exactly as the author typed it (style-spec
    /// §9.2).
    pub text: String,
}

impl CommentToken {
    /// `true` iff this is a doc comment (line or block). Mirrors
    /// [`paideia_as_lexer::Trivia::is_doc`] so callers do not have to
    /// `match` on both kinds.
    #[must_use]
    pub fn is_doc(&self) -> bool {
        matches!(
            self.kind,
            TriviaKind::DocLineComment | TriviaKind::DocBlockComment
        )
    }
}

/// Returns every comment trivia in `source`, in document order.
///
/// See the module docs for the exact contract. Short version:
///
/// * Pre-lex failures → empty vec (never panics).
/// * Lex-time diagnostics are swallowed; already-emitted trivia is kept.
/// * Only [`TriviaKind::LineComment`] / [`BlockComment`] /
///   [`DocLineComment`] / [`DocBlockComment`] appear in the result.
/// * `text` is a byte-slice of the validated source — identical to what
///   the user typed, markers included.
///
/// [`BlockComment`]: TriviaKind::BlockComment
/// [`DocLineComment`]: TriviaKind::DocLineComment
/// [`DocBlockComment`]: TriviaKind::DocBlockComment
#[must_use]
pub fn extract_comments(source: &str) -> Vec<CommentToken> {
    // FileId::new(1) is non-zero so this is infallible; the match is
    // just a total-cover guard against a future signature change.
    let file = match FileId::new(1) {
        Some(f) => f,
        None => return Vec::new(),
    };

    // Pre-lex validation (BOM strip, lone-CR check). On failure the
    // harness treats the file as unformattable; mirror that here by
    // returning an empty vec rather than panicking.
    let source_text = match SourceText::from_bytes(file, source.as_bytes()) {
        Ok(s) => s,
        Err(_diag) => return Vec::new(),
    };

    let content = source_text.content();
    let mut sink = VecSink::new();
    let stream = tokenize_with_trivia(file, &source_text, &mut sink);
    // We INTENTIONALLY ignore `sink.error_count()`: an unterminated
    // block comment at EOF should still surface the comments that came
    // before it. The harness is comment-centric, not lex-centric.
    let _ = sink.error_count();

    let total = stream
        .leading_trivia
        .iter()
        .map(|v| v.len())
        .sum::<usize>()
        + stream.trailing_trivia.len();
    let mut out = Vec::with_capacity(total);

    // Leading-trivia runs, in token order. `leading_trivia[i]` sits
    // immediately before `tokens[i]` — this iteration order equals
    // document (byte) order.
    for run in &stream.leading_trivia {
        for trivia in run {
            if !is_comment(trivia.kind) {
                continue;
            }
            if let Some(tok) = slice_trivia(content, trivia) {
                out.push(tok);
            }
        }
    }
    // Then the file-trailing bucket (whatever sits between the last
    // real token and EOF). Captures a trailing `// end of file`
    // comment, which otherwise would attach to the never-materialised
    // EOF sentinel.
    for trivia in &stream.trailing_trivia {
        if !is_comment(trivia.kind) {
            continue;
        }
        if let Some(tok) = slice_trivia(content, trivia) {
            out.push(tok);
        }
    }

    out
}

/// `true` iff `kind` is a user-authored comment (any of the four
/// comment variants). Kept as a free fn so the two callers in
/// [`extract_comments`] share one source of truth.
fn is_comment(kind: TriviaKind) -> bool {
    matches!(
        kind,
        TriviaKind::LineComment
            | TriviaKind::BlockComment
            | TriviaKind::DocLineComment
            | TriviaKind::DocBlockComment
    )
}

/// Build a [`CommentToken`] from a `Trivia` + the validated source.
///
/// Returns `None` on bogus span bounds (lexer invariant violation —
/// should never fire, but keeps the extractor total). Slicing out of
/// bounds would otherwise panic.
fn slice_trivia(content: &str, trivia: &paideia_as_lexer::Trivia) -> Option<CommentToken> {
    let start = trivia.span.byte_start() as usize;
    let end = trivia.span.byte_end() as usize;
    let bytes = content.as_bytes();
    if end > bytes.len() || start > end {
        return None;
    }
    // The lexer only emits comment trivia on UTF-8 char boundaries
    // (both markers are ASCII), so direct string slicing is safe. We
    // nonetheless go through `get` for defence in depth.
    let text = content.get(start..end)?.to_string();
    Some(CommentToken {
        kind: trivia.kind,
        byte_offset: trivia.span.byte_start(),
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_empty_source_returns_empty() {
        assert!(extract_comments("").is_empty());
    }

    #[test]
    fn extract_source_without_comments_returns_empty() {
        assert!(extract_comments("let x = 1\n").is_empty());
    }

    #[test]
    fn extract_single_line_comment() {
        let src = "// hello\nlet x = 1\n";
        let got = extract_comments(src);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, TriviaKind::LineComment);
        assert_eq!(got[0].text, "// hello");
    }

    #[test]
    fn extract_trailing_line_comment_from_trailing_bucket() {
        // A trailing comment after the last real token lives in
        // TriviaStream::trailing_trivia; the extractor must still pick
        // it up.
        let src = "let x = 1 // tail\n";
        let got = extract_comments(src);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "// tail");
    }

    #[test]
    fn extract_block_comment_preserves_internal_whitespace() {
        let src = "/* a\n   b\n   c */ let x = 1\n";
        let got = extract_comments(src);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, TriviaKind::BlockComment);
        assert_eq!(got[0].text, "/* a\n   b\n   c */");
    }

    #[test]
    fn extract_nested_block_comment_as_single_token() {
        // The lexer flattens nested block comments to one trivia; so
        // must the extractor.
        let src = "/* outer /* inner */ outer */ let x = 1\n";
        let got = extract_comments(src);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, TriviaKind::BlockComment);
        assert_eq!(got[0].text, "/* outer /* inner */ outer */");
    }

    #[test]
    fn extract_doc_line_and_doc_block() {
        let src = "/// doc line\n/** doc block */\nlet x = 1\n";
        let got = extract_comments(src);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].kind, TriviaKind::DocLineComment);
        assert_eq!(got[0].text, "/// doc line");
        assert!(got[0].is_doc());
        assert_eq!(got[1].kind, TriviaKind::DocBlockComment);
        assert_eq!(got[1].text, "/** doc block */");
        assert!(got[1].is_doc());
    }

    #[test]
    fn extract_order_matches_document_order() {
        // Four comments scattered across leading-of-token-0,
        // leading-of-token-1, and the trailing bucket. The vec must be
        // in source order, not grouped by kind.
        let src = "// A\n/* B */ let x = 1 // C\n/// D\n";
        let got = extract_comments(src);
        let kinds: Vec<_> = got.iter().map(|c| c.kind).collect();
        let texts: Vec<_> = got.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            kinds,
            vec![
                TriviaKind::LineComment,
                TriviaKind::BlockComment,
                TriviaKind::LineComment,
                TriviaKind::DocLineComment,
            ]
        );
        assert_eq!(texts, vec!["// A", "/* B */", "// C", "/// D"]);
    }

    #[test]
    fn extract_handles_crlf_source() {
        // Spans point into `content` which keeps CRLF intact (only BOM
        // is stripped by SourceText).
        let src = "// one\r\n// two\r\nlet x = 1\r\n";
        let got = extract_comments(src);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].text, "// one");
        assert_eq!(got[1].text, "// two");
    }

    #[test]
    fn extract_rejects_lone_cr_gracefully() {
        // Lone CR fails SourceText validation. extract_comments must
        // return empty (not panic) so the harness can decide what to
        // do with that file.
        let src = "// doc\rlet x = 1\n";
        assert!(extract_comments(src).is_empty());
    }

    #[test]
    fn extract_byte_offsets_are_monotonic() {
        // Document-order iteration implies non-decreasing byte offsets.
        let src = "// A\n// B\n// C\nlet x = 1\n";
        let got = extract_comments(src);
        for pair in got.windows(2) {
            assert!(pair[0].byte_offset < pair[1].byte_offset);
        }
    }
}
