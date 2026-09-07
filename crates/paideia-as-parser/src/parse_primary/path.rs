//! Path / identifier parsing (`Ident (:: Ident)*`) plus the two contextual
//! keywords `quote` and `uninit`.
//!
//! Extracted from `parse_primary/mod.rs` (issue #1407 God-file split).
//! Kept as `pub(crate)` because `modules.rs`, `parse_handler.rs`, etc.
//! call `Parser::parse_path_or_ident` cross-module.

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a path or single identifier.
    ///
    /// Path syntax: `Ident (:: Ident)*`.
    /// Also dispatches to `parse_quote_expr` if the identifier is the
    /// contextual keyword "quote" followed by `{`, or returns an ExprUninit
    /// if the identifier is the contextual keyword "uninit".
    /// Returns an ExprPath node with segments, or an ExprQuote/ExprUninit on those contexts.
    pub(crate) fn parse_path_or_ident(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let first_tok = self.expect(TokenKind::Ident)?;
        let span_start = first_tok.span;

        // Check if this is the contextual keyword "quote" followed by `{` or "uninit"
        let source = self.source();
        let start = first_tok.span.byte_start() as usize;
        let end = (first_tok.span.byte_start() + first_tok.span.byte_len()) as usize;
        let ident_lexeme = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        if ident_lexeme == "quote" && self.peek().is_some_and(|t| t.kind == TokenKind::LBrace) {
            return self.parse_quote_expr(first_tok);
        }

        if ident_lexeme == "uninit" {
            return Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprUninit,
                span_start,
                ExprData::Uninit,
            ));
        }

        // Otherwise, parse as a normal path
        let mut segments = vec![self.arena_mut().alloc(NodeKind::Ident, span_start)];
        let mut span_end = span_start;

        while self.at(TokenKind::ColonColon) {
            self.bump(); // consume `::`

            let ident_tok = self.expect(TokenKind::Ident)?;
            span_end = ident_tok.span;
            segments.push(self.arena_mut().alloc(NodeKind::Ident, ident_tok.span));
        }

        // Compute the span covering the entire path.
        let path_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_expr(NodeKind::ExprPath, path_span, ExprData::Path { segments }))
    }
}
