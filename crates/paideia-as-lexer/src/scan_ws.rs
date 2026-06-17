//! Whitespace scanning per `syntax-reference.md` §2.1.
//!
//! Recognized whitespace:
//! - U+0009 (TAB)
//! - U+0020 (SPACE)
//! - U+000A (LF) — emitted as [`TriviaKind::Newline`] (statement separator)
//! - U+000D (CR) inside a `\r\n` pair (also emitted as `Newline`)
//!
//! ZWSP (U+200B) inside source is **forbidden** (§2.1) and emits `E0003`.
//! Other Unicode whitespace (e.g., NBSP, EM SPACE) is currently not
//! recognized; if encountered, the scanner returns `None` and the caller
//! treats the byte as an unrecognized character (likely `E0011`).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

use crate::trivia::TriviaKind;

/// Outcome of scanning a whitespace run.
#[derive(Debug, Clone)]
pub struct WhitespaceScan {
    /// Either `Whitespace` (horizontal) or `Newline`.
    pub kind: TriviaKind,
    /// Number of bytes consumed.
    pub byte_len: u32,
    /// Optional diagnostic — `E0003` for ZWSP.
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Scan a whitespace run starting at `byte_offset`.
///
/// Returns `None` if the byte at `byte_offset` is not whitespace.
///
/// Horizontal whitespace (TAB, SPACE) and newlines are kept distinct:
/// a SPACE-LF-SPACE sequence yields three trivia (Whitespace, Newline,
/// Whitespace). This preserves statement-separator semantics for §9
/// without requiring the lexer driver to peek inside the trivia.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a
/// UTF-8 char boundary.
#[must_use]
pub fn scan_whitespace(file: FileId, content: &str, byte_offset: u32) -> Option<WhitespaceScan> {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let bytes = content.as_bytes();
    let first = bytes[start];

    // Newline: LF, or CRLF.
    if first == b'\n' {
        return Some(WhitespaceScan {
            kind: TriviaKind::Newline,
            byte_len: 1,
            diagnostic: None,
        });
    }
    if first == b'\r' && bytes.get(start + 1) == Some(&b'\n') {
        return Some(WhitespaceScan {
            kind: TriviaKind::Newline,
            byte_len: 2,
            diagnostic: None,
        });
    }

    // ZWSP (U+200B) is forbidden in source. Its UTF-8 encoding is
    // `0xE2 0x80 0x8B` — check before generic whitespace.
    if first == 0xE2 && bytes.get(start + 1) == Some(&0x80) && bytes.get(start + 2) == Some(&0x8B) {
        let diag = Diagnostic::error(e_code(3))
            .message("zero-width space (U+200B) is forbidden in source")
            .with_span(Span::new(file, byte_offset, 3))
            .finish();
        return Some(WhitespaceScan {
            kind: TriviaKind::Whitespace,
            byte_len: 3,
            diagnostic: Some(Box::new(diag)),
        });
    }

    // Horizontal whitespace run (TAB or SPACE).
    if first == b'\t' || first == b' ' {
        let mut end = start + 1;
        while end < bytes.len() && (bytes[end] == b'\t' || bytes[end] == b' ') {
            end += 1;
        }
        return Some(WhitespaceScan {
            kind: TriviaKind::Whitespace,
            byte_len: (end - start) as u32,
            diagnostic: None,
        });
    }

    None
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
    fn space_run() {
        let r = scan_whitespace(file(), "   x", 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Whitespace);
        assert_eq!(r.byte_len, 3);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn tab_run() {
        let r = scan_whitespace(file(), "\t\tx", 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Whitespace);
        assert_eq!(r.byte_len, 2);
    }

    #[test]
    fn mixed_tab_and_space() {
        let r = scan_whitespace(file(), " \t \tx", 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Whitespace);
        assert_eq!(r.byte_len, 4);
    }

    #[test]
    fn lf_is_newline() {
        let r = scan_whitespace(file(), "\nx", 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Newline);
        assert_eq!(r.byte_len, 1);
    }

    #[test]
    fn crlf_is_one_newline() {
        let r = scan_whitespace(file(), "\r\nx", 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Newline);
        assert_eq!(r.byte_len, 2);
    }

    #[test]
    fn whitespace_stops_at_newline() {
        let r = scan_whitespace(file(), "   \nx", 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Whitespace);
        assert_eq!(r.byte_len, 3); // spaces only; the LF is a separate scan.
    }

    #[test]
    fn zwsp_emits_e0003() {
        // ZWSP is 3 bytes: 0xE2 0x80 0x8B.
        let src = "\u{200B}x";
        let r = scan_whitespace(file(), src, 0).unwrap();
        assert_eq!(r.kind, TriviaKind::Whitespace);
        assert_eq!(r.byte_len, 3);
        let d = r.diagnostic.expect("E0003");
        assert_eq!(d.code().number(), 3);
    }

    #[test]
    fn non_whitespace_returns_none() {
        assert!(scan_whitespace(file(), "abc", 0).is_none());
        assert!(scan_whitespace(file(), "(", 0).is_none());
    }

    #[test]
    fn nbsp_not_recognized() {
        // NBSP (U+00A0) is not whitespace under §2.1; scanner returns None
        // so the caller can emit E0011 (unrecognized character).
        let src = "\u{00A0}x";
        assert!(scan_whitespace(file(), src, 0).is_none());
    }
}
