//! Operator and punctuation scanning per `syntax-reference.md` §4 and §6.
//!
//! The scanner is longest-match: when the input could start `==` or `=`,
//! the two-character form wins. ASCII multi-char operators are tested
//! before single-char ones. Unicode equivalents from §4 (`→`, `↦`, `↓`,
//! `∷`) are recognized as alternates for `->`, `=>`, `$`, `::`.
//!
//! ASCII-only mode: if a recognized Unicode glyph is encountered, emit
//! `E0010` and still produce the corresponding token so the lexer can
//! continue (§2.4 multi-diagnostic recovery).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

use crate::glyph_table;
use crate::token::TokenKind;

/// Outcome of scanning an operator or punctuation run.
#[derive(Debug, Clone)]
pub struct OpScan {
    /// Token kind to emit.
    pub kind: TokenKind,
    /// Number of bytes consumed.
    pub byte_len: u32,
    /// Optional diagnostic — currently only `E0010` when a Unicode glyph
    /// is encountered in ASCII-only mode.
    pub diagnostic: Option<Box<Diagnostic>>,
}

/// Toggle for §4 Unicode glyph acceptance.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AsciiMode {
    /// Default: accept both Unicode glyphs and their ASCII fallbacks.
    UnicodeAllowed,
    /// Strict ASCII: emit `E0010` (recoverably) when a Unicode glyph is
    /// encountered.
    AsciiOnly,
}

/// Scan one operator or punctuation token starting at `byte_offset`.
///
/// Returns `None` if the byte at `byte_offset` does not begin a known
/// operator or punctuation. The caller is responsible for dispatching
/// identifier, number, char, and string scans.
///
/// # Panics
///
/// Panics if `byte_offset` is past the end of `content` or not on a UTF-8
/// char boundary.
#[must_use]
pub fn scan_op(file: FileId, content: &str, byte_offset: u32, mode: AsciiMode) -> Option<OpScan> {
    let start = byte_offset as usize;
    assert!(start < content.len(), "byte_offset out of range");
    assert!(
        content.is_char_boundary(start),
        "byte_offset not on a char boundary"
    );

    let rest = &content[start..];

    // 1. Try Unicode glyphs from the table (longest first by char count).
    //    All current §4 Unicode glyphs are single-char; check by prefix.
    for entry in glyph_table::GLYPHS {
        if let Some(uni) = entry.unicode
            && rest.starts_with(uni)
        {
            let diagnostic = match mode {
                AsciiMode::AsciiOnly => Some(Box::new(
                    Diagnostic::error(e_code(10))
                        .message(format!(
                            "Unicode operator '{uni}' used in ASCII-only mode; use '{}' instead",
                            entry.ascii
                        ))
                        .with_span(Span::new(file, byte_offset, uni.len() as u32))
                        .finish(),
                )),
                AsciiMode::UnicodeAllowed => None,
            };
            return Some(OpScan {
                kind: entry.kind,
                byte_len: uni.len() as u32,
                diagnostic,
            });
        }
    }

    // 2. Multi-char ASCII operators (longest-match, 3-char first).
    if let Some(scan) = three_char_ascii(rest) {
        return Some(OpScan {
            kind: scan,
            byte_len: 3,
            diagnostic: None,
        });
    }
    if let Some(scan) = two_char_ascii(rest) {
        return Some(OpScan {
            kind: scan,
            byte_len: 2,
            diagnostic: None,
        });
    }

    // 3. Single-char ASCII punctuation and operators.
    let first = rest.as_bytes()[0];
    let kind = single_char_ascii(first)?;
    Some(OpScan {
        kind,
        byte_len: 1,
        diagnostic: None,
    })
}

fn three_char_ascii(rest: &str) -> Option<TokenKind> {
    // No 3-char operator in the phase-1 set (§6's `...` would go here).
    // Reserved for future: `...` -> DotDotDot, `==>` -> reserved.
    let _ = rest;
    None
}

fn two_char_ascii(rest: &str) -> Option<TokenKind> {
    let bytes = rest.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    Some(match (bytes[0], bytes[1]) {
        (b'-', b'>') => TokenKind::Arrow,
        (b'=', b'>') => TokenKind::FatArrow,
        (b':', b':') => TokenKind::ColonColon,
        (b'=', b'=') => TokenKind::Eq,
        (b'!', b'=') => TokenKind::Neq,
        (b'<', b'=') => TokenKind::Le,
        (b'>', b'=') => TokenKind::Ge,
        (b'&', b'&') => TokenKind::AndAnd,
        (b'|', b'|') => TokenKind::OrOr,
        (b'<', b'<') => TokenKind::Shl,
        (b'>', b'>') => TokenKind::Shr,
        (b'!', b'{') => TokenKind::EffectOpen,
        (b'@', b'{') => TokenKind::CapOpen,
        _ => return None,
    })
}

