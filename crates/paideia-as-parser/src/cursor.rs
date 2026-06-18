//! Token cursor used by the parser.
//!
//! `TokenCursor` is a non-owning view into a slice of [`Token`]s with
//! a single `pos` index. Peek and bump are O(1); cloning the cursor is
//! a `Copy` since it only stores a slice reference and an index.

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_lexer::{Token, TokenKind};

/// Read-only cursor over a token slice.
#[derive(Copy, Clone, Debug)]
pub struct TokenCursor<'a> {
    tokens: &'a [Token],
    pos: usize,
    file: FileId,
}

impl<'a> TokenCursor<'a> {
    /// Construct a cursor positioned at the first token of `tokens`.
    #[must_use]
    pub fn new(tokens: &'a [Token], file: FileId) -> Self {
        Self {
            tokens,
            pos: 0,
            file,
        }
    }

    /// File this cursor's tokens belong to.
    #[must_use]
    pub fn file(self) -> FileId {
        self.file
    }

    /// Current position (index of the next token to consume).
    #[must_use]
    pub fn position(self) -> usize {
        self.pos
    }

    /// Returns the current token without advancing. `None` past EOF.
    #[must_use]
    pub fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    /// Returns the token at `self.pos + n` without advancing.
    #[must_use]
    pub fn peek_at(&self, n: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + n)
    }

    /// Returns the kind of the current token, or `TokenKind::Eof` if past
    /// the end of the slice.
    #[must_use]
    pub fn current_kind(&self) -> TokenKind {
        self.peek().map_or(TokenKind::Eof, |t| t.kind)
    }

    /// `true` if the current token's kind matches `kind`.
    #[must_use]
    pub fn at(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    /// Advance to the next token and return what was there. Returns
    /// `None` if already past the end.
    pub fn bump(&mut self) -> Option<Token> {
        let tok = self.peek().cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    /// Span that points at the current token's source range, or a
    /// zero-length span at the end-of-input if we're past the slice.
    #[must_use]
    pub fn current_span(&self) -> Span {
        self.peek().map_or_else(
            || {
                let end = self
                    .tokens
                    .last()
                    .map_or(0, |t| t.span.byte_start() + t.span.byte_len());
                Span::new(self.file, end, 0)
            },
            |t| t.span,
        )
    }

    /// Span of the previously consumed token, or a zero-length span
    /// at position 0 if no token has been consumed yet.
    #[must_use]
    pub fn previous_span(&self) -> Span {
        if self.pos == 0 {
            Span::new(self.file, 0, 0)
        } else {
            self.tokens
                .get(self.pos - 1)
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file, 0, 0))
        }
    }

    /// `true` if the cursor is past the last token (or the next token is
    /// `Eof`).
    #[must_use]
    pub fn is_at_end(&self) -> bool {
        self.peek().is_none() || self.at(TokenKind::Eof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, span(byte_start, byte_len))
    }

    #[test]
    fn peek_does_not_advance() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3), tok(TokenKind::Ident, 4, 1)];
        let c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        assert_eq!(c.peek().unwrap().kind, TokenKind::KwLet);
        assert_eq!(c.position(), 0);
    }

    #[test]
    fn bump_advances() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3), tok(TokenKind::Ident, 4, 1)];
        let mut c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        let popped = c.bump().unwrap();
        assert_eq!(popped.kind, TokenKind::KwLet);
        assert_eq!(c.position(), 1);
        assert_eq!(c.peek().unwrap().kind, TokenKind::Ident);
    }

    #[test]
    fn bump_past_end_returns_none() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3)];
        let mut c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        c.bump().unwrap();
        assert!(c.bump().is_none());
    }

    #[test]
    fn at_kind() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3)];
        let c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        assert!(c.at(TokenKind::KwLet));
        assert!(!c.at(TokenKind::KwFn));
    }

    #[test]
    fn current_kind_returns_eof_past_end() {
        let toks: Vec<Token> = vec![];
        let c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        assert_eq!(c.current_kind(), TokenKind::Eof);
    }

    #[test]
    fn peek_at_lookahead() {
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::Ident, 4, 1),
            tok(TokenKind::Assign, 6, 1),
        ];
        let c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        assert_eq!(c.peek_at(0).unwrap().kind, TokenKind::KwLet);
        assert_eq!(c.peek_at(1).unwrap().kind, TokenKind::Ident);
        assert_eq!(c.peek_at(2).unwrap().kind, TokenKind::Assign);
        assert!(c.peek_at(3).is_none());
    }

    #[test]
    fn current_span_at_end_is_zero_length() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3)];
        let mut c = TokenCursor::new(&toks, FileId::new(1).unwrap());
        c.bump();
        let s = c.current_span();
        assert_eq!(s.byte_len(), 0);
        assert_eq!(s.byte_start(), 3);
    }
}
