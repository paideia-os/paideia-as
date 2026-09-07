//! Algebraic-effect primary expressions: `perform Effect::op(args)` and
//! `resume value`.
//!
//! Extracted from `parse_primary/mod.rs` (issue #1407 God-file split).
//! Both helpers are `pub(super)` — reachable only from `parse_primary`'s
//! dispatch table.

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Diagnostic, Span};
use paideia_as_lexer::TokenKind;

use super::p_code;
use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a perform expression: `perform Effect::op(args)`.
    ///
    /// Algorithm:
    /// 1. Expect `KwPerform`.
    /// 2. Parse a path (Ident (:: Ident)*).
    /// 3. Expect `LParen`.
    /// 4. Parse comma-separated argument expressions until `RParen`.
    /// 5. Allocate ExprData::Perform { op_path, args }.
    pub(super) fn parse_perform(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let perform_tok = self.expect(TokenKind::KwPerform)?;
        let span_start = perform_tok.span;

        // Parse the effect operation path (e.g., `Io::port_read`)
        let op_path = self.parse_path_or_ident()?;

        // Expect `(`
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(161))
                .message("expected `(` after effect-operation path".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Parse arguments: comma-separated expressions
        let mut args = vec![];
        if !self.at(TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume comma

                // Check for trailing comma
                if self.at(TokenKind::RParen) {
                    break;
                }
            }
        }

        // Expect `)`
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        let rparen_tok = self.expect(TokenKind::RParen)?;
        let rparen_span = rparen_tok.span;

        // Compute span from `perform` keyword through closing `)`
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprPerform,
            span,
            ExprData::Perform { op_path, args },
        ))
    }

    /// Parse a resume expression: `resume value`.
    ///
    /// Algorithm:
    /// 1. Expect `KwResume`.
    /// 2. Parse a full expression.
    /// 3. Allocate ExprData::Resume { value }.
    pub(super) fn parse_resume(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let resume_tok = self.expect(TokenKind::KwResume)?;
        let span_start = resume_tok.span;

        // Parse the value expression with full infix/prefix/postfix support
        let value = self.parse_expr()?;

        let value_span = self
            .arena()
            .get(value)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        // Compute span from `resume` keyword through the value
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            value_span.byte_start() + value_span.byte_len() - span_start.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_expr(NodeKind::ExprResume, span, ExprData::Resume { value }))
    }
}