fn single_char_ascii(b: u8) -> Option<TokenKind> {
    Some(match b {
        b'(' => TokenKind::LParen,
        b')' => TokenKind::RParen,
        b'{' => TokenKind::LBrace,
        b'}' => TokenKind::RBrace,
        b'[' => TokenKind::LBracket,
        b']' => TokenKind::RBracket,
        b',' => TokenKind::Comma,
        b';' => TokenKind::Semicolon,
        b':' => TokenKind::Colon,
        b'.' => TokenKind::Dot,
        b'+' => TokenKind::Plus,
        b'-' => TokenKind::Minus,
        b'*' => TokenKind::Star,
        b'/' => TokenKind::Slash,
        b'%' => TokenKind::Percent,
        b'=' => TokenKind::Assign,
        b'<' => TokenKind::Lt,
        b'>' => TokenKind::Gt,
        b'!' => TokenKind::Bang,
        b'&' => TokenKind::Amp,
        b'|' => TokenKind::Pipe,
        b'^' => TokenKind::Caret,
        b'?' => TokenKind::Question,
        b'@' => TokenKind::At,
        b'#' => TokenKind::Hash,
        b'$' => TokenKind::LinearMark,
        b'~' => TokenKind::AffineMark,
        _ => return None,
    })
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

    fn scan(s: &str) -> OpScan {
        scan_op(file(), s, 0, AsciiMode::UnicodeAllowed).expect("expected op")
    }

    fn scan_ascii(s: &str) -> OpScan {
        scan_op(file(), s, 0, AsciiMode::AsciiOnly).expect("expected op")
    }

    // ── ASCII longest-match ────────────────────────────────────────────

    #[test]
    fn longest_match_eq_vs_assign() {
        let r = scan("==");
        assert_eq!(r.kind, TokenKind::Eq);
        assert_eq!(r.byte_len, 2);

        let r = scan("=x");
        assert_eq!(r.kind, TokenKind::Assign);
        assert_eq!(r.byte_len, 1);
    }

    #[test]
    fn longest_match_arrow_vs_minus() {
        let r = scan("->");
        assert_eq!(r.kind, TokenKind::Arrow);
        assert_eq!(r.byte_len, 2);

        let r = scan("-x");
        assert_eq!(r.kind, TokenKind::Minus);
        assert_eq!(r.byte_len, 1);
    }

    #[test]
    fn longest_match_fat_arrow_vs_assign() {
        let r = scan("=>");
        assert_eq!(r.kind, TokenKind::FatArrow);
        assert_eq!(r.byte_len, 2);
    }

    #[test]
    fn longest_match_colon_colon_vs_colon() {
        let r = scan("::");
        assert_eq!(r.kind, TokenKind::ColonColon);
        assert_eq!(r.byte_len, 2);

        let r = scan(":x");
        assert_eq!(r.kind, TokenKind::Colon);
        assert_eq!(r.byte_len, 1);
    }

    // ── Effect / capability brackets ───────────────────────────────────

    #[test]
    fn effect_bracket_open() {
        let r = scan("!{io}");
        assert_eq!(r.kind, TokenKind::EffectOpen);
        assert_eq!(r.byte_len, 2);
    }

    #[test]
    fn capability_bracket_open() {
        let r = scan("@{cap}");
        assert_eq!(r.kind, TokenKind::CapOpen);
        assert_eq!(r.byte_len, 2);
    }

    // ── Single-char punctuation ────────────────────────────────────────

    #[test]
    fn parens() {
        assert_eq!(scan("(").kind, TokenKind::LParen);
        assert_eq!(scan(")").kind, TokenKind::RParen);
    }

    #[test]
    fn braces_and_brackets() {
        assert_eq!(scan("{").kind, TokenKind::LBrace);
        assert_eq!(scan("}").kind, TokenKind::RBrace);
        assert_eq!(scan("[").kind, TokenKind::LBracket);
        assert_eq!(scan("]").kind, TokenKind::RBracket);
    }

    #[test]
    fn comma_semicolon_dot() {
        assert_eq!(scan(",").kind, TokenKind::Comma);
        assert_eq!(scan(";").kind, TokenKind::Semicolon);
        assert_eq!(scan(".").kind, TokenKind::Dot);
    }

    #[test]
    fn arithmetic_operators() {
        for (s, k) in [
            ("+", TokenKind::Plus),
            ("-", TokenKind::Minus),
            ("*", TokenKind::Star),
            ("/", TokenKind::Slash),
            ("%", TokenKind::Percent),
        ] {
            assert_eq!(scan(s).kind, k, "input {s:?}");
        }
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(scan("<").kind, TokenKind::Lt);
        assert_eq!(scan(">").kind, TokenKind::Gt);
        assert_eq!(scan("<=").kind, TokenKind::Le);
        assert_eq!(scan(">=").kind, TokenKind::Ge);
        assert_eq!(scan("!=").kind, TokenKind::Neq);
    }

    #[test]
    fn logical_and_bit_ops() {
        assert_eq!(scan("&").kind, TokenKind::Amp);
        assert_eq!(scan("|").kind, TokenKind::Pipe);
        assert_eq!(scan("&&").kind, TokenKind::AndAnd);
        assert_eq!(scan("||").kind, TokenKind::OrOr);
        assert_eq!(scan("<<").kind, TokenKind::Shl);
        assert_eq!(scan(">>").kind, TokenKind::Shr);
        assert_eq!(scan("^").kind, TokenKind::Caret);
    }

    #[test]
    fn other_single_char() {
        assert_eq!(scan("?").kind, TokenKind::Question);
        assert_eq!(scan("@").kind, TokenKind::At);
        assert_eq!(scan("#").kind, TokenKind::Hash);
        assert_eq!(scan("!").kind, TokenKind::Bang);
    }

    #[test]
    fn substructural_markers_ascii() {
        assert_eq!(scan("$").kind, TokenKind::LinearMark);
        assert_eq!(scan("~").kind, TokenKind::AffineMark);
    }

    // ── Unicode glyphs (§4) ────────────────────────────────────────────

    #[test]
    fn unicode_arrow() {
        let r = scan("→x");
        assert_eq!(r.kind, TokenKind::Arrow);
        assert_eq!(r.byte_len, "→".len() as u32);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn unicode_fat_arrow() {
        let r = scan("↦x");
        assert_eq!(r.kind, TokenKind::FatArrow);
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn unicode_double_colon() {
        let r = scan("∷x");
        assert_eq!(r.kind, TokenKind::ColonColon);
    }

    #[test]
    fn unicode_linear_consume() {
        let r = scan("↓x");
        assert_eq!(r.kind, TokenKind::LinearMark);
    }

    // ── Roundtrip: every glyph table entry's Unicode form yields the
    //              same TokenKind as its ASCII form. ─────────────────
    #[test]
    fn unicode_ascii_roundtrip() {
        for entry in glyph_table::GLYPHS {
            if let Some(uni) = entry.unicode {
                let ascii_kind = scan(entry.ascii).kind;
                let unicode_kind = scan(uni).kind;
                assert_eq!(
                    ascii_kind, unicode_kind,
                    "mismatch for {} <-> {}",
                    entry.ascii, uni
                );
                assert_eq!(unicode_kind, entry.kind);
            }
        }
    }

    // ── ASCII-only mode ────────────────────────────────────────────────

    #[test]
    fn ascii_only_mode_emits_e0010_on_unicode_glyph() {
        let r = scan_ascii("↓x");
        assert_eq!(r.kind, TokenKind::LinearMark);
        let d = r.diagnostic.expect("E0010 expected");
        assert_eq!(d.code().number(), 10);
    }

    #[test]
    fn ascii_only_mode_passes_ascii_through_clean() {
        let r = scan_ascii("->");
        assert_eq!(r.kind, TokenKind::Arrow);
        assert!(r.diagnostic.is_none());
    }

    // ── Non-operators return None ──────────────────────────────────────

    #[test]
    fn identifier_byte_returns_none() {
        assert!(scan_op(file(), "abc", 0, AsciiMode::UnicodeAllowed).is_none());
    }

    #[test]
    fn digit_returns_none() {
        assert!(scan_op(file(), "42", 0, AsciiMode::UnicodeAllowed).is_none());
    }

    #[test]
    fn quote_returns_none() {
        assert!(scan_op(file(), "'a'", 0, AsciiMode::UnicodeAllowed).is_none());
    }
}
