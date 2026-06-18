//! With-handler expression parsing.
//!
//! Implements §8 WithHandlerExpr grammar: `with handler-expr handle name block`.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a with-handler expression: `with handler-expr handle name block`.
    ///
    /// **Algorithm:**
    /// 1. Expect `KwWith`.
    /// 2. Parse handler expression via `parse_expr()`.
    /// 3. Expect `KwHandle`.
    /// 4. Expect `Ident` (the bound name).
    /// 5. Parse block via `parse_block()`.
    /// 6. Allocate `ExprData::WithHandler { handler, bind, block }`.
    ///
    /// Returns the `NodeId` of the allocated expression on success.
    pub(crate) fn parse_with_handler(&mut self) -> Result<NodeId, ParseError> {
        let with_tok = self.expect(TokenKind::KwWith)?;
        let span_start = with_tok.span;

        // Parse the handler expression
        let handler = self.parse_expr()?;

        // Expect `handle`
        self.expect(TokenKind::KwHandle)?;

        // Expect binding identifier
        let bind_tok = self.expect(TokenKind::Ident)?;
        let bind = self.arena_mut().alloc(NodeKind::Ident, bind_tok.span);

        // Parse the block
        let block = self.parse_block()?;

        let block_span = self
            .arena()
            .get(block)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        let span = paideia_as_diagnostics::Span::new(
            span_start.file(),
            span_start.byte_start(),
            block_span.byte_start() + block_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprWithHandler,
            span,
            ExprData::WithHandler {
                handler,
                bind,
                block,
            },
        ))
    }
}

// Tests will be in integration tests; parse_with_handler is internal to the parser module.
