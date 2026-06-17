//! Identifier scanning per `syntax-reference.md` §3.1 and §3.3.
//!
//! The grammar is `identifier ::= (XID_Start | '_') (XID_Continue | '_')*`,
//! adopting the Unicode XID properties via the `unicode-ident` crate.
//! Raw identifiers (§3.3) are `` `keyword` `` — backtick-quoted text that
//! tokenizes as [`TokenKind::Ident`] even if the inner text is a reserved
//! word.
//!
//! Leading-digit identifiers (`1foo`) emit `E0012` per §12; the scanner
//! still consumes the full identifier-shaped run so the lexer driver can
//! continue past the error to surface additional diagnostics.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};
use unicode_ident::{is_xid_continue, is_xid_start};

use crate::reserved::lookup;
use crate::token::TokenKind;

/// Outcome of scanning an identifier-shaped run.
#[derive(Debug, Clone)]
pub struct IdentScan {
    /// Token kind to emit (an `Ident` or a keyword `Kw*`).
    pub kind: TokenKind,
    /// Number of bytes consumed by the scan. The lexer should advance its
    /// cursor by this many bytes.
    pub byte_len: u32,
    /// Diagnostic to emit alongside the token, if any. Currently only
    /// `E0012` (leading-digit identifier) and `E0014` (unterminated raw
    /// identifier) populate this.
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Scan an identifier-shaped run starting at byte offset `byte_offset` in
/// `content`.
///
/// # Preconditions
///
/// The byte at `byte_offset` is one of:
/// - an ASCII underscore `_`,
/// - a Unicode `XID_Start` character,
/// - an ASCII digit (in which case `E0012` is emitted but the scan still
///   consumes the run),
/// - an ASCII backtick `` ` `` (raw identifier per §3.3).
///
/// The caller is responsible for that dispatch.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a UTF-8
/// char boundary.
#[must_use]
pub fn scan_identifier(file: FileId, content: &str, byte_offset: u32) -> IdentScan {
    let start = byte_offset as usize;
    assert!(start <= content.len(), "byte_offset out of range");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let bytes = content.as_bytes();
    if start < bytes.len() && bytes[start] == b'`' {
        return scan_raw_identifier(file, content, byte_offset);
    }

    // Ordinary (possibly Unicode) identifier.
    let rest = &content[start..];
    let mut chars = rest.char_indices();
    let first = chars
        .next()
        .expect("scan_identifier called at end of input");
    let (first_offset, first_char) = first;
    debug_assert_eq!(first_offset, 0);

    let leading_digit = first_char.is_ascii_digit();

    // Consume the run.
    let mut end = first_char.len_utf8();
    for (i, c) in chars {
        if is_xid_continue(c) || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }

    let text = &rest[..end];
    let kind = if leading_digit {
        TokenKind::Ident
    } else {
        lookup(text).unwrap_or(TokenKind::Ident)
    };

    let diagnostic = if leading_digit {
        Some(Box::new(
            Diagnostic::error(e_code(12))
                .message("identifier may not start with a digit")
                .with_span(Span::new(file, byte_offset, end as u32))
                .finish(),
        ))
    } else if !(is_xid_start(first_char) || first_char == '_') {
        // Caller dispatched in error: the first char isn't a valid
        // identifier starter and isn't a digit. Treat the same way as
        // a leading-digit error so the caller's contract is enforced.
        Some(Box::new(
            Diagnostic::error(e_code(12))
                .message("identifier starts with invalid character")
                .with_span(Span::new(file, byte_offset, end as u32))
                .finish(),
        ))
    } else {
        None
    };

    IdentScan {
        kind,
        byte_len: end as u32,
        diagnostic,
    }
}

/// Scan a raw identifier `` `keyword` ``. The caller has verified that
/// `content[byte_offset]` is a backtick.
fn scan_raw_identifier(file: FileId, content: &str, byte_offset: u32) -> IdentScan {
    let start = byte_offset as usize;
    let rest = &content[start..];

    // Find the closing backtick. If absent, the run is the rest of the
    // line (until newline) or end-of-input; we emit a not-yet-existing
    // diagnostic (E0014, unterminated raw identifier — using E0012 as a
    // stand-in until that catalog code lands).
    let mut end = 1usize; // skip opening backtick
    let chars = rest[1..].char_indices();
    let mut closing = None;
    for (i, c) in chars {
        if c == '`' {
            closing = Some(1 + i);
            break;
        }
        if c == '\n' {
            break;
        }
        end = 1 + i + c.len_utf8();
    }

    if let Some(close_at) = closing {
        // Inner identifier text is rest[1..close_at].
        let consumed = close_at + 1; // include closing backtick
        IdentScan {
            kind: TokenKind::Ident,
            byte_len: consumed as u32,
            diagnostic: None,
        }
    } else {
        // Unterminated raw identifier — emit E0012 (closest available
        // category-E code) until a dedicated code is added.
        IdentScan {
            kind: TokenKind::Ident,
            byte_len: end as u32,
            diagnostic: Some(Box::new(
                Diagnostic::error(e_code(12))
                    .message("unterminated raw identifier")
                    .with_span(Span::new(file, byte_offset, end as u32))
                    .finish(),
            )),
        }
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
    fn ascii_identifier() {
        let s = "foo_bar rest";
        let r = scan_identifier(file(), s, 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 7);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn underscore_only_identifier_is_wildcard() {
        let r = scan_identifier(file(), "_", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 1);
    }

    #[test]
    fn leading_underscore_identifier() {
        let r = scan_identifier(file(), "_unused", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 7);
    }

    #[test]
    fn unicode_identifier_kanji() {
        // 家族 = 6 bytes (3 + 3).
        let r = scan_identifier(file(), "家族 rest", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 6);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn reserved_word_resolves_to_keyword_kind() {
        let r = scan_identifier(file(), "let foo", 0);
        assert_eq!(r.kind, TokenKind::KwLet);
        assert_eq!(r.byte_len, 3);
    }

    #[test]
    fn raw_identifier_treats_keyword_as_ident() {
        let r = scan_identifier(file(), "`let` rest", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        // `let` = 5 bytes incl. backticks.
        assert_eq!(r.byte_len, 5);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn raw_identifier_unterminated_emits_diagnostic() {
        let r = scan_identifier(file(), "`let\nrest", 0);
        assert!(r.diagnostic.is_some());
    }

    #[test]
    fn leading_digit_emits_e0012() {
        let r = scan_identifier(file(), "1foo", 0);
        assert_eq!(r.byte_len, 4);
        let d = r.diagnostic.expect("expected E0012");
        assert_eq!(d.code().number(), 12);
    }

    #[test]
    fn identifier_stops_at_punctuation() {
        let r = scan_identifier(file(), "foo,bar", 0);
        assert_eq!(r.byte_len, 3);
    }

    #[test]
    fn identifier_stops_at_whitespace() {
        let r = scan_identifier(file(), "foo bar", 0);
        assert_eq!(r.byte_len, 3);
    }

    #[test]
    fn pascal_case_identifier() {
        let r = scan_identifier(file(), "PascalType ", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 10);
    }

    #[test]
    fn screaming_snake_case_identifier() {
        let r = scan_identifier(file(), "MAX_VAL ", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 7);
    }

    #[test]
    fn self_type_resolves_to_keyword() {
        let r = scan_identifier(file(), "Self other", 0);
        assert_eq!(r.kind, TokenKind::KwSelfType);
    }

    #[test]
    fn self_value_resolves_to_keyword() {
        let r = scan_identifier(file(), "self other", 0);
        assert_eq!(r.kind, TokenKind::KwSelfValue);
    }

    #[test]
    fn identifier_with_trailing_digits() {
        let r = scan_identifier(file(), "x42 rest", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        assert_eq!(r.byte_len, 3);
    }

    #[test]
    fn unicode_xid_continue_after_ascii_start() {
        // "x家" — XID_Continue includes the kanji.
        let r = scan_identifier(file(), "x家", 0);
        assert_eq!(r.kind, TokenKind::Ident);
        // "x" (1) + "家" (3) = 4
        assert_eq!(r.byte_len, 4);
    }
}
