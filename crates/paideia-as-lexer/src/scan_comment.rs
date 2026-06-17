//! Comment scanning per `syntax-reference.md` §2.2.
//!
//! Four shapes:
//! - `// line` — [`TriviaKind::LineComment`]
//! - `/* block */` — [`TriviaKind::BlockComment`]
//! - `/// doc-line` — [`TriviaKind::DocLineComment`]
//! - `/** doc-block */` — [`TriviaKind::DocBlockComment`]
//!
//! Block comments are **flat** (non-nested) in phase 1: the scanner stops
//! at the first `*/` it sees. §2.2 does not specify nesting and Rust's
//! convention is non-binding here; nested block comments are a deferred
//! follow-up if needed.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

use crate::trivia::TriviaKind;

/// Outcome of scanning a comment.
#[derive(Debug, Clone)]
pub struct CommentScan {
    /// Trivia kind to emit.
    pub kind: TriviaKind,
    /// Number of bytes consumed (includes opening `//` / `/*` and the
    /// closing `*/` if present).
    pub byte_len: u32,
    /// Diagnostic (only `E0009` for unterminated block comments — until
    /// E0009 is in the catalog, we use the generic invalid-comment
    /// stand-in `E0004`).
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Scan a comment starting at `byte_offset`.
///
/// # Preconditions
///
/// The caller has verified that the two bytes at `byte_offset` are `/`
/// followed by either `/` or `*`. The scanner panics otherwise.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a UTF-8
/// char boundary, or if the prefix isn't a comment opener.
#[must_use]
pub fn scan_comment(file: FileId, content: &str, byte_offset: u32) -> CommentScan {
    let start = byte_offset as usize;
    assert!(start + 1 < content.len(), "byte_offset too close to end");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let bytes = content.as_bytes();
    assert_eq!(bytes[start], b'/', "comment must start with '/'");

    match bytes[start + 1] {
        b'/' => scan_line_comment(content, byte_offset),
        b'*' => scan_block_comment(file, content, byte_offset),
        other => panic!(
            "scan_comment called on non-comment prefix '/{}'",
            other as char
        ),
    }
}

/// Scan `//` or `///`. Stops at first `\n` (exclusive) or EOF.
fn scan_line_comment(content: &str, byte_offset: u32) -> CommentScan {
    let start = byte_offset as usize;
    let bytes = content.as_bytes();

    // §2.2: `///` is a doc-line comment only if the third byte exists,
    // is not `/` (which would make it a 4-slash comment, just a regular
    // line comment by convention), and is part of the comment.
    let is_doc = bytes.get(start + 2) == Some(&b'/') && bytes.get(start + 3) != Some(&b'/');

    // Scan until newline or EOF.
    let mut end = start;
    while end < bytes.len() && bytes[end] != b'\n' {
        end += 1;
    }

    CommentScan {
        kind: if is_doc {
            TriviaKind::DocLineComment
        } else {
            TriviaKind::LineComment
        },
        byte_len: (end - start) as u32,
        diagnostic: None,
    }
}

/// Scan `/* ... */` or `/** ... */`. Phase-1 non-nested.
fn scan_block_comment(file: FileId, content: &str, byte_offset: u32) -> CommentScan {
    let start = byte_offset as usize;
    let bytes = content.as_bytes();

    // `/**` is a doc-block comment only if the third byte exists, is
    // `*`, and the fourth byte is not `/` (which would make `/**/`,
    // an empty regular block comment).
    let is_doc = bytes.get(start + 2) == Some(&b'*') && bytes.get(start + 3) != Some(&b'/');

    // Walk forward, looking for `*/`.
    let mut i = start + 2;
    let mut closed = false;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            i += 2;
            closed = true;
            break;
        }
        i += 1;
    }

    if !closed {
        // Walk to EOF.
        i = bytes.len();
    }

    let kind = if is_doc {
        TriviaKind::DocBlockComment
    } else {
        TriviaKind::BlockComment
    };

    let diagnostic = if !closed {
        Some(Box::new(
            Diagnostic::error(e_code(4))
                .message("unterminated block comment")
                .with_span(Span::new(file, byte_offset, (i - start) as u32))
                .finish(),
        ))
    } else {
        None
    };

    CommentScan {
        kind,
        byte_len: (i - start) as u32,
        diagnostic,
    }
}

fn e_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::E, Severity::Error, n).expect("valid E code")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> FileId {
        FileId::new(1).unwrap()
    }

    #[test]
    fn line_comment_basic() {
        let r = scan_comment(file(), "// foo\nlet x", 0);
        assert_eq!(r.kind, TriviaKind::LineComment);
        assert_eq!(r.byte_len, 6); // "// foo"
    }

    #[test]
    fn line_comment_to_eof() {
        let r = scan_comment(file(), "// foo", 0);
        assert_eq!(r.kind, TriviaKind::LineComment);
        assert_eq!(r.byte_len, 6);
    }

    #[test]
    fn doc_line_comment() {
        let r = scan_comment(file(), "/// doc\nlet x", 0);
        assert_eq!(r.kind, TriviaKind::DocLineComment);
        assert_eq!(r.byte_len, 7); // "/// doc"
    }

    #[test]
    fn four_slashes_is_line_comment_not_doc() {
        let r = scan_comment(file(), "//// quad\nlet x", 0);
        assert_eq!(r.kind, TriviaKind::LineComment);
    }

    #[test]
    fn block_comment_basic() {
        let r = scan_comment(file(), "/* foo */ rest", 0);
        assert_eq!(r.kind, TriviaKind::BlockComment);
        assert_eq!(r.byte_len, 9); // "/* foo */"
    }

    #[test]
    fn block_comment_spans_lines() {
        let r = scan_comment(file(), "/* line1\nline2 */ rest", 0);
        assert_eq!(r.kind, TriviaKind::BlockComment);
        assert_eq!(r.byte_len, 17); // "/* line1\nline2 */"
    }

    #[test]
    fn nested_looking_block_comment_is_flat() {
        // §2.2 silent on nesting; phase-1 takes the flat path: the first
        // `*/` closes the comment. The trailing `c */` is NOT part of
        // this comment.
        let r = scan_comment(file(), "/* a /* b */ c */", 0);
        assert_eq!(r.kind, TriviaKind::BlockComment);
        assert_eq!(r.byte_len, 12); // "/* a /* b */"
    }

    #[test]
    fn doc_block_comment() {
        let r = scan_comment(file(), "/** doc */ rest", 0);
        assert_eq!(r.kind, TriviaKind::DocBlockComment);
        assert_eq!(r.byte_len, 10);
    }

    #[test]
    fn empty_block_is_not_doc() {
        // `/**/` is an empty block comment, not a (malformed) doc-block.
        let r = scan_comment(file(), "/**/ rest", 0);
        assert_eq!(r.kind, TriviaKind::BlockComment);
        assert_eq!(r.byte_len, 4);
    }

    #[test]
    fn unterminated_block_emits_diagnostic() {
        let r = scan_comment(file(), "/* unterminated", 0);
        assert!(r.diagnostic.is_some());
        // Consumed to EOF.
        assert_eq!(r.byte_len, 15);
    }

    #[test]
    fn empty_line_comment() {
        let r = scan_comment(file(), "//\nx", 0);
        assert_eq!(r.kind, TriviaKind::LineComment);
        assert_eq!(r.byte_len, 2);
    }
}
