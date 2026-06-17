//! Number literal scanning per `syntax-reference.md` §5.1.
//!
//! Parses decimal, hexadecimal, binary, octal, and floating-point literals.
//! Supports underscore separators and type suffixes (u8, i32, f64, etc.).
//! The scanner produces a `TokenKind::IntLit` or `TokenKind::FloatLit` and
//! validates basic well-formedness:
//!
//! - Base prefixes (0x, 0b, 0o) must have at least one valid digit after the prefix.
//! - Underscores must not be the first char after the base prefix.
//! - Type suffixes must be recognized (u8, i32, f64, usize, etc.).
//! - For fixed-width suffixes, basic overflow detection is performed.

use crate::token::TokenKind;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

/// Outcome of scanning a number literal.
#[derive(Debug, Clone)]
pub struct NumberScan {
    /// Token kind: `IntLit` or `FloatLit`.
    pub kind: TokenKind,
    /// Number of bytes consumed by the scan.
    pub byte_len: u32,
    /// Diagnostic to emit alongside the token, if any (e.g., E0006 for overflow).
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Scan a number literal starting at byte offset `byte_offset` in `content`.
///
/// # Preconditions
///
/// The byte at `byte_offset` is one of:
/// - An ASCII digit `0-9`.
/// - The lexer dispatcher has verified this is a valid number start.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a UTF-8
/// char boundary.
#[must_use]
pub fn scan_number(file: FileId, content: &str, byte_offset: u32) -> NumberScan {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let bytes = content.as_bytes();
    let rest = &content[start..];

    // Check for base prefix: 0x, 0b, 0o
    let (base, prefix_len, after_prefix) = if bytes[start] == b'0' && start + 1 < bytes.len() {
        match bytes[start + 1] {
            b'x' | b'X' => (16, 2, &rest[2..]),
            b'b' | b'B' => (2, 2, &rest[2..]),
            b'o' | b'O' => (8, 2, &rest[2..]),
            _ => (10, 0, rest),
        }
    } else {
        (10, 0, rest)
    };

    // Scan the integer part (digits, underscores, possibly followed by . or e/E for floats).
    let (int_part, int_len, has_digit) = scan_int_part(after_prefix, base);

    // Check if there's a decimal point or exponent (indicates float).
    let next_offset = int_len;
    let is_float = if next_offset < after_prefix.len() {
        let next_byte = after_prefix.as_bytes()[next_offset];
        next_byte == b'.' || next_byte == b'e' || next_byte == b'E'
    } else {
        false
    };

    let (kind, token_len, diagnostic) = if is_float {
        let (flt_len, diag) =
            scan_float_part(file, after_prefix, int_len, byte_offset + prefix_len as u32);
        (TokenKind::FloatLit, flt_len, diag)
    } else {
        // Integer literal; validate and scan suffix.
        if prefix_len > 0 && !has_digit {
            // Base prefix with no digit after it (e.g., "0x" alone).
            let span = Span::new(file, byte_offset, byte_offset + prefix_len as u32);
            let diag = Diagnostic::error(e_code(6))
                .message("number literal requires at least one digit after base prefix")
                .with_span(span)
                .finish();
            return NumberScan {
                kind: TokenKind::IntLit,
                byte_len: (prefix_len + int_len) as u32,
                diagnostic: Some(Box::new(diag)),
            };
        }

        let after_int = &after_prefix[int_len..];
        let (suffix, suffix_len) = scan_suffix(after_int);

        // Validate suffix and perform overflow check if applicable.
        let overflow_diag = validate_number_and_overflow(
            file,
            byte_offset + prefix_len as u32,
            int_part,
            base,
            suffix,
            byte_offset + (prefix_len + int_len + suffix_len) as u32,
        );

        (TokenKind::IntLit, int_len + suffix_len, overflow_diag)
    };

    NumberScan {
        kind,
        byte_len: (prefix_len + token_len) as u32,
        diagnostic,
    }
}

/// Scan the integer part (digits and underscores) in the given base.
/// Returns (the digit sequence as a string, number of bytes consumed, whether at least one digit was found).
fn scan_int_part(content: &str, base: u32) -> (&str, usize, bool) {
    let bytes = content.as_bytes();
    let valid_fn = |c: u8| match base {
        2 => c == b'0' || c == b'1',
        8 => (b'0'..=b'7').contains(&c),
        10 => c.is_ascii_digit(),
        16 => c.is_ascii_hexdigit(),
        _ => false,
    };

    let mut has_digit = false;
    let mut len = 0;

    for (i, &byte) in bytes.iter().enumerate() {
        if valid_fn(byte) {
            has_digit = true;
            len = i + 1;
        } else if byte == b'_' && has_digit {
            // Underscore allowed if we already have at least one digit.
            len = i + 1;
        } else {
            break;
        }
    }

    (&content[..len], len, has_digit)
}

/// Scan the fractional part and exponent of a float literal.
/// Returns (number of bytes consumed after the integer part, optional diagnostic).
fn scan_float_part(
    file: FileId,
    content: &str,
    int_len: usize,
    int_start: u32,
) -> (usize, Option<Box<Diagnostic>>) {
    let bytes = content.as_bytes();
    let mut len = int_len;

    // Scan fractional part if present.
    if len < bytes.len() && bytes[len] == b'.' {
        len += 1;
        // After the dot, consume at least one digit.
        let (_frac_part, frac_len, _has_frac_digit) = scan_int_part(&content[len..], 10);
        len += frac_len;
    }

    // Scan exponent if present.
    if len < bytes.len() && (bytes[len] == b'e' || bytes[len] == b'E') {
        len += 1;
        if len < bytes.len() && (bytes[len] == b'+' || bytes[len] == b'-') {
            len += 1;
        }
        let (_exp_part, exp_len, _has_exp_digit) = scan_int_part(&content[len..], 10);
        len += exp_len;
    }

    // Scan float type suffix (f32, f64).
    let after_float = &content[len..];
    let (suffix, suffix_len) = scan_suffix(after_float);

    // Validate float suffix.
    let diagnostic = if !suffix.is_empty() && suffix != "f32" && suffix != "f64" {
        let span = Span::new(
            file,
            int_start + len as u32,
            int_start + (len + suffix_len) as u32,
        );
        Some(Box::new(
            Diagnostic::error(e_code(6))
                .message("invalid float suffix")
                .with_span(span)
                .finish(),
        ))
    } else {
        None
    };

    (len + suffix_len, diagnostic)
}

/// Scan an optional type suffix (e.g., u8, i32, f64).
/// Returns (the suffix string, number of bytes consumed).
fn scan_suffix(content: &str) -> (&str, usize) {
    let bytes = content.as_bytes();
    if bytes.is_empty() {
        return ("", 0);
    }

    // Suffixes are lowercase letter(s), optionally followed by digits.
    let mut len = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_lowercase() || byte == b'_' || (i > 0 && byte.is_ascii_digit()) {
            len = i + 1;
        } else {
            break;
        }
    }

