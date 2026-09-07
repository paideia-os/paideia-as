//! Bracketed / parenthesized primary expressions: array literals
//! (`[e, ...]`, `[e; count]`), the unit literal `()`, single-expression
//! parenthesized groupings, and the (stubbed-as-Placeholder) tuple form
//! `(a, b, c)`.
//!
//! Extracted from `parse_primary/mod.rs` (issue #1407 God-file split).
//! Both entry points share a delimiter-recovery flow that reports
//! P0101 on mismatched close, and array-empty reports P0210.

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Diagnostic, Span};
use paideia_as_lexer::TokenKind;

use super::p_code;
use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse an array literal: `[expr1, expr2, ..., exprN]` or `[expr1, expr2, ...,]`.
    ///
    /// Algorithm:
    /// 1. Expect `[`.
    /// 2. If `]` immediately follows, emit P0210 and return Err (empty array without type annotation).
    /// 3. Otherwise, parse comma-separated expressions until `]`.
    /// 4. Allow trailing comma before the closing `]`.
    /// 5. Return ExprArrayLit with the list of element node IDs.
    ///
    /// Returns an ExprArrayLit node with `ArrayLit(Vec<NodeId>)`.
    pub(super) fn parse_array_lit(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lbracket_tok = self.expect(TokenKind::LBracket)?;
        let span_start = lbracket_tok.span;

        // Check for empty array literal: `[]`
        if self.at(TokenKind::RBracket) {
            // Empty array requires explicit type annotation — emit P0210
            let span = span_start;
            let diag = Diagnostic::error(p_code(210))
                .message("empty array literal requires explicit type annotation".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Parse first element
        let first_elem = self.parse_expr()?;

        // Check for repeat syntax: `[expr; count]`
        if self.at(TokenKind::Semicolon) {
            self.bump(); // consume semicolon
            let count_expr = self.parse_expr()?;

            // Expect closing bracket
            if !self.at(TokenKind::RBracket) {
                return self.error_mismatched_delimiter(span_start);
            }
            let rbracket_tok = self.expect(TokenKind::RBracket)?;
            let rbracket_span = rbracket_tok.span;

            // Compute span from `[` to `]`
            let array_span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbracket_span.byte_start() + rbracket_span.byte_len() - span_start.byte_start(),
            );

            // Allocate an ArrayRepeat node. The elaborator will expand this during lowering
            // by evaluating the count literal and replicating the expr.
            return Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprArrayRepeat,
                array_span,
                ExprData::ArrayRepeat {
                    expr: first_elem,
                    count: count_expr,
                },
            ));
        }

        // Normal comma-separated array literal
        let mut elements = vec![first_elem];

        loop {
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma

            // Check for trailing comma before closing bracket
            if self.at(TokenKind::RBracket) {
                break;
            }

            elements.push(self.parse_expr()?);
        }

        // Expect closing bracket
        if !self.at(TokenKind::RBracket) {
            return self.error_mismatched_delimiter(span_start);
        }
        let rbracket_tok = self.expect(TokenKind::RBracket)?;
        let rbracket_span = rbracket_tok.span;

        // Compute span from `[` to `]`
        let array_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbracket_span.byte_start() + rbracket_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprArrayLit,
            array_span,
            ExprData::ArrayLit(elements),
        ))
    }

    /// Parse parenthesized expressions: `()`, `(expr)`, or `(a, b, c)`.
    ///
    /// - `()` allocates a Placeholder and wraps it in ExprLiteral.
    /// - `(expr)` returns the inner expression (parens are syntactic sugar).
    /// - `(a, b, c)` allocates a Placeholder node (tuples deferred to a later PR).
    pub(super) fn parse_paren_expr(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Check for empty parens: `()`
        if self.at(TokenKind::RParen) {
            self.bump();
            let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, lparen_span);
            return Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprLiteral,
                lparen_span,
                ExprData::Literal { lit: lit_id },
            ));
        }

        // Parse the first expression with full infix/prefix/postfix support
        let first_expr = self.parse_expr()?;

        // Check for comma: tuple case or parenthesized single expr?
        if self.at(TokenKind::Comma) {
            // Tuple: collect remaining elements
            let mut _elements = vec![first_expr];

            while self.at(TokenKind::Comma) {
                self.bump(); // consume comma

                // Check for trailing comma before closing paren
                if self.at(TokenKind::RParen) {
                    break;
                }

                _elements.push(self.parse_expr()?);
            }

            if !self.at(TokenKind::RParen) {
                return self.error_mismatched_delimiter(lparen_span);
            }
            let rparen_tok = self.expect(TokenKind::RParen)?;
            let rparen_span = rparen_tok.span;

            // Allocate tuple as Placeholder (deferred to future PR).
            // Span covers the entire tuple, from `(` to `)`.
            let tuple_span = Span::new(
                lparen_span.file(),
                lparen_span.byte_start(),
                rparen_span.byte_start() + rparen_span.byte_len() - lparen_span.byte_start(),
            );

            return Ok(self.arena_mut().alloc(NodeKind::Placeholder, tuple_span));
        }

        // Parenthesized single expression: expect RParen and return inner expr
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        self.bump(); // consume `)`
        Ok(first_expr)
    }
}
