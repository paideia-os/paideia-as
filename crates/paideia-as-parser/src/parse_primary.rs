//! Primary expression parsing: literals, identifiers, paths, and parenthesized
//! expressions.
//!
//! Primary expressions are the atoms of the syntax tree — the base cases that
//! other expression categories build upon. This module implements the `parse_primary`
//! method that dispatches on token kind and constructs the appropriate AST nodes.

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a primary expression (atom).
    ///
    /// Dispatches on the current token kind:
    /// - **Literals** (IntLit, FloatLit, CharLit, StringLit, ByteLit, ByteStringLit):
    ///   allocate a Placeholder node for the literal, wrap in ExprLiteral.
    /// - **Boolean keywords** (KwTrue, KwFalse, KwNull):
    ///   allocate synthetic Placeholder nodes, wrap in ExprLiteral.
    /// - **Identifiers**: parse as a path with segments separated by `::`.
    /// - **KwSelfType / KwSelfValue**: treat as a single-segment path.
    /// - **LParen**: disambiguate between `()` (unit), `(expr)` (parenthesized),
    ///   and `(a, b, c)` (tuple; currently stubbed as Placeholder).
    /// - **Otherwise**: emit P0100 "expected expression" and return Err.
    ///
    /// Note: Block expressions, lambdas, and control-flow constructs are
    /// dispatched in `parse_expr_bp` Step 0, before primary parsing.
    ///
    /// On parse failure, returns `Err(ParseError)` after emitting a diagnostic.
    /// The caller is responsible for calling [`Parser::recover_to_one_of`] to
    /// synchronize if needed.
    ///
    /// Returns the `NodeId` of the allocated expression on success.
    pub fn parse_primary(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        match self.peek() {
            None => self.error_expected_expression(),

            Some(tok) => {
                let tok_kind = tok.kind;
                let span_start = tok.span;

                match tok_kind {
                    // Numeric and character literals
                    TokenKind::IntLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::FloatLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::CharLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::StringLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::ByteLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::ByteStringLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    // Boolean and null constants
                    // Note: We allocate synthetic Placeholder nodes for true/false/null.
                    // A future PR will add dedicated NodeKind variants (BoolLit, NullLit).
                    TokenKind::KwTrue => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::KwFalse => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::KwNull => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    // Effect operations
                    TokenKind::KwPerform => self.parse_perform(),
                    TokenKind::KwResume => self.parse_resume(),
                    TokenKind::KwHandle => self.parse_handler_value(),

                    // Antiquotation (only if followed by `(`)
                    TokenKind::AffineMark
                        if self.peek_at(1).is_some_and(|t| t.kind == TokenKind::LParen) =>
                    {
                        self.parse_antiquote_expr()
                    }

                    // Identifiers and paths (including contextual keyword "quote")
                    TokenKind::Ident => self.parse_path_or_ident(),

                    TokenKind::KwSelfType | TokenKind::KwSelfValue => {
                        self.bump();
                        let ident_id = self.arena_mut().alloc(NodeKind::Ident, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprPath,
                            span_start,
                            ExprData::Path {
                                segments: vec![ident_id],
                            },
                        ))
                    }

                    // Parenthesized expressions and tuples
                    TokenKind::LParen => self.parse_paren_expr(),

                    // Anything else is an error
                    // (Block expressions are handled in parse_expr_bp Step 0)
                    _ => self.error_expected_expression(),
                }
            }
        }
    }

    /// Parse a path or single identifier.
    ///
    /// Path syntax: `Ident (:: Ident)*`.
    /// Also dispatches to `parse_quote_expr` if the identifier is the
    /// contextual keyword "quote" followed by `{`.
    /// Returns an ExprPath node with segments, or an ExprQuote on quote syntax.
    pub(crate) fn parse_path_or_ident(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let first_tok = self.expect(TokenKind::Ident)?;
        let span_start = first_tok.span;

        // Check if this is the contextual keyword "quote" followed by `{`
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

    /// Parse parenthesized expressions: `()`, `(expr)`, or `(a, b, c)`.
    ///
    /// - `()` allocates a Placeholder and wraps it in ExprLiteral.
    /// - `(expr)` returns the inner expression (parens are syntactic sugar).
    /// - `(a, b, c)` allocates a Placeholder node (tuples deferred to a later PR).
    fn parse_paren_expr(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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

    /// Parse a perform expression: `perform Effect::op(args)`.
    ///
    /// Algorithm:
    /// 1. Expect `KwPerform`.
    /// 2. Parse a path (Ident (:: Ident)*).
    /// 3. Expect `LParen`.
    /// 4. Parse comma-separated argument expressions until `RParen`.
    /// 5. Allocate ExprData::Perform { op_path, args }.
    fn parse_perform(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
    fn parse_resume(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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

    /// Emit a P0100 ("expected expression") diagnostic and return `Err(ParseError)`.
    fn error_expected_expression(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            // At EOF: use a zero-width span at byte 0
            Span::new(self.file(), 0, 0)
        };

        let diag = Diagnostic::error(p_code(100))
            .message("expected expression".to_string())
            .with_span(span)
            .finish();
        self.emit_diagnostic(diag);

        Err(ParseError)
    }

    /// Emit a P0101 ("mismatched delimiter") diagnostic and return `Err(ParseError)`.
    ///
    /// Called when an opening paren/brace has no matching closing paren/brace.
    fn error_mismatched_delimiter(
        &mut self,
        _opening_span: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            Span::new(self.file(), 0, 0)
        };

        let diag = Diagnostic::error(p_code(101))
            .message("mismatched delimiter: expected `)`".to_string())
            .with_span(span)
            .finish();
        self.emit_diagnostic(diag);

        Err(ParseError)
    }
}

/// Construct a P-category diagnostic code at the given number, returning
/// the `DiagnosticCode`.
fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    #[test]
    fn parses_int_literal() {
        let tokens = vec![tok(TokenKind::IntLit, 0, 2), tok(TokenKind::Eof, 2, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        // Verify it's an ExprLiteral
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_string_literal() {
        let tokens = vec![tok(TokenKind::StringLit, 0, 5), tok(TokenKind::Eof, 5, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_char_literal() {
        let tokens = vec![tok(TokenKind::CharLit, 0, 3), tok(TokenKind::Eof, 3, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_bool_true() {
        let tokens = vec![tok(TokenKind::KwTrue, 0, 4), tok(TokenKind::Eof, 4, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_bool_false() {
        let tokens = vec![tok(TokenKind::KwFalse, 0, 5), tok(TokenKind::Eof, 5, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_simple_identifier() {
        let tokens = vec![tok(TokenKind::Ident, 0, 4), tok(TokenKind::Eof, 4, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPath);
    }

    #[test]
    fn parses_path_of_three_segments() {
        let tokens = vec![
            tok(TokenKind::Ident, 0, 2),      // "a"
            tok(TokenKind::ColonColon, 2, 2), // "::"
            tok(TokenKind::Ident, 4, 2),      // "b"
            tok(TokenKind::ColonColon, 6, 2), // "::"
            tok(TokenKind::Ident, 8, 2),      // "c"
            tok(TokenKind::Eof, 10, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPath);
    }

    #[test]
    fn parses_parenthesized_expression() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::IntLit, 1, 2),
            tok(TokenKind::RParen, 3, 1),
            tok(TokenKind::Eof, 4, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_unit_literal() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::RParen, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_tuple_three_elements() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            tok(TokenKind::Comma, 4, 1),
            tok(TokenKind::IntLit, 5, 1),
            tok(TokenKind::RParen, 6, 1),
            tok(TokenKind::Eof, 7, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::Placeholder);
    }

    #[test]
    fn mismatched_paren_emits_p0101() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            // Missing RParen
            tok(TokenKind::Eof, 4, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_err());
        assert_eq!(sink.diagnostics().len(), 1);
        let diag = &sink.diagnostics()[0];
        assert_eq!(diag.code().number(), 101);
    }

    #[test]
    fn parses_empty_block_rejected() {
        // Empty blocks are not allowed; must have a tail expression.
        // Per #156 requirement: emits P0157 and returns Err.
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::RBrace, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        // Note: blocks are now parsed in parse_expr_bp Step 0, not in parse_primary
        let result = parser.parse_expr();
        assert!(result.is_err(), "empty block should parse error");

        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 157),
            "expected P0157 diagnostic (empty block)"
        );
    }

    #[test]
    fn parses_perform_basic() {
        // perform Io::port_read(0x60)
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 9),
            tok(TokenKind::LParen, 21, 1),
            tok(TokenKind::IntLit, 22, 3),
            tok(TokenKind::RParen, 25, 1),
            tok(TokenKind::Eof, 26, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPerform);
    }

    #[test]
    fn parses_perform_zero_args() {
        // perform Io::flush()
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 5),
            tok(TokenKind::LParen, 17, 1),
            tok(TokenKind::RParen, 18, 1),
            tok(TokenKind::Eof, 19, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPerform);
        if let Some(ExprData::Perform { args, .. }) = arena.expr_data(expr_id) {
            assert_eq!(args.len(), 0);
        } else {
            panic!("expected Perform variant");
        }
    }

    #[test]
    fn parses_perform_multi_args() {
        // perform Io::port_write(0x64, 0xED)
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 10),
            tok(TokenKind::LParen, 22, 1),
            tok(TokenKind::IntLit, 23, 3),
            tok(TokenKind::Comma, 26, 1),
            tok(TokenKind::IntLit, 28, 3),
            tok(TokenKind::RParen, 31, 1),
            tok(TokenKind::Eof, 32, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        if let Some(ExprData::Perform { args, .. }) = arena.expr_data(expr_id) {
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected Perform variant");
        }
    }

    #[test]
    fn parses_perform_path_three_segments() {
        // perform Mod::Io::read(addr)
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 3),
            tok(TokenKind::ColonColon, 11, 2),
            tok(TokenKind::Ident, 13, 2),
            tok(TokenKind::ColonColon, 15, 2),
            tok(TokenKind::Ident, 17, 4),
            tok(TokenKind::LParen, 21, 1),
            tok(TokenKind::Ident, 22, 4),
            tok(TokenKind::RParen, 26, 1),
            tok(TokenKind::Eof, 27, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        if let Some(ExprData::Perform { op_path, .. }) = arena.expr_data(expr_id) {
            if let Some(ExprData::Path { segments }) = arena.expr_data(*op_path) {
                assert_eq!(segments.len(), 3);
            } else {
                panic!("expected Path for op_path");
            }
        } else {
            panic!("expected Perform variant");
        }
    }

    #[test]
    fn perform_missing_paren_emits_p0161() {
        // perform Io::flush ... missing (
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 5),
            tok(TokenKind::Eof, 17, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_err());
        assert_eq!(sink.diagnostics().len(), 1);
        let diag = &sink.diagnostics()[0];
        assert_eq!(diag.code().number(), 161);
    }

    #[test]
    fn parses_resume_value() {
        // resume v
        let tokens = vec![
            tok(TokenKind::KwResume, 0, 6),
            tok(TokenKind::Ident, 7, 1),
            tok(TokenKind::Eof, 8, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprResume);
        if let Some(ExprData::Resume { value }) = arena.expr_data(expr_id) {
            let value_node = arena.get(*value).unwrap();
            assert_eq!(value_node.kind, NodeKind::ExprPath);
        } else {
            panic!("expected Resume variant");
        }
    }

    #[test]
    fn parses_resume_unit() {
        // resume ()
        let tokens = vec![
            tok(TokenKind::KwResume, 0, 6),
            tok(TokenKind::LParen, 7, 1),
            tok(TokenKind::RParen, 8, 1),
            tok(TokenKind::Eof, 9, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprResume);
    }
}
