//! Quote and antiquote parsing: `quote { ... }` and `~(...)`.
//!
//! Quotation is a metaprogramming facility that captures abstract syntax trees
//! as first-class values. Antiquotation (`~(...)`) splices computed values
//! into quoted expressions.
//!
//! Reserved P-codes (P0170–P0179) for future extensions:
//! - P0170: antiquote outside quote block
//! - P0171: malformed quote (missing closing brace)
//! - P0172–P0179: reserved for future use

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a quote expression: `quote { body }`.
    ///
    /// Consumes the `quote` keyword and `{`, parses the body, and expects `}`.
    /// Increments `in_quote_depth` for the duration of parsing the body so
    /// that antiquotes inside are properly recognized.
    ///
    /// On success, returns an `ExprQuote` node wrapping the body.
    /// On error (malformed quote), emits P0171 and returns `Err(ParseError)`.
    pub fn parse_quote_expr(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let quote_tok = self.expect(TokenKind::Ident)?;
        let span_start = quote_tok.span;

        // Verify the token is contextually "quote" by checking source text
        let source = self.source();
        let start = quote_tok.span.byte_start() as usize;
        let end = (quote_tok.span.byte_start() + quote_tok.span.byte_len()) as usize;
        let quote_lexeme = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        if quote_lexeme != "quote" {
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 100).expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message("expected contextual keyword 'quote'")
                .with_span(quote_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Expect `{`
        self.expect(TokenKind::LBrace)?;

        // Increment quote depth to enable antiquote recognition
        self.in_quote_depth = self.in_quote_depth.saturating_add(1);

        // Parse the body expression
        let body_id = self.parse_expr();

        // Decrement quote depth (always, even on error)
        self.in_quote_depth = self.in_quote_depth.saturating_sub(1);

        let body_id = body_id?;

        // Expect `}`
        let rbrace_span = self
            .expect(TokenKind::RBrace)
            .map_err(|_| {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 171)
                    .expect("valid P0171 code");
                let span = self.current_span();
                let diag = Diagnostic::error(code)
                    .message("malformed quote: expected `}` to close `quote { … }`")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                ParseError
            })?
            .span;

        // Compute span from quote keyword to closing brace
        let quote_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_span.byte_start() + rbrace_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprQuote,
            quote_span,
            ExprData::Quote { body: body_id },
        ))
    }

    /// Parse an antiquote expression: `~(value)`.
    ///
    /// Validates that we are inside a `quote { ... }` block (checks `in_quote_depth > 0`).
    /// If not, emits P0170 and returns `Err(ParseError)`.
    /// On success, parses the value expression and returns an `ExprAntiquote` node.
    ///
    /// # Panics
    ///
    /// Panics if `in_quote_depth` overflows (saturating arithmetic prevents this).
    pub fn parse_antiquote_expr(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let tilde_tok = self.expect(TokenKind::AffineMark)?;
        let span_start = tilde_tok.span;

        // Check if we are inside a quote block
        if self.in_quote_depth == 0 {
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 170).expect("valid P0170 code");
            let diag = Diagnostic::error(code)
                .message("antiquote `~(...)` outside of a `quote { ... }` block")
                .with_span(span_start)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Expect `(`
        self.expect(TokenKind::LParen)?;

        // Parse the value expression
        let value_id = self.parse_expr()?;

        // Expect `)`
        let rparen_span = self.expect(TokenKind::RParen)?.span;

        // Compute span from tilde to closing paren
        let antiquote_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprAntiquote,
            antiquote_span,
            ExprData::Antiquote { value: value_id },
        ))
    }
}
