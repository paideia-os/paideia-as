//! Token-stream pattern scanner.
//!
//! Walks the lexer output for the 12-token direct-UART pattern and produces
//! one [`Match`] per hit. Trivia (comments, whitespace) between adjacent
//! tokens is transparent because the lexer already collects it separately.
//! Matches inside comments or string literals cannot occur: comments become
//! `Trivia` and strings become a single `StringLit` token whose interior is
//! never traversed.

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_lexer::{Lexer, SourceText, Token, TokenKind};

/// A single successful pattern match.
///
/// `byte_start` .. `byte_end` covers the full 12-token span in the source
/// (inclusive of the trailing `;` of `call uart_puts;`, exclusive of any
/// following newline). `msg_symbol` is the captured identifier at position
/// 6, e.g. `banner_msg`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Match {
    /// Byte offset of the first character of `lea` in the source.
    pub byte_start: usize,
    /// Byte offset one past the last character (`;`) in the source.
    pub byte_end: usize,
    /// The captured msg symbol name, verbatim.
    pub msg_symbol: String,
}

/// Scan a source buffer for every direct-UART pattern.
///
/// Returns matches in source order (ascending `byte_start`). Malformed
/// sources still produce best-effort matches — the lexer error-recovers
/// past unrecognised bytes, and any partial pattern simply fails to
/// match.
pub fn scan(source: &str) -> Vec<Match> {
    let file = FileId::new(1).expect("FileId(1) is non-zero");
    // paideia-as-lexer expects a validated SourceText. If the input isn't
    // valid UTF-8 or is empty, we return no matches — this is a source-in
    // rewriter, not a diagnostic tool.
    let src = match SourceText::from_bytes(file, source.as_bytes()) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut lexer = Lexer::new(file, &src);
    let mut sink = SilentSink::default();
    let tokens = lexer.collect_tokens(&mut sink);
    scan_tokens(source, &tokens)
}

fn scan_tokens(source: &str, tokens: &[Token]) -> Vec<Match> {
    let mut out = Vec::new();
    // The pattern is 12 tokens. Window from 0..len-12+1; be defensive.
    if tokens.len() < 12 {
        return out;
    }
    let mut i = 0;
    while i + 12 <= tokens.len() {
        if let Some(m) = try_match(source, &tokens[i..i + 12]) {
            out.push(m);
            i += 12;
        } else {
            i += 1;
        }
    }
    out
}

fn try_match(source: &str, w: &[Token]) -> Option<Match> {
    debug_assert_eq!(w.len(), 12);
    // 0: Ident "lea"
    ident_text_eq(source, &w[0], "lea")?;
    // 1: Ident "rdi"
    ident_text_eq(source, &w[1], "rdi")?;
    // 2: Comma
    kind_eq(&w[2], TokenKind::Comma)?;
    // 3: LBracket
    kind_eq(&w[3], TokenKind::LBracket)?;
    // 4: Ident "rip"
    ident_text_eq(source, &w[4], "rip")?;
    // 5: Plus
    kind_eq(&w[5], TokenKind::Plus)?;
    // 6: Ident (captured msg symbol)
    if w[6].kind != TokenKind::Ident {
        return None;
    }
    let msg_symbol = span_text(source, w[6].span).to_owned();
    // 7: RBracket
    kind_eq(&w[7], TokenKind::RBracket)?;
    // 8: Semicolon
    kind_eq(&w[8], TokenKind::Semicolon)?;
    // 9: Ident "call"
    ident_text_eq(source, &w[9], "call")?;
    // 10: Ident "uart_puts"
    ident_text_eq(source, &w[10], "uart_puts")?;
    // 11: Semicolon
    kind_eq(&w[11], TokenKind::Semicolon)?;

    Some(Match {
        byte_start: w[0].span.byte_start() as usize,
        byte_end: w[11].span.byte_end() as usize,
        msg_symbol,
    })
}

fn kind_eq(tok: &Token, kind: TokenKind) -> Option<()> {
    if tok.kind == kind { Some(()) } else { None }
}

fn ident_text_eq(source: &str, tok: &Token, expected: &str) -> Option<()> {
    if tok.kind != TokenKind::Ident {
        return None;
    }
    if span_text(source, tok.span) == expected {
        Some(())
    } else {
        None
    }
}

fn span_text(source: &str, span: Span) -> &str {
    let start = span.byte_start() as usize;
    let end = span.byte_end() as usize;
    &source[start..end]
}

/// A `DiagnosticSink` that silently swallows every diagnostic. Migration
/// is a source-in / source-out operation; parse errors are the caller's
/// concern (they surface at `paideia-as build` time).
#[derive(Default)]
struct SilentSink {
    count: usize,
    errors: usize,
}

impl paideia_as_diagnostics::DiagnosticSink for SilentSink {
    fn emit(
        &mut self,
        d: paideia_as_diagnostics::Diagnostic,
    ) -> Result<(), paideia_as_diagnostics::DiagnosticOverflow> {
        self.count += 1;
        if d.severity() == paideia_as_diagnostics::Severity::Error {
            self.errors += 1;
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.count
    }

    fn error_count(&self) -> usize {
        self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Vec<Match> {
        scan(src)
    }

    #[test]
    fn empty_source_is_empty() {
        assert!(one("").is_empty());
    }

    #[test]
    fn single_pattern_matches() {
        let src = "lea rdi, [rip + banner_msg]; call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
        assert_eq!(ms[0].byte_start, 0);
        // Match ends at the ';' after uart_puts.
        assert_eq!(ms[0].byte_end, src.trim_end_matches('\n').len());
    }

    #[test]
    fn multiline_pattern_matches() {
        let src = "      lea rdi, [rip + idt_ok_msg];\n      call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "idt_ok_msg");
    }

    #[test]
    fn lea_without_call_does_not_match() {
        let src = "lea rdi, [rip + banner_msg]; nop;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn call_without_lea_does_not_match() {
        let src = "mov rax, 1; call uart_puts;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn wrong_register_does_not_match() {
        // lea rsi, not rdi
        let src = "lea rsi, [rip + banner_msg]; call uart_puts;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn comment_is_not_matched() {
        let src = "// lea rdi, [rip + banner_msg]; call uart_puts;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn string_literal_is_not_matched() {
        let src = r#"let x : str = "lea rdi, [rip + banner_msg]; call uart_puts;""#;
        assert!(one(src).is_empty());
    }

    #[test]
    fn two_adjacent_patterns_match() {
        let src = "lea rdi, [rip + a_msg]; call uart_puts;\n\
                   lea rdi, [rip + b_msg]; call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].msg_symbol, "a_msg");
        assert_eq!(ms[1].msg_symbol, "b_msg");
        assert!(ms[0].byte_end <= ms[1].byte_start);
    }

    #[test]
    fn different_call_target_does_not_match() {
        // call uart_putc, not uart_puts
        let src = "lea rdi, [rip + banner_msg]; call uart_putc;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn trailing_comment_still_matches() {
        let src = "lea rdi, [rip + banner_msg]; call uart_puts;  // OK marker\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
    }
}
