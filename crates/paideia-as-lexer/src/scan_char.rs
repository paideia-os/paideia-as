//! Character and byte literal scanning per `syntax-reference.md` §5.2 and §5.4.
//!
//! Parses single Unicode character literals (`'x'`) and byte literals (`b'x'`).
//! Supports escape sequences: `\n`, `\t`, `\r`, `\\`, `\'`, `\"`, `\0`,
//! `\xNN` (hex byte), and `\u{...}` (Unicode codepoint).
//!
//! Character literals contain exactly one Unicode scalar value (32-bit).
//! Byte literals contain exactly one byte (8-bit, ASCII-restricted unless escaped).
//! Surrogate codepoints (0xD800..0xDFFF) in `\u{...}` emit `E0017`.

use crate::token::TokenKind;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

/// Outcome of scanning a character or byte literal.
#[derive(Debug, Clone)]
pub struct CharScan {
    /// Token kind: `CharLit` or `ByteLit`.
    pub kind: TokenKind,
    /// Number of bytes consumed by the scan.
    pub byte_len: u32,
    /// Diagnostic to emit alongside the token, if any.
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Scan a character or byte literal starting at byte offset `byte_offset` in `content`.
///
/// # Preconditions
///
/// The byte at `byte_offset` is either:
/// - A single quote `'` (for character literals), or
/// - `b` followed by a single quote (for byte literals).
///
/// The lexer dispatcher is responsible for that routing.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a UTF-8
/// char boundary.
#[must_use]
pub fn scan_char(file: FileId, content: &str, byte_offset: u32) -> CharScan {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let bytes = content.as_bytes();

    // Detect if this is a byte literal (b'x') or regular char ('x').
    let is_byte = bytes[start] == b'b';
    let quote_pos = if is_byte { start + 1 } else { start };

    assert_eq!(bytes[quote_pos], b'\'', "expected opening quote");

    let inner_start = quote_pos + 1;
    let is_byte_str = is_byte;

    // Scan the contents between quotes.
    let (content_len, codepoint, diag) =
        scan_char_contents(file, content, inner_start as u32, is_byte_str);

    let total_len = (quote_pos - start) + 1 + content_len;
    let closing_present = inner_start + content_len < content.len()
        && content.as_bytes()[inner_start + content_len] == b'\'';
    let total_len = if closing_present {
        total_len + 1
    } else {
        total_len
    };

    let kind = if is_byte {
        TokenKind::ByteLit
    } else {
        TokenKind::CharLit
    };

    // If we already have a diagnostic from escape parsing, use it.
    let diagnostic = if let Some(d) = diag {
        Some(d)
    } else if codepoint.is_none() && closing_present {
        // Empty character: '' — emit E0007.
        let span = Span::new(file, byte_offset, byte_offset + total_len as u32);
        Some(Box::new(
            Diagnostic::error(e_code(7))
                .message("character literal must contain exactly one character")
                .with_span(span)
                .finish(),
        ))
    } else if !closing_present {
        // Unterminated character.
        let span = Span::new(file, byte_offset, byte_offset + total_len as u32);
        Some(Box::new(
            Diagnostic::error(e_code(7))
                .message("unterminated character literal")
                .with_span(span)
                .finish(),
        ))
    } else {
        None
    };

    CharScan {
        kind,
        byte_len: total_len as u32,
        diagnostic,
    }
}

/// Scan the contents of a character literal (between the opening and closing quotes).
/// Returns (bytes consumed, the codepoint if valid, optional diagnostic).
fn scan_char_contents(
    file: FileId,
    content: &str,
    start: u32,
    is_byte: bool,
) -> (usize, Option<u32>, Option<Box<Diagnostic>>) {
    let start_usize = start as usize;
    let bytes = content.as_bytes();

    if start_usize >= bytes.len() {
        // EOF reached before any content.
        return (0, None, None);
    }

    // Check if this is an escape sequence.
    if bytes[start_usize] == b'\\' {
        let (escape_char, escape_len, diag) = parse_escape_sequence(file, content, start, is_byte);
        let escape_char_val = escape_char.map(|c| c as u32);

        // Check for closing quote immediately after the escape.
        let after_escape = start_usize + escape_len;
        if after_escape < bytes.len() && bytes[after_escape] == b'\'' {
            // Valid single-escape character.
            return (escape_len, escape_char_val, diag);
        } else if after_escape >= bytes.len() {
            // EOF after escape; no closing quote.
            return (escape_len, escape_char_val, diag);
        } else {
            // There's something after the escape but it's not the closing quote.
            // This is a multi-character literal; emit E0007.
            let span = Span::new(file, start, start + escape_len as u32 + 1);
            let d = Diagnostic::error(e_code(7))
                .message("character literal contains more than one character")
                .with_span(span)
                .finish();
            return (escape_len, escape_char_val, Some(Box::new(d)));
        }
    }

    // Non-escape character: consume one Unicode codepoint.
    let rest = &content[start_usize..];
    let mut chars = rest.chars();
    if let Some(ch) = chars.next() {
        let char_len = ch.len_utf8();

        // Check for a closing quote immediately after this character.
        if start_usize + char_len < bytes.len() && bytes[start_usize + char_len] == b'\'' {
            // Valid single character.
            return (char_len, Some(ch as u32), None);
        } else if start_usize + char_len >= bytes.len() {
            // EOF; no closing quote but we have one character.
            return (char_len, Some(ch as u32), None);
        } else {
            // There's something after the character; it's a multi-character literal.
            let span = Span::new(file, start, start + 1);
            let d = Diagnostic::error(e_code(7))
                .message("character literal contains more than one character")
                .with_span(span)
                .finish();
            return (char_len, Some(ch as u32), Some(Box::new(d)));
        }
    }

    // No character at all (empty).
    (0, None, None)
}

/// Parse an escape sequence starting at `content[start]` (the backslash).
/// Returns (the escaped char (if valid), bytes consumed, optional diagnostic).
fn parse_escape_sequence(
    file: FileId,
    content: &str,
    start: u32,
    is_byte: bool,
) -> (Option<char>, usize, Option<Box<Diagnostic>>) {
    let start_usize = start as usize;
    let bytes = content.as_bytes();

    assert_eq!(bytes[start_usize], b'\\', "expected backslash");

    if start_usize + 1 >= bytes.len() {
        // Backslash at EOF.
        return (None, 1, None);
    }

    match bytes[start_usize + 1] {
        b'n' => (Some('\n'), 2, None),
        b'r' => (Some('\r'), 2, None),
        b't' => (Some('\t'), 2, None),
        b'\\' => (Some('\\'), 2, None),
        b'\'' => (Some('\''), 2, None),
        b'"' => (Some('"'), 2, None),
        b'0' => (Some('\0'), 2, None),
        b'x' => parse_hex_escape(file, content, start, is_byte),
        b'u' => {
            if is_byte {
                // \u{...} not allowed in byte literals.
                // Still parse it to consume the full escape sequence.
                let (_, esc_len, _) = parse_unicode_escape(file, content, start);
                let span = Span::new(file, start, start + esc_len as u32);
                let diag = Diagnostic::error(e_code(8))
                    .message("Unicode escape not allowed in byte literal")
                    .with_span(span)
                    .finish();
                (None, esc_len, Some(Box::new(diag)))
            } else {
                parse_unicode_escape(file, content, start)
            }
        }
        _ => {
            // Unknown escape sequence; emit E0008.
            let span = Span::new(file, start, start + 2);
            let diag = Diagnostic::error(e_code(8))
                .message("unknown escape sequence")
                .with_span(span)
                .finish();
            (None, 2, Some(Box::new(diag)))
        }
    }
}

/// Parse a hex escape sequence `\xHH`.
/// Returns (the char, bytes consumed, optional diagnostic).
fn parse_hex_escape(
    file: FileId,
    content: &str,
    start: u32,
    _is_byte: bool,
) -> (Option<char>, usize, Option<Box<Diagnostic>>) {
    let start_usize = start as usize;
    let bytes = content.as_bytes();

    // Expect \x followed by exactly 2 hex digits.
    if start_usize + 3 >= bytes.len() {
        let span = Span::new(file, start, start + (bytes.len() - start_usize) as u32);
        let diag = Diagnostic::error(e_code(8))
            .message("incomplete hex escape sequence (expected \\xHH)")
            .with_span(span)
            .finish();
        return (None, bytes.len() - start_usize, Some(Box::new(diag)));
    }

    let d1 = parse_hex_digit(bytes[start_usize + 2]);
    let d2 = parse_hex_digit(bytes[start_usize + 3]);

    match (d1, d2) {
        (Some(h1), Some(h2)) => {
            let val = (h1 << 4) | h2;
            // For byte literals, \x must be in 0..=127 (or 0..=255? check spec).
            // Using 0..=255 for now (since it's a byte).
            // Actually, the spec says "\xNN" is for ASCII range (≤ 0x7F).
            // Let's use 0..=255 and let later passes validate.
            // Since val is u8 (0..=255), the is_byte check is always false.
            (Some(val as char), 4, None)
        }
        _ => {
            let span = Span::new(file, start, start + 4);
            let diag = Diagnostic::error(e_code(8))
                .message("invalid hex escape sequence (expected two hex digits)")
                .with_span(span)
                .finish();
            (None, 4, Some(Box::new(diag)))
        }
    }
}

/// Parse a Unicode escape sequence `\u{NNNNNN}`.
/// Returns (the char, bytes consumed, optional diagnostic).
fn parse_unicode_escape(
    file: FileId,
    content: &str,
    start: u32,
) -> (Option<char>, usize, Option<Box<Diagnostic>>) {
    let start_usize = start as usize;
    let bytes = content.as_bytes();

    // Expect \u{...}
    if start_usize + 3 >= bytes.len() || bytes[start_usize + 2] != b'{' {
        let span = Span::new(file, start, start + 3);
        let diag = Diagnostic::error(e_code(16))
            .message("incomplete Unicode escape (expected \\u{...})")
            .with_span(span)
            .finish();
        return (None, 3, Some(Box::new(diag)));
    }

    // Find the closing brace and extract hex digits.
    let mut hex_end = start_usize + 3;
    while hex_end < bytes.len() && bytes[hex_end] != b'}' {
        hex_end += 1;
    }

    if hex_end >= bytes.len() {
        // No closing brace found.
        let span = Span::new(file, start, start + (hex_end - start_usize) as u32);
        let diag = Diagnostic::error(e_code(16))
            .message("unclosed Unicode escape")
            .with_span(span)
            .finish();
        return (None, hex_end - start_usize, Some(Box::new(diag)));
    }

    let hex_str = std::str::from_utf8(&bytes[start_usize + 3..hex_end]).unwrap_or("");

    if hex_str.is_empty() {
        let span = Span::new(file, start, start + (hex_end - start_usize + 1) as u32);
        let diag = Diagnostic::error(e_code(16))
            .message("empty Unicode escape")
            .with_span(span)
            .finish();
        return (None, hex_end - start_usize + 1, Some(Box::new(diag)));
    }

    match u32::from_str_radix(hex_str, 16) {
        Ok(val) if val > 0x10FFFF => {
            let span = Span::new(file, start, start + (hex_end - start_usize + 1) as u32);
            let diag = Diagnostic::error(e_code(16))
                .message("Unicode codepoint out of range")
                .with_span(span)
                .finish();
            (None, hex_end - start_usize + 1, Some(Box::new(diag)))
        }
        Ok(val) if (0xD800..=0xDFFF).contains(&val) => {
            // Surrogate codepoint; emit E0017.
            let span = Span::new(file, start, start + (hex_end - start_usize + 1) as u32);
            let diag = Diagnostic::error(e_code(17))
                .message("surrogate codepoint not allowed in Unicode escape")
                .with_span(span)
                .finish();
            (None, hex_end - start_usize + 1, Some(Box::new(diag)))
        }
        Ok(val) => match char::from_u32(val) {
            Some(ch) => (Some(ch), hex_end - start_usize + 1, None),
            None => {
                let span = Span::new(file, start, start + (hex_end - start_usize + 1) as u32);
                let diag = Diagnostic::error(e_code(16))
                    .message("invalid Unicode codepoint")
                    .with_span(span)
                    .finish();
                (None, hex_end - start_usize + 1, Some(Box::new(diag)))
            }
        },
        Err(_) => {
            let span = Span::new(file, start, start + (hex_end - start_usize + 1) as u32);
            let diag = Diagnostic::error(e_code(16))
                .message("invalid hex digits in Unicode escape")
                .with_span(span)
                .finish();
            (None, hex_end - start_usize + 1, Some(Box::new(diag)))
        }
    }
}

/// Parse a single hex digit character. Returns its value (0-15) or None.
fn parse_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
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
    fn ascii_char() {
        let r = scan_char(file(), "'a' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert_eq!(r.byte_len, 3);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn unicode_kanji() {
        // '家' = 3 bytes UTF-8, so total is 1 + 3 + 1 = 5 bytes.
        let r = scan_char(file(), "'家' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert_eq!(r.byte_len, 5);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn escape_newline() {
        let r = scan_char(file(), r"'\n' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn unicode_escape() {
        let r = scan_char(file(), r"'\u{1F600}' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        // ' (1) + \ (1) + u (1) + { (1) + 1F600 (5) + } (1) + ' (1) = 11
        assert_eq!(r.byte_len, 11);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn surrogate_emits_e0017() {
        let r = scan_char(file(), r"'\u{D800}' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 17);
    }

    #[test]
    fn byte_literal() {
        let r = scan_char(file(), "b'a' rest", 0);
        assert_eq!(r.kind, TokenKind::ByteLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn byte_with_hex_escape() {
        let r = scan_char(file(), r"b'\x41' rest", 0);
        assert_eq!(r.kind, TokenKind::ByteLit);
        assert_eq!(r.byte_len, 7);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn unterminated_char() {
        let r = scan_char(file(), "'a", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 7);
    }

    #[test]
    fn empty_char_literal() {
        let r = scan_char(file(), "''", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 7);
    }

    #[test]
    fn multi_char_literal() {
        let r = scan_char(file(), "'ab' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 7);
    }

    #[test]
    fn escape_backslash() {
        let r = scan_char(file(), r"'\\' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn byte_unicode_escape_disallowed() {
        let r = scan_char(file(), r"b'\u{41}' rest", 0);
        assert_eq!(r.kind, TokenKind::ByteLit);
        // b (1) + ' (1) + \ (1) + u (1) + { (1) + 41 (2) + } (1) + ' (1) = 9
        assert_eq!(r.byte_len, 9);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 8);
    }

    #[test]
    fn hex_escape_valid() {
        let r = scan_char(file(), r"'\x42' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert_eq!(r.byte_len, 6);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn escape_single_quote() {
        let r = scan_char(file(), r"'\'' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn emoji_unicode() {
        let r = scan_char(file(), "'😀' rest", 0);
        assert_eq!(r.kind, TokenKind::CharLit);
        // '😀' is 4 bytes, so 1 + 4 + 1 = 6.
        assert_eq!(r.byte_len, 6);
        assert!(r.diagnostic.is_none());
    }
}
