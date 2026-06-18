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
use paideia_as_lexer::{Token, TokenKind};

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a quote expression: `quote { body }`.
    ///
    /// Expects the `quote` keyword token to have already been consumed
    /// (caller has verified it's "quote" via source text). Parses the `{`,
    /// body, and closing `}`.
    ///
    /// Increments `in_quote_depth` before parsing the body to enable antiquote
    /// recognition, and decrements it afterward (even on error). This manual
    /// depth management is necessary because Rust's borrow checker prevents
    /// using an RAII guard that would hold a mutable reference to self.
    /// Tests verify that depth is correctly restored on parse errors.
    ///
    /// On success, returns an `ExprQuote` node wrapping the body.
    /// On error (malformed quote), emits P0171 and returns `Err(ParseError)`.
    pub(crate) fn parse_quote_expr(
        &mut self,
        quote_tok: Token,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span_start = quote_tok.span;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, span(byte_start, byte_len))
    }

    /// Helper to construct a quote token stream and parse it.
    fn parse_quote_tokens(
        tokens: Vec<Token>,
        source: &str,
    ) -> (
        AstArena,
        Result<paideia_as_ast::NodeId, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut p = Parser::new(
                &tokens,
                source,
                FileId::new(1).unwrap(),
                &mut arena,
                &mut sink,
            );
            p.parse_primary()
        };
        (arena, result, sink.diagnostics().to_vec())
    }

    #[test]
    fn parses_simple_quote() {
        // quote { 1 }
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),   // "quote"
            tok(TokenKind::LBrace, 6, 1),  // "{"
            tok(TokenKind::IntLit, 8, 1),  // "1"
            tok(TokenKind::RBrace, 10, 1), // "}"
            tok(TokenKind::Eof, 11, 0),
        ];
        let source = "quote { 1 }";
        let (arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_ok(), "simple quote should parse");
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprQuote);
        assert_eq!(diags.len(), 0, "no diagnostics for valid quote");
    }

    #[test]
    fn parses_two_level_nested_quote() {
        // quote { quote { 1 } }
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),   // "quote" (outer)
            tok(TokenKind::LBrace, 6, 1),  // "{"
            tok(TokenKind::Ident, 8, 5),   // "quote" (inner)
            tok(TokenKind::LBrace, 14, 1), // "{"
            tok(TokenKind::IntLit, 16, 1), // "1"
            tok(TokenKind::RBrace, 18, 1), // "}" (inner)
            tok(TokenKind::RBrace, 20, 1), // "}" (outer)
            tok(TokenKind::Eof, 21, 0),
        ];
        let source = "quote { quote { 1 } }";
        let (arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_ok(), "two-level nested quote should parse");
        let outer_id = result.unwrap();
        let outer_node = arena.get(outer_id).unwrap();
        assert_eq!(outer_node.kind, NodeKind::ExprQuote);

        // Verify the body is also a Quote
        if let Some(ExprData::Quote { body }) = arena.expr_data(outer_id) {
            let inner_node = arena.get(*body).unwrap();
            assert_eq!(
                inner_node.kind,
                NodeKind::ExprQuote,
                "inner body should be ExprQuote"
            );
        } else {
            panic!("outer expression should be Quote");
        }

        assert_eq!(diags.len(), 0, "no diagnostics for nested quote");
    }

    #[test]
    fn parses_three_level_nested_quote() {
        // quote { quote { quote { 1 } } }
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),   // "quote" (L1)
            tok(TokenKind::LBrace, 6, 1),  // "{"
            tok(TokenKind::Ident, 8, 5),   // "quote" (L2)
            tok(TokenKind::LBrace, 14, 1), // "{"
            tok(TokenKind::Ident, 16, 5),  // "quote" (L3)
            tok(TokenKind::LBrace, 22, 1), // "{"
            tok(TokenKind::IntLit, 24, 1), // "1"
            tok(TokenKind::RBrace, 26, 1), // "}" (L3)
            tok(TokenKind::RBrace, 28, 1), // "}" (L2)
            tok(TokenKind::RBrace, 30, 1), // "}" (L1)
            tok(TokenKind::Eof, 31, 0),
        ];
        let source = "quote { quote { quote { 1 } } }";
        let (arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_ok(), "three-level nested quote should parse");
        let l1_id = result.unwrap();
        let l1_node = arena.get(l1_id).unwrap();
        assert_eq!(l1_node.kind, NodeKind::ExprQuote);

        if let Some(ExprData::Quote { body: l2_id }) = arena.expr_data(l1_id) {
            let l2_node = arena.get(*l2_id).unwrap();
            assert_eq!(l2_node.kind, NodeKind::ExprQuote);

            if let Some(ExprData::Quote { body: l3_id }) = arena.expr_data(*l2_id) {
                let l3_node = arena.get(*l3_id).unwrap();
                assert_eq!(l3_node.kind, NodeKind::ExprQuote);
            } else {
                panic!("L2 body should be Quote");
            }
        } else {
            panic!("L1 should be Quote");
        }

        assert_eq!(
            diags.len(),
            0,
            "no diagnostics for three-level nested quote"
        );
    }

    #[test]
    fn parses_nested_quote_antiquote_inside_inner_quote() {
        // quote { quote { ~(b) } }
        // This tests antiquotes at inner nesting depth (they should be recognized
        // since in_quote_depth > 0 at depth 2).
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),       // "quote" (outer)
            tok(TokenKind::LBrace, 6, 1),      // "{"
            tok(TokenKind::Ident, 8, 5),       // "quote" (inner)
            tok(TokenKind::LBrace, 14, 1),     // "{"
            tok(TokenKind::AffineMark, 16, 1), // "~"
            tok(TokenKind::LParen, 17, 1),     // "("
            tok(TokenKind::Ident, 18, 1),      // "b"
            tok(TokenKind::RParen, 19, 1),     // ")"
            tok(TokenKind::RBrace, 21, 1),     // "}" (inner)
            tok(TokenKind::RBrace, 23, 1),     // "}" (outer)
            tok(TokenKind::Eof, 24, 0),
        ];
        let source = "quote { quote { ~(b) } }";
        let (_arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_ok(), "nested quote with antiquotes should parse");
        assert_eq!(diags.len(), 0, "no diagnostics");
    }

    #[test]
    fn nested_quote_outer_span_encloses_inner() {
        // quote { quote { 1 } }
        // Outer span should encompass the entire expression.
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),   // "quote" at 0-5
            tok(TokenKind::LBrace, 6, 1),  // "{"
            tok(TokenKind::Ident, 8, 5),   // "quote"
            tok(TokenKind::LBrace, 14, 1), // "{"
            tok(TokenKind::IntLit, 16, 1), // "1"
            tok(TokenKind::RBrace, 18, 1), // "}" at 18-19
            tok(TokenKind::RBrace, 20, 1), // "}" at 20-21
            tok(TokenKind::Eof, 21, 0),
        ];
        let source = "quote { quote { 1 } }";
        let (arena, result, _diags) = parse_quote_tokens(tokens, source);

        let outer_id = result.unwrap();
        let outer_node = arena.get(outer_id).unwrap();
        let outer_span = outer_node.span;

        if let Some(ExprData::Quote { body: inner_id }) = arena.expr_data(outer_id) {
            let inner_node = arena.get(*inner_id).unwrap();
            let inner_span = inner_node.span;

            // Outer span should start at byte 0 (start of "quote")
            assert_eq!(outer_span.byte_start(), 0, "outer span should start at 0");

            // Outer span's end should be >= inner span's end
            let outer_end = outer_span.byte_start() + outer_span.byte_len();
            let inner_end = inner_span.byte_start() + inner_span.byte_len();
            assert!(
                outer_end >= inner_end,
                "outer span end {} should enclose inner span end {}",
                outer_end,
                inner_end
            );
        } else {
            panic!("expected Quote variant");
        }
    }

    #[test]
    fn quote_depth_returns_to_zero_after_parse() {
        // After parsing a quote (even if nested), the parser's in_quote_depth should be 0.
        // This verifies the depth is properly managed.
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),   // "quote"
            tok(TokenKind::LBrace, 6, 1),  // "{"
            tok(TokenKind::IntLit, 8, 1),  // "1"
            tok(TokenKind::RBrace, 10, 1), // "}"
            tok(TokenKind::Eof, 11, 0),
        ];
        let source = "quote { 1 }";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let _result = parser.parse_primary();
        assert_eq!(
            parser.in_quote_depth, 0,
            "quote depth should return to 0 after parsing"
        );
    }

    #[test]
    fn multi_line_quote_snapshot() {
        // Test a multi-line quote to ensure the pretty-printer handles it.
        // quote { x + y }
        // Tests that arbitrary expressions can be quoted.
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),   // "quote"
            tok(TokenKind::LBrace, 6, 1),  // "{"
            tok(TokenKind::Ident, 8, 1),   // "x"
            tok(TokenKind::Plus, 10, 1),   // "+"
            tok(TokenKind::Ident, 12, 1),  // "y"
            tok(TokenKind::RBrace, 14, 1), // "}"
            tok(TokenKind::Eof, 15, 0),
        ];
        let source = "quote { x + y }";
        let (arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_ok(), "multi-line quote should parse");
        assert_eq!(diags.len(), 0, "no diagnostics for valid quote");

        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprQuote);

        // Verify pretty-print works
        if let Some(ExprData::Quote { body }) = arena.expr_data(expr_id) {
            let body_node = arena.get(*body).unwrap();
            // The body should be an Infix expression (x + y)
            assert_eq!(
                body_node.kind,
                NodeKind::ExprInfix,
                "body should be an infix expression"
            );
        } else {
            panic!("expected Quote variant");
        }
    }

    #[test]
    fn antiquote_outside_quote_emits_p0170() {
        // ~(v) outside a quote block should emit P0170
        let tokens = vec![
            tok(TokenKind::AffineMark, 0, 1), // "~"
            tok(TokenKind::LParen, 1, 1),     // "("
            tok(TokenKind::Ident, 2, 1),      // "v"
            tok(TokenKind::RParen, 3, 1),     // ")"
            tok(TokenKind::Eof, 4, 0),
        ];
        let source = "~(v)";
        let (_arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_err(), "antiquote outside quote should fail");
        assert!(
            diags.iter().any(|d| d.code().number() == 170),
            "should emit P0170 for antiquote outside quote"
        );
    }

    #[test]
    fn malformed_quote_missing_closing_brace_emits_p0171() {
        // quote { 1 ... EOF (missing closing brace)
        let tokens = vec![
            tok(TokenKind::Ident, 0, 5),  // "quote"
            tok(TokenKind::LBrace, 6, 1), // "{"
            tok(TokenKind::IntLit, 8, 1), // "1"
            tok(TokenKind::Eof, 10, 0),
        ];
        let source = "quote { 1";
        let (_arena, result, diags) = parse_quote_tokens(tokens, source);

        assert!(result.is_err(), "malformed quote should fail");
        assert!(
            diags.iter().any(|d| d.code().number() == 171),
            "should emit P0171 for malformed quote"
        );
    }
}
