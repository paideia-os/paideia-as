//! Top-level lexer driver per `syntax-reference.md` §2.
//!
//! The [`Lexer`] scans source text into a stream of [`Token`]s. Whitespace
//! and comments are collected as [`Trivia`] and can be retrieved separately
//! via [`Lexer::take_trivia`].
//!
//! Error recovery follows §2.4: when an unrecognized byte is encountered,
//! a diagnostic is emitted and the lexer continues by advancing past the
//! offending character.

use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, FileId, Severity, Span,
};
use unicode_ident::is_xid_start;

use crate::source::SourceText;
use crate::token::{Token, TokenKind};
use crate::trivia::Trivia;
#[cfg(test)]
use crate::trivia::TriviaKind;
use crate::{
    AsciiMode, scan_char, scan_comment, scan_identifier, scan_number, scan_op, scan_string,
    scan_whitespace,
};

/// Top-level lexer for paideia-as source text.
///
/// The lexer produces a stream of tokens by scanning `content` byte-by-byte.
/// Whitespace and comments are accumulated as trivia and can be retrieved
/// separately via [`Lexer::take_trivia`].
///
/// The lexer does NOT own a `SourceMap` — the caller is responsible for
/// wiring `FileId` and `SourceMap` together.
pub struct Lexer<'a> {
    /// File ID for spans emitted by this lexer.
    file: FileId,
    /// The source text being lexed.
    content: &'a str,
    /// Current byte offset into `content`.
    cursor: u32,
    /// ASCII-only mode for operator scanning (see [`AsciiMode`]).
    ascii_mode: AsciiMode,
    /// Trivia accumulated since the last token.
    trivia: Vec<Trivia>,
    /// Set to `true` after a `DiagnosticOverflow` is encountered;
    /// subsequent calls to `next_token` return `Eof`.
    bailed: bool,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer for the given source.
    ///
    /// Initializes with [`AsciiMode::UnicodeAllowed`].
    ///
    /// # Arguments
    ///
    /// - `file`: The `FileId` to embed in token spans.
    /// - `source`: The validated source text.
    #[must_use]
    pub fn new(file: FileId, source: &'a SourceText) -> Self {
        Self::with_ascii_mode(file, source, AsciiMode::UnicodeAllowed)
    }

    /// Creates a new lexer with explicit ASCII-only mode.
    ///
    /// # Arguments
    ///
    /// - `file`: The `FileId` to embed in token spans.
    /// - `source`: The validated source text.
    /// - `mode`: ASCII mode (see [`AsciiMode`]).
    #[must_use]
    pub fn with_ascii_mode(file: FileId, source: &'a SourceText, mode: AsciiMode) -> Self {
        Self {
            file,
            content: source.content(),
            cursor: 0,
            ascii_mode: mode,
            trivia: Vec::new(),
            bailed: false,
        }
    }

    /// Returns the next token, emitting any diagnostics to `sink`.
    ///
    /// If the sink returns `DiagnosticOverflow`, the lexer stops emitting
    /// additional diagnostics and returns [`TokenKind::Eof`] on all subsequent
    /// calls.
    ///
    /// # Arguments
    ///
    /// - `sink`: A mutable reference to a diagnostic sink.
    pub fn next_token(&mut self, sink: &mut dyn DiagnosticSink) -> Token {
        // If we've already hit diagnostic overflow, return Eof early.
        if self.bailed {
            return Token::new(TokenKind::Eof, Span::new(self.file, self.cursor, 0));
        }

        loop {
            let cursor_usize = self.cursor as usize;

            // Check for end of input.
            if cursor_usize >= self.content.len() {
                return Token::new(TokenKind::Eof, Span::new(self.file, self.cursor, 0));
            }

            // Peek at the current byte.
            let byte = self.content.as_bytes()[cursor_usize];

            // Try whitespace first.
            if let Some(ws) = scan_whitespace(self.file, self.content, self.cursor) {
                if let Some(diag) = ws.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                    return Token::new(TokenKind::Eof, Span::new(self.file, self.cursor, 0));
                }
                self.trivia.push(Trivia::new(
                    ws.kind,
                    Span::new(self.file, self.cursor, ws.byte_len),
                ));
                self.cursor += ws.byte_len;
                continue;
            }

            // Try comment (must peek ahead for `/` + `/` or `/*`).
            if byte == b'/' && cursor_usize + 1 < self.content.len() {
                let next_byte = self.content.as_bytes()[cursor_usize + 1];
                if next_byte == b'/' || next_byte == b'*' {
                    let comment = scan_comment(self.file, self.content, self.cursor);
                    if let Some(diag) = comment.diagnostic
                        && sink.emit(*diag).is_err()
                    {
                        self.bailed = true;
                        return Token::new(TokenKind::Eof, Span::new(self.file, self.cursor, 0));
                    }
                    self.trivia.push(Trivia::new(
                        comment.kind,
                        Span::new(self.file, self.cursor, comment.byte_len),
                    ));
                    self.cursor += comment.byte_len;
                    continue;
                }
            }

            // Try lifetime parameter (e.g., 'a, 'b) before character literals.
            // A lifetime is `'` followed by an identifier-like sequence.
            // We distinguish from character literals by checking if the next char
            // is ASCII letter or underscore (potential identifier start).
            if byte == b'\'' && cursor_usize + 1 < self.content.len() {
                let next_byte = self.content.as_bytes()[cursor_usize + 1];
                // Check if this might be a lifetime: ' followed by identifier-like char
                if next_byte == b'_'
                    || (next_byte >= b'a' && next_byte <= b'z')
                    || (next_byte >= b'A' && next_byte <= b'Z')
                {
                    // Tentatively scan as a lifetime identifier
                    let ident_start = (cursor_usize + 1) as u32;
                    if ident_start < self.content.len() as u32 {
                        let ident_scan = scan_identifier(self.file, self.content, ident_start);
                        // A lifetime is only valid if followed by a non-identifier character
                        // or at end of input (not by another quote, which would make it a char literal)
                        let after_ident = cursor_usize + 1 + ident_scan.byte_len as usize;
                        let looks_like_lifetime = after_ident >= self.content.len()
                            || !matches!(self.content.as_bytes()[after_ident], b'\'');

                        if looks_like_lifetime && ident_scan.kind == TokenKind::Ident {
                            // This is a lifetime: scan the whole 'ident sequence
                            let lifetime_len = 1 + ident_scan.byte_len; // 1 for ', rest for ident
                            let span = Span::new(self.file, self.cursor, lifetime_len);
                            self.cursor += lifetime_len;
                            return Token::new(TokenKind::Ident, span); // Treat as Ident for now
                        }
                    }
                }
            }

            // Try character or byte literal.
            if byte == b'\'' {
                let char_scan = scan_char(self.file, self.content, self.cursor);
                if let Some(diag) = char_scan.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                }
                let span = Span::new(self.file, self.cursor, char_scan.byte_len);
                self.cursor += char_scan.byte_len;
                return Token::new(char_scan.kind, span);
            }

            // Try byte literal `b'…'`.
            if byte == b'b'
                && cursor_usize + 1 < self.content.len()
                && self.content.as_bytes()[cursor_usize + 1] == b'\''
            {
                let char_scan = scan_char(self.file, self.content, self.cursor);
                if let Some(diag) = char_scan.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                }
                let span = Span::new(self.file, self.cursor, char_scan.byte_len);
                self.cursor += char_scan.byte_len;
                return Token::new(char_scan.kind, span);
            }

            // Try string literal `"…"`.
            if byte == b'"' {
                let string_scan = scan_string(self.file, self.content, self.cursor);
                if let Some(diag) = string_scan.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                }
                let span = Span::new(self.file, self.cursor, string_scan.byte_len);
                self.cursor += string_scan.byte_len;
                return Token::new(string_scan.kind, span);
            }

            // Try raw string `r"…"` or `r#"…"#`.
            if byte == b'r' && cursor_usize + 1 < self.content.len() {
                let next_byte = self.content.as_bytes()[cursor_usize + 1];
                if next_byte == b'"' || next_byte == b'#' {
                    let string_scan = scan_string(self.file, self.content, self.cursor);
                    if let Some(diag) = string_scan.diagnostic
                        && sink.emit(*diag).is_err()
                    {
                        self.bailed = true;
                    }
                    let span = Span::new(self.file, self.cursor, string_scan.byte_len);
                    self.cursor += string_scan.byte_len;
                    return Token::new(string_scan.kind, span);
                }
            }

            // Try byte string `b"…"` or raw byte string `br"…"` / `rb"…"`.
            if byte == b'b' && cursor_usize + 1 < self.content.len() {
                let next_byte = self.content.as_bytes()[cursor_usize + 1];
                if next_byte == b'"' || next_byte == b'r' {
                    let string_scan = scan_string(self.file, self.content, self.cursor);
                    if let Some(diag) = string_scan.diagnostic
                        && sink.emit(*diag).is_err()
                    {
                        self.bailed = true;
                    }
                    let span = Span::new(self.file, self.cursor, string_scan.byte_len);
                    self.cursor += string_scan.byte_len;
                    return Token::new(string_scan.kind, span);
                }
            }

            // Try number (ASCII digit).
            if byte.is_ascii_digit() {
                let number = scan_number(self.file, self.content, self.cursor);
                if let Some(diag) = number.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                }
                let span = Span::new(self.file, self.cursor, number.byte_len);
                self.cursor += number.byte_len;
                return Token::new(number.kind, span);
            }

            // Try identifier (XID_Start, `_`, Unicode XID_Start, or backtick).
            if byte == b'_'
                || byte.is_ascii_alphabetic()
                || byte == b'`'
                || self.content[cursor_usize..]
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_ascii() && is_xid_start(c))
            {
                let ident = scan_identifier(self.file, self.content, self.cursor);
                if let Some(diag) = ident.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                }
                let span = Span::new(self.file, self.cursor, ident.byte_len);
                self.cursor += ident.byte_len;
                return Token::new(ident.kind, span);
            }

            // Try operator.
            if let Some(op) = scan_op(self.file, self.content, self.cursor, self.ascii_mode) {
                if let Some(diag) = op.diagnostic
                    && sink.emit(*diag).is_err()
                {
                    self.bailed = true;
                }
                let span = Span::new(self.file, self.cursor, op.byte_len);
                self.cursor += op.byte_len;
                return Token::new(op.kind, span);
            }

            // Unrecognized byte: emit E0011 (unrecognized character).
            let ch = self.content[cursor_usize..].chars().next().unwrap();
            let char_len = ch.len_utf8() as u32;
            let diag = Diagnostic::error(e_code(11))
                .message(format!(
                    "unrecognized character '{}'",
                    if ch.is_control() {
                        format!("U+{:04X}", ch as u32)
                    } else {
                        ch.to_string()
                    }
                ))
                .with_span(Span::new(self.file, self.cursor, char_len))
                .finish();

            if sink.emit(diag).is_err() {
                self.bailed = true;
                return Token::new(TokenKind::Eof, Span::new(self.file, self.cursor, 0));
            }

            self.cursor += char_len;
        }
    }

    /// Scans all remaining tokens until EOF.
    ///
    /// Convenience wrapper that calls [`next_token`](Lexer::next_token)
    /// in a loop until [`TokenKind::Eof`] is returned.
    pub fn collect_tokens(&mut self, sink: &mut dyn DiagnosticSink) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token(sink);
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    /// Returns and clears the trivia accumulated since the last token.
    pub fn take_trivia(&mut self) -> Vec<Trivia> {
        std::mem::take(&mut self.trivia)
    }
}