    (&content[..len], len)
}

/// Validate that the suffix is recognized and check for overflow if a fixed-width suffix is present.
/// Returns an optional diagnostic for overflow or invalid suffix.
fn validate_number_and_overflow(
    file: FileId,
    int_start: u32,
    int_text: &str,
    base: u32,
    suffix: &str,
    end: u32,
) -> Option<Box<Diagnostic>> {
    // List of valid suffixes and their max values (for overflow checking).
    let valid_suffixes = [
        ("u8", 0u128, 255u128),
        ("u16", 0u128, 65535u128),
        ("u32", 0u128, 4294967295u128),
        ("u64", 0u128, 18446744073709551615u128),
        ("u128", 0u128, 340282366920938463463374607431768211455u128),
        ("usize", 0u128, 18446744073709551615u128), // Assume 64-bit
        ("i8", 0u128, 127u128),
        ("i16", 0u128, 32767u128),
        ("i32", 0u128, 2147483647u128),
        ("i64", 0u128, 9223372036854775807u128),
        ("i128", 0u128, 170141183460469231731687303715884105727u128),
        ("isize", 0u128, 9223372036854775807u128), // Assume 64-bit, positive half
    ];

    if suffix.is_empty() {
        // Unsuffixed integer; no overflow check (parser decides at type-check time).
        return None;
    }

    // Check if suffix is valid.
    let max_val = valid_suffixes
        .iter()
        .find(|(suf, _, _)| *suf == suffix)
        .map(|(_, _, max)| *max);

    let max_val = match max_val {
        Some(m) => m,
        None => {
            // Unknown suffix; emit E0006.
            let span = Span::new(file, int_start, end);
            return Some(Box::new(
                Diagnostic::error(e_code(6))
                    .message(format!("unknown integer suffix '{}'", suffix))
                    .with_span(span)
                    .finish(),
            ));
        }
    };

    // Try to parse the integer value in the given base.
    let clean_int = int_text.replace('_', "");
    let val_result = u128::from_str_radix(&clean_int, base);

    match val_result {
        Ok(val) if val > max_val => {
            let span = Span::new(file, int_start, end);
            Some(Box::new(
                Diagnostic::error(e_code(6))
                    .message(format!(
                        "number {} is too large for type '{}'",
                        clean_int, suffix
                    ))
                    .with_span(span)
                    .finish(),
            ))
        }
        Err(_e) => {
            // Radix parsing failed (shouldn't happen if scan_int_part validated correctly).
            let span = Span::new(file, int_start, end);
            Some(Box::new(
                Diagnostic::error(e_code(6))
                    .message("invalid number literal")
                    .with_span(span)
                    .finish(),
            ))
        }
        Ok(_) => None, // Value is within range.
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
    fn decimal_simple() {
        let r = scan_number(file(), "42 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 2);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn decimal_with_separators() {
        let r = scan_number(file(), "1_000_000 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 9);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn decimal_with_suffix() {
        let r = scan_number(file(), "1_000_000u64 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 12);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn hex_lowercase() {
        let r = scan_number(file(), "0xff_aa rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 7);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn hex_uppercase() {
        let r = scan_number(file(), "0xFF rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn binary() {
        let r = scan_number(file(), "0b1010 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 6);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn octal() {
        let r = scan_number(file(), "0o755 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 5);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn float_basic() {
        let r = scan_number(file(), "3.14 rest", 0);
        assert_eq!(r.kind, TokenKind::FloatLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn float_with_exp() {
        let r = scan_number(file(), "3.14e10f32 rest", 0);
        assert_eq!(r.kind, TokenKind::FloatLit);
        assert_eq!(r.byte_len, 10);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn hex_overflow_u64() {
        let r = scan_number(file(), "0xFFFF_FFFF_FFFF_FFFFFu64 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        // The number is too large for u64, should have a diagnostic.
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 6);
    }

    #[test]
    fn empty_hex_prefix() {
        let r = scan_number(file(), "0x rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        // "0x" consumed, no digit after prefix.
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 6);
    }

    #[test]
    fn leading_underscore_after_prefix() {
        // "0x_ff" should fail: underscore immediately after prefix.
        let r = scan_number(file(), "0x_ff rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        // No digit after prefix (underscore alone doesn't count).
        assert!(r.diagnostic.is_some());
    }

    #[test]
    fn float_with_negative_exp() {
        let r = scan_number(file(), "1e-5 rest", 0);
        assert_eq!(r.kind, TokenKind::FloatLit);
        assert_eq!(r.byte_len, 4);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn zero() {
        let r = scan_number(file(), "0 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 1);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn u8_at_boundary() {
        let r = scan_number(file(), "255u8 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 5);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn u8_overflow() {
        let r = scan_number(file(), "256u8 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 6);
    }

    #[test]
    fn unknown_suffix() {
        let r = scan_number(file(), "123xyz rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert!(r.diagnostic.is_some());
        let diag = r.diagnostic.as_ref().unwrap();
        assert_eq!(diag.code().number(), 6);
    }

    #[test]
    fn binary_with_separator() {
        let r = scan_number(file(), "0b1111_0000 rest", 0);
        assert_eq!(r.kind, TokenKind::IntLit);
        assert_eq!(r.byte_len, 11);
        assert!(r.diagnostic.is_none());
    }
}
