//! String and byte-string literal scanning per `syntax-reference.md` §5.3 and §5.4.
//!
//! Parses regular strings (`"..."`), raw strings (`r"..."`, `r#"..."#`, etc.),
//! and byte strings (`b"..."`, `br"..."`, etc.). The scanner handles escape
//! sequences (for regular strings) and validates delimiters (for raw strings).

use crate::token::TokenKind;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

/// Outcome of scanning a string literal.
#[derive(Debug, Clone)]
pub struct StringScan {
    /// Token kind: `StringLit`, `ByteStringLit`.
    pub kind: TokenKind,
    /// Number of bytes consumed by the scan.
    pub byte_len: u32,
    /// Diagnostic to emit alongside the token, if any.
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Scan a string literal starting at byte offset `byte_offset` in `content`.
///
/// # Preconditions
///
/// The byte at `byte_offset` is one of:
/// - A double quote `"` (for regular strings).
/// - `r` followed by optional `#`s and a double quote (for raw strings).
/// - `b` followed by a double quote (for byte strings).
/// - `b` or `r` followed by the other, then optional `#`s and `"` (for raw byte strings).
///
/// The lexer dispatcher is responsible for that routing.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a UTF-8
/// char boundary.
#[must_use]
pub fn scan_string(file: FileId, content: &str, byte_offset: u32) -> StringScan {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let bytes = content.as_bytes();
    let mut pos = start;

    // Detect prefixes: b? r? "#"* "..."
    let is_byte = bytes[pos] == b'b';
    if is_byte {
        pos += 1;
    }

    let is_raw = pos < bytes.len() && bytes[pos] == b'r';
    if is_raw {
        pos += 1;
    }

    // Count opening hashes (only for raw strings).
    let (num_hashes, after_hashes) = if is_raw {
        let mut hash_count = 0usize;
        while pos < bytes.len() && bytes[pos] == b'#' {
            hash_count += 1;
            pos += 1;
        }
        (hash_count, pos)
    } else {
        (0, pos)
    };

    // Expect opening quote.
    if after_hashes >= bytes.len() || bytes[after_hashes] != b'"' {
        // Error: expected quote.
        return StringScan {
            kind: if is_byte {
                TokenKind::ByteStringLit
            } else {
                TokenKind::StringLit
            },
            byte_len: (after_hashes - start) as u32,
            diagnostic: Some(Box::new(
                Diagnostic::error(e_code(4))
                    .message("expected opening quote for string literal")
                    .with_span(Span::new(
                        file,
                        byte_offset,
                        byte_offset + (after_hashes - start) as u32,
                    ))
                    .finish(),
            )),
        };
    }

    pos = after_hashes + 1; // Skip opening quote.

    // Scan the string contents.
    if is_raw {
        let (content_len, diag) = scan_raw_string_contents(file, content, pos as u32, num_hashes);
        let closing_found = pos + content_len < bytes.len()
            && bytes[pos + content_len] == b'"'
            && check_raw_closing(bytes, pos + content_len + 1, num_hashes);
        let total_closing_len = if closing_found { 1 + num_hashes } else { 0 };

        StringScan {
            kind: if is_byte {
                TokenKind::ByteStringLit
            } else {
                TokenKind::StringLit
            },
            byte_len: (pos - start + content_len + total_closing_len) as u32,
            diagnostic: if !closing_found {
                Some(Box::new(
                    Diagnostic::error(e_code(4))
                        .message("unterminated string literal")
                        .with_span(Span::new(
                            file,
                            byte_offset,
                            byte_offset + (pos - start + content_len) as u32,
                        ))
                        .finish(),
                ))
            } else {
                diag
            },
        }
    } else {
        // Regular or byte string (with escape processing).
        let (content_len, diag) = scan_regular_string_contents(file, content, pos as u32, is_byte);
        let closing_found = pos + content_len < bytes.len() && bytes[pos + content_len] == b'"';

        StringScan {
            kind: if is_byte {
                TokenKind::ByteStringLit
            } else {
                TokenKind::StringLit
            },
            byte_len: (pos - start + content_len + (if closing_found { 1 } else { 0 })) as u32,
            diagnostic: if !closing_found {
                Some(Box::new(
                    Diagnostic::error(e_code(4))
                        .message("unterminated string literal")
                        .with_span(Span::new(
                            file,
                            byte_offset,
                            byte_offset + (pos - start + content_len) as u32,
                        ))
                        .finish(),
                ))
            } else {
                diag
            },
        }
    }
}

/// Scan raw string contents (no escape processing).
/// Returns (bytes consumed before closing quote or EOF, optional diagnostic).
fn scan_raw_string_contents(
    _file: FileId,
    content: &str,
    start: u32,
    num_hashes: usize,
) -> (usize, Option<Box<Diagnostic>>) {
    let start_usize = start as usize;
    let bytes = content.as_bytes();
    let mut pos = start_usize;

    loop {
        if pos >= bytes.len() {
            // EOF before closing.
            return (pos - start_usize, None);
        }

        if bytes[pos] == b'"' {
            // Check if the following hashes match the opening count.
            if check_raw_closing(bytes, pos + 1, num_hashes) {
                // Found closing; content_len is up to the quote.
                return (pos - start_usize, None);
            }
        }

        // Not a closing sequence; consume the byte.
        pos += 1;
    }
}

/// Check if bytes starting at `pos` contain exactly `num_hashes` consecutive hashes,
/// followed by a non-hash character or EOF.
fn check_raw_closing(bytes: &[u8], pos: usize, num_hashes: usize) -> bool {
    let mut i = 0;
    let mut hash_count = 0;

    while pos + i < bytes.len() && i < num_hashes {
        if bytes[pos + i] == b'#' {
            hash_count += 1;
            i += 1;
        } else {
            break;
        }
    }

    // If we've seen exactly num_hashes and either reached EOF or the next char is not #, it's a match.
    if hash_count == num_hashes {
        // Either we're at EOF or the next char is not a #.
        pos + i >= bytes.len() || bytes[pos + i] != b'#'
    } else {
        false
    }
}

/// Scan regular string contents (with escape processing).
/// Returns (bytes consumed, optional diagnostic).
fn scan_regular_string_contents(
    file: FileId,
    content: &str,
    start: u32,
    is_byte: bool,
) -> (usize, Option<Box<Diagnostic>>) {
    let start_usize = start as usize;
    let bytes = content.as_bytes();
    let mut pos = start_usize;
    let mut first_error: Option<Box<Diagnostic>> = None;

    loop {
        if pos >= bytes.len() {
            // EOF before closing quote.
            return (pos - start_usize, first_error);
        }

        match bytes[pos] {
            b'"' => {
                // Found closing quote.
                return (pos - start_usize, first_error);
            }
            b'\\' => {
                // Escape sequence.
                let (escape_len, diag) = parse_string_escape(file, content, pos as u32, is_byte);
                pos += escape_len;
                if first_error.is_none() && diag.is_some() {
                    first_error = diag;
                }
            }
            b'\n' => {
                // Newline in regular string is an error (not allowed; use raw string or \n).
                if first_error.is_none() {
                    first_error = Some(Box::new(
                        Diagnostic::error(e_code(4))
                            .message("unescaped newline in string literal")
                            .with_span(Span::new(file, start + pos as u32, start + pos as u32 + 1))
                            .finish(),
                    ));
                }
                pos += 1;
            }
            _ => {
                // Regular character.
                let ch = content[pos..].chars().next().unwrap_or('\0');
                pos += ch.len_utf8();
            }
        }
    }
}

/// Parse a string escape sequence starting at `pos` (the backslash).
/// Returns (bytes consumed, optional diagnostic).
fn parse_string_escape(
    file: FileId,
    content: &str,
    pos: u32,
    is_byte: bool,
) -> (usize, Option<Box<Diagnostic>>) {
    let pos_usize = pos as usize;
    let bytes = content.as_bytes();

    assert_eq!(bytes[pos_usize], b'\\', "expected backslash");

    if pos_usize + 1 >= bytes.len() {
        // Backslash at EOF.
        return (1, None);
    }

    match bytes[pos_usize + 1] {
        b'n' | b'r' | b't' | b'\\' | b'\'' | b'"' | b'0' => {
            // Valid escape.
            (2, None)
        }
        b'x' => {
            // Hex escape \xHH.
            if pos_usize + 3 >= bytes.len() {
                let diag = Diagnostic::error(e_code(8))
                    .message("incomplete hex escape sequence")
                    .with_span(Span::new(file, pos, pos + (bytes.len() - pos_usize) as u32))
                    .finish();
                (bytes.len() - pos_usize, Some(Box::new(diag)))
            } else {
                let d1 = parse_hex_digit(bytes[pos_usize + 2]);
                let d2 = parse_hex_digit(bytes[pos_usize + 3]);
                match (d1, d2) {
                    (Some(_), Some(_)) => (4, None),
                    _ => {
                        let diag = Diagnostic::error(e_code(8))
                            .message("invalid hex escape sequence")
                            .with_span(Span::new(file, pos, pos + 4))
                            .finish();
                        (4, Some(Box::new(diag)))
                    }
                }
            }
        }
        b'u' => {
            if is_byte {
                // \u{...} not allowed in byte strings.
                let diag = Diagnostic::error(e_code(8))
                    .message("Unicode escape not allowed in byte string literal")
                    .with_span(Span::new(file, pos, pos + 2))
                    .finish();
                (2, Some(Box::new(diag)))
            } else {
                // Unicode escape \u{...}.
                let (len, diag) = parse_unicode_escape_in_string(file, content, pos);
                (len, diag)
            }
        }
        _ => {
            let diag = Diagnostic::error(e_code(8))
                .message("unknown escape sequence")
                .with_span(Span::new(file, pos, pos + 2))
                .finish();
            (2, Some(Box::new(diag)))
        }
    }
}

/// Parse a Unicode escape sequence in a string context.
/// Returns (bytes consumed, optional diagnostic).
fn parse_unicode_escape_in_string(
    file: FileId,
    content: &str,
    pos: u32,
) -> (usize, Option<Box<Diagnostic>>) {
    let pos_usize = pos as usize;
    let bytes = content.as_bytes();

    // Expect \u{...}
    if pos_usize + 3 >= bytes.len() || bytes[pos_usize + 2] != b'{' {
        let diag = Diagnostic::error(e_code(16))
            .message("incomplete Unicode escape")
            .with_span(Span::new(file, pos, pos + 3))
            .finish();
        return (3, Some(Box::new(diag)));
    }

    // Find closing brace.
    let mut hex_end = pos_usize + 3;
    while hex_end < bytes.len() && bytes[hex_end] != b'}' {
        hex_end += 1;
    }

    if hex_end >= bytes.len() {
        let diag = Diagnostic::error(e_code(16))
            .message("unclosed Unicode escape")
            .with_span(Span::new(file, pos, pos + (hex_end - pos_usize) as u32))
            .finish();
        return (hex_end - pos_usize, Some(Box::new(diag)));
    }

    let hex_str = std::str::from_utf8(&bytes[pos_usize + 3..hex_end]).unwrap_or("");

    if hex_str.is_empty() {
        let diag = Diagnostic::error(e_code(16))
            .message("empty Unicode escape")
            .with_span(Span::new(file, pos, pos + (hex_end - pos_usize + 1) as u32))
            .finish();
        return (hex_end - pos_usize + 1, Some(Box::new(diag)));
    }

    match u32::from_str_radix(hex_str, 16) {
        Ok(val) if val > 0x10FFFF => {
            let diag = Diagnostic::error(e_code(16))
                .message("Unicode codepoint out of range")
                .with_span(Span::new(file, pos, pos + (hex_end - pos_usize + 1) as u32))
                .finish();
            (hex_end - pos_usize + 1, Some(Box::new(diag)))
        }
        Ok(val) if (0xD800..=0xDFFF).contains(&val) => {
            let diag = Diagnostic::error(e_code(17))
                .message("surrogate codepoint not allowed in Unicode escape")
                .with_span(Span::new(file, pos, pos + (hex_end - pos_usize + 1) as u32))
                .finish();
            (hex_end - pos_usize + 1, Some(Box::new(diag)))
        }
        Ok(_val) => (hex_end - pos_usize + 1, None),
        Err(_) => {
            let diag = Diagnostic::error(e_code(16))
                .message("invalid hex digits in Unicode escape")
                .with_span(Span::new(file, pos, pos + (hex_end - pos_usize + 1) as u32))
                .finish();
            (hex_end - pos_usize + 1, Some(Box::new(diag)))
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

/// Extract and process a string literal's content (without quotes or prefixes).
/// Returns the decoded string or an error if escape processing fails.
///
/// This assumes the input is the raw source text of a string including quotes.
/// For regular strings, processes escape sequences; for raw strings, returns as-is.
pub fn extract_string_content(
    content: &str,
    byte_offset: u32,
    is_raw: bool,
    is_byte: bool,
) -> Result<String, String> {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");

    let bytes = content.as_bytes();
    let mut pos = start;

    // Skip prefix: b? r? "#"* "
    if bytes[pos] == b'b' {
        pos += 1;
    }
    if pos < bytes.len() && bytes[pos] == b'r' {
        pos += 1;
    }

    let mut hash_count = 0;
    while pos < bytes.len() && bytes[pos] == b'#' {
        hash_count += 1;
        pos += 1;
    }

    // Skip opening quote
    if pos >= bytes.len() || bytes[pos] != b'"' {
        return Err("missing opening quote".to_string());
    }
    pos += 1;

    if is_raw {
        extract_raw_string_content(content, pos as u32, hash_count)
    } else {
        extract_regular_string_content(content, pos as u32, is_byte)
    }
}

/// Extract raw string content (no escape processing).
fn extract_raw_string_content(
    content: &str,
    start: u32,
    num_hashes: usize,
) -> Result<String, String> {
    let start_usize = start as usize;
    let bytes = content.as_bytes();
    let mut pos = start_usize;

    loop {
        if pos >= bytes.len() {
            return Err("unterminated raw string".to_string());
        }

        if bytes[pos] == b'"' && check_raw_closing(bytes, pos + 1, num_hashes) {
            // Found closing
            return Ok(content[start_usize..pos].to_string());
        }

        pos += 1;
    }
}

/// Extract regular string content with escape processing.
fn extract_regular_string_content(
    content: &str,
    start: u32,
    is_byte: bool,
) -> Result<String, String> {
    let start_usize = start as usize;
    let bytes = content.as_bytes();
    let mut pos = start_usize;
    let mut result = String::new();

    loop {
        if pos >= bytes.len() {
            return Err("unterminated string".to_string());
        }

        match bytes[pos] {
            b'"' => {
                return Ok(result);
            }
            b'\\' => {
                // Process escape sequence
                if pos + 1 >= bytes.len() {
                    return Err("unterminated string".to_string());
                }

                let (ch, advance) = process_string_escape(content, pos as u32, is_byte)?;
                result.push(ch);
                pos += advance;
            }
            b'\n' => {
                return Err("unescaped newline in string".to_string());
            }
            _ => {
                let ch = content[pos..].chars().next().unwrap_or('\0');
                result.push(ch);
                pos += ch.len_utf8();
            }
        }
    }
}

/// Extract byte string content with escape processing.
pub fn extract_byte_string_content(
    content: &str,
    byte_offset: u32,
    is_raw: bool,
) -> Result<Vec<u8>, String> {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");

    let bytes = content.as_bytes();
    let mut pos = start;

    // Skip prefix: b? r? "#"* "
    if bytes[pos] == b'b' {
        pos += 1;
    }
    if pos < bytes.len() && bytes[pos] == b'r' {
        pos += 1;
    }

    let mut hash_count = 0;
    while pos < bytes.len() && bytes[pos] == b'#' {
        hash_count += 1;
        pos += 1;
    }

    // Skip opening quote
    if pos >= bytes.len() || bytes[pos] != b'"' {
        return Err("missing opening quote".to_string());
    }
    pos += 1;

    if is_raw {
        extract_raw_byte_string_content(content, pos as u32, hash_count)
    } else {
        extract_regular_byte_string_content(content, pos as u32)
    }
}

/// Extract raw byte string content (no escape processing).
fn extract_raw_byte_string_content(
    content: &str,
    start: u32,
    num_hashes: usize,
) -> Result<Vec<u8>, String> {
    let start_usize = start as usize;
    let bytes = content.as_bytes();
    let mut pos = start_usize;

    loop {
        if pos >= bytes.len() {
            return Err("unterminated raw byte string".to_string());
        }

        if bytes[pos] == b'"' && check_raw_closing(bytes, pos + 1, num_hashes) {
            // Found closing
            return Ok(bytes[start_usize..pos].to_vec());
        }

        pos += 1;
    }
}

/// Extract regular byte string content with escape processing.
fn extract_regular_byte_string_content(content: &str, start: u32) -> Result<Vec<u8>, String> {
    let start_usize = start as usize;
    let bytes = content.as_bytes();
    let mut pos = start_usize;
    let mut result = Vec::new();

    loop {
        if pos >= bytes.len() {
            return Err("unterminated byte string".to_string());
        }

        match bytes[pos] {
            b'"' => {
                return Ok(result);
            }
            b'\\' => {
                // Process escape sequence
                if pos + 1 >= bytes.len() {
                    return Err("unterminated byte string".to_string());
                }

                let byte_val = process_byte_escape(content, pos as u32)?;
                result.push(byte_val);

                // Advance based on escape type
                match bytes[pos + 1] {
                    b'n' | b'r' | b't' | b'\\' | b'\'' | b'"' | b'0' => {
                        pos += 2;
                    }
                    b'x' => {
                        pos += 4;
                    }
                    _ => {
                        pos += 2;
                    }
                }
            }
            b'\n' => {
                return Err("unescaped newline in byte string".to_string());
            }
            b if b > 127 => {
                return Err("non-ASCII byte in byte string (use escape)".to_string());
            }
            _ => {
                result.push(bytes[pos]);
                pos += 1;
            }
        }
    }
}

/// Process a single escape sequence and return the resulting character + bytes to advance.
fn process_string_escape(content: &str, pos: u32, is_byte: bool) -> Result<(char, usize), String> {
    let pos_usize = pos as usize;
    let bytes = content.as_bytes();

    assert_eq!(bytes[pos_usize], b'\\', "expected backslash");

    if pos_usize + 1 >= bytes.len() {
        return Err("incomplete escape sequence".to_string());
    }

    match bytes[pos_usize + 1] {
        b'n' => Ok(('\n', 2)),
        b'r' => Ok(('\r', 2)),
        b't' => Ok(('\t', 2)),
        b'\\' => Ok(('\\', 2)),
        b'\'' => Ok(('\'', 2)),
        b'"' => Ok(('"', 2)),
        b'0' => Ok(('\0', 2)),
        b'x' => {
            if pos_usize + 3 >= bytes.len() {
                return Err("incomplete hex escape".to_string());
            }
            let d1 = parse_hex_digit(bytes[pos_usize + 2]).ok_or("invalid hex digit")?;
            let d2 = parse_hex_digit(bytes[pos_usize + 3]).ok_or("invalid hex digit")?;
            let byte_val = (d1 << 4) | d2;
            if byte_val > 127 {
                return Err("hex escape out of ASCII range in string".to_string());
            }
            Ok((byte_val as char, 4))
        }
        b'u' => {
            if is_byte {
                return Err("Unicode escape not allowed in byte string".to_string());
            }
            let (codepoint, advance) = parse_unicode_escape(content, pos)?;
            match char::from_u32(codepoint) {
                Some(ch) => Ok((ch, advance)),
                None => Err("invalid Unicode codepoint".to_string()),
            }
        }
        _ => Err("unknown escape sequence".to_string()),
    }
}

/// Process a byte escape sequence and return the resulting byte.
fn process_byte_escape(content: &str, pos: u32) -> Result<u8, String> {
    let pos_usize = pos as usize;
    let bytes = content.as_bytes();

    assert_eq!(bytes[pos_usize], b'\\', "expected backslash");

    if pos_usize + 1 >= bytes.len() {
        return Err("incomplete escape sequence".to_string());
    }

    match bytes[pos_usize + 1] {
        b'n' => Ok(b'\n'),
        b'r' => Ok(b'\r'),
        b't' => Ok(b'\t'),
        b'\\' => Ok(b'\\'),
        b'\'' => Ok(b'\''),
        b'"' => Ok(b'"'),
        b'0' => Ok(b'\0'),
        b'x' => {
            if pos_usize + 3 >= bytes.len() {
                return Err("incomplete hex escape".to_string());
            }
            let d1 = parse_hex_digit(bytes[pos_usize + 2]).ok_or("invalid hex digit")?;
            let d2 = parse_hex_digit(bytes[pos_usize + 3]).ok_or("invalid hex digit")?;
            Ok((d1 << 4) | d2)
        }
        _ => Err("unknown escape sequence in byte string".to_string()),
    }
}

/// Parse a Unicode escape sequence and return (codepoint, bytes_to_advance).
fn parse_unicode_escape(content: &str, pos: u32) -> Result<(u32, usize), String> {
    let pos_usize = pos as usize;
    let bytes = content.as_bytes();

    // Expect \u{...}
    if pos_usize + 2 >= bytes.len() || bytes[pos_usize + 2] != b'{' {
        return Err("incomplete Unicode escape".to_string());
    }

    // Find closing brace
    let mut hex_end = pos_usize + 3;
    while hex_end < bytes.len() && bytes[hex_end] != b'}' {
        hex_end += 1;
    }

    if hex_end >= bytes.len() {
        return Err("unclosed Unicode escape".to_string());
    }

    let hex_str = std::str::from_utf8(&bytes[pos_usize + 3..hex_end])
        .map_err(|_| "invalid UTF-8 in Unicode escape".to_string())?;

    if hex_str.is_empty() {
        return Err("empty Unicode escape".to_string());
    }

    let codepoint = u32::from_str_radix(hex_str, 16)
        .map_err(|_| "invalid hex in Unicode escape".to_string())?;

    if codepoint > 0x10FFFF {
        return Err("Unicode codepoint out of range".to_string());
    }

    if (0xD800..=0xDFFF).contains(&codepoint) {
        return Err("surrogate codepoint not allowed".to_string());
    }

    Ok((codepoint, hex_end - pos_usize + 1))
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
    fn simple_string() {
        let s = "\"hello\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 7);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn empty_string() {
        let s = "\"\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 2);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn string_with_escape() {
        let s = "\"hello\\nworld\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 14);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn raw_string() {
        let s = "r\"raw string\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 13);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn raw_string_with_quote() {
        let s = "r#\"can have \" inside\"# rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        // r# (2) + " (1) + can have " inside (15) + "# (2) = 20
        // Actually: "r#" is 2 chars, "can have " is 9, quote is 1, space is 1, "inside" is 6, "# is 2 = 21
        // Let me count the actual string: r#"can have " inside"#
        // r (1) + # (1) + " (1) + c-a-n- -h-a-v-e (9) + " (1) + space (1) + i-n-s-i-d-e (6) + " (1) + # (1) = 22
        assert_eq!(r.byte_len, 22);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn raw_string_double_hash() {
        let s = "r##\"hello \"# world\"##";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        // r## (3) + " (1) + hello "# world (14) + "## (3) = 21
        assert_eq!(r.byte_len, 21);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn unterminated_string() {
        let s = "\"hello";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 4);
    }

    #[test]
    fn byte_string() {
        let s = "b\"hello\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::ByteStringLit);
        assert_eq!(r.byte_len, 8);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn raw_byte_string() {
        let s = "br\"raw bytes\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::ByteStringLit);
        assert_eq!(r.byte_len, 13);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn string_with_unicode_escape() {
        let s = "\"emoji \\u{1F600}\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 17);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn byte_string_unicode_escape_forbidden() {
        let s = "b\"value \\u{41}\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::ByteStringLit);
        // Should have a diagnostic about \u not allowed in byte strings.
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 8);
    }

    #[test]
    fn string_with_hex_escape() {
        let s = "\"hello \\x41 world\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 18);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn string_with_surrogate_escape() {
        let s = "\"bad \\u{D800}\" rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 17);
    }

    #[test]
    fn raw_string_single_hash() {
        let s = "r#\"inner \" quote\"# rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::StringLit);
        assert_eq!(r.byte_len, 18);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn raw_byte_string_single_hash() {
        let s = "br#\"raw byte\" string\"# rest";
        let r = scan_string(file(), s, 0);
        assert_eq!(r.kind, TokenKind::ByteStringLit);
        // b (1) + r (1) + # (1) + " (1) + raw byte (8) + " (1) + space (1) + string (6) + " (1) + # (1) = 22
        assert_eq!(r.byte_len, 22);
        assert!(r.diagnostic.is_none());
    }
}