/// Helper to construct an E-code diagnostic code.
fn e_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::E, Severity::Error, n).expect("valid E code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::VecSink;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    fn lex(input: &str) -> (Vec<Token>, Vec<Trivia>) {
        let st = SourceText::from_bytes(file_id(), input.as_bytes()).unwrap();
        let mut lexer = Lexer::new(file_id(), &st);
        let mut sink = VecSink::new();
        let tokens = lexer.collect_tokens(&mut sink);
        let trivia = lexer.take_trivia();
        (tokens, trivia)
    }

    #[test]
    fn lex_simple_program() {
        let (tokens, _) = lex("let x = 42;");
        assert_eq!(tokens.len(), 6); // let, x, =, 42, ;, eof
        assert_eq!(tokens[0].kind, TokenKind::KwLet);
        assert_eq!(tokens[1].kind, TokenKind::Ident);
        assert_eq!(tokens[2].kind, TokenKind::Assign);
        assert_eq!(tokens[3].kind, TokenKind::IntLit);
        assert_eq!(tokens[4].kind, TokenKind::Semicolon);
        assert_eq!(tokens[5].kind, TokenKind::Eof);
    }

    #[test]
    fn lex_with_trivia_preserved() {
        let st = SourceText::from_bytes(file_id(), b"// comment\nlet x").unwrap();
        let mut lexer = Lexer::new(file_id(), &st);
        let mut sink = VecSink::new();

        // Consume trivia before first token.
        let _ = lexer.next_token(&mut sink);
        let trivia = lexer.take_trivia();

        // We should have collected the comment and newline as trivia.
        assert!(trivia.iter().any(|t| t.kind == TriviaKind::LineComment));
        assert!(trivia.iter().any(|t| t.kind == TriviaKind::Newline));
    }

    #[test]
    fn lex_recovers_from_unknown_byte() {
        let (tokens, _) = lex("§ let x");
        // Should have: error token indicator + KwLet, Ident, Eof
        // The lexer emits a diagnostic but continues.
        assert_eq!(tokens[0].kind, TokenKind::KwLet);
        assert_eq!(tokens[1].kind, TokenKind::Ident);
        assert_eq!(tokens[2].kind, TokenKind::Eof);
    }

    #[test]
    fn lex_bails_after_100_errors() {
        // Build a source with 150 invalid tokens (150 unknown chars).
        let mut src = String::new();
        for _ in 0..150 {
            src.push('§');
            src.push(' ');
        }

        let st = SourceText::from_bytes(file_id(), src.as_bytes()).unwrap();
        let mut lexer = Lexer::new(file_id(), &st);
        let mut sink = VecSink::with_policy(paideia_as_diagnostics::BailPolicy::cap(100));

        let tokens = lexer.collect_tokens(&mut sink);

        // Sink should have 101 errors: the cap is 100, but the 101st error
        // that triggers overflow is still recorded before the error is returned.
        assert_eq!(sink.error_count(), 101);
        // The last token should be Eof (early bailout).
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn token_spans_contiguous() {
        // Test with a program with no whitespace so tokens are contiguous.
        let (tokens, _) = lex("letx=1");
        // Collect all spans and verify they cover the entire source contiguously.
        let mut covered = 0u32;
        for token in &tokens {
            if token.kind != TokenKind::Eof {
                assert_eq!(token.span.byte_start(), covered);
                covered += token.span.byte_len();
            }
        }
        // We should have covered the entire source.
        assert!(covered > 0);
    }

    #[test]
    fn lex_unicode_identifier() {
        let (tokens, _) = lex("家族 + 1");
        assert_eq!(tokens.len(), 4); // ident, +, 1, eof
        assert_eq!(tokens[0].kind, TokenKind::Ident);
        assert_eq!(tokens[1].kind, TokenKind::Plus);
        assert_eq!(tokens[2].kind, TokenKind::IntLit);
        assert_eq!(tokens[3].kind, TokenKind::Eof);
    }

    #[test]
    fn handle_lexes_as_ident() {
        // Verify that "handle" is now a contextual keyword (Ident, not KwHandle).
        let (tokens, _) = lex("handle effect { }");
        assert_eq!(tokens.len(), 5); // handle, effect, {, }, eof
        // handle should now be an Ident, not a reserved keyword
        assert_eq!(tokens[0].kind, TokenKind::Ident);
        assert_eq!(tokens[1].kind, TokenKind::Ident);
        assert_eq!(tokens[2].kind, TokenKind::LBrace);
        assert_eq!(tokens[3].kind, TokenKind::RBrace);
        assert_eq!(tokens[4].kind, TokenKind::Eof);
    }
}
