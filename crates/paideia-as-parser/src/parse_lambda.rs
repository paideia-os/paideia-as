//! Lambda expression parsing.
//!
//! Implements §8 LambdaExpr grammar: both `fn` style and pipe-form lambdas.
//! - `fn` style: `fn (x: T) (y: U) -> body` with explicit parameter groups.
//! - Pipe form: `|x, y| body` with comma-separated identifiers.

use paideia_as_ast::{ExprData, NodeKind, PatternData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a lambda expression with `fn` keyword style.
    ///
    /// Form: `fn <T, U> (p1: T1) (p2: T2) -> expr`.
    /// Returns a `NodeKind::ExprLambda` with `pipe_form: false`.
    ///
    /// For phase-1:
    /// - Patterns inside `(... : T)` are treated as Ident patterns.
    /// - Types after `:` are parsed using the full type parser (PR-24).
    /// - Generic parameters are optional (added in phase-4 m9-001).
    pub(crate) fn parse_lambda_fn(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let fn_tok = self.expect(TokenKind::KwFn)?;
        let fn_span = fn_tok.span;

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        let mut params = Vec::new();

        // Parse zero or more parameter groups: (pat : ty) (pat : ty) ...
        loop {
            self.expect(TokenKind::LParen)?;

            // Check for empty parameter list: () -> ...
            if self.at(TokenKind::RParen) {
                self.expect(TokenKind::RParen)?;
                // Check for another parameter group
                if !self.at(TokenKind::LParen) {
                    break;
                }
                continue;
            }

            // Parse pattern (for phase-1, just accept Ident)
            let pattern = self.parse_pattern_atomic()?;
            params.push(pattern);

            self.expect(TokenKind::Colon)?;

            // Parse type using the full type parser
            let _ty = self.parse_type()?;
            // In a complete parser, we'd wrap the pattern and type together.
            // For phase-1, we store just the pattern; the type is parsed but
            // not currently attached to the pattern node. Document this.

            self.expect(TokenKind::RParen)?;

            // Check for another parameter group
            if !self.at(TokenKind::LParen) {
                break;
            }
        }

        // Optional `->` before the body (m3-002):
        // If the next token is `{`, treat it as a block body without explicit arrow.
        // Otherwise, expect `->` and parse the body as before.
        let body = if self.at(TokenKind::LBrace) {
            // Block body without arrow: fn (x: T) { ... }
            self.parse_expr()?
        } else {
            // Arrow present: fn (x: T) -> expr
            self.expect(TokenKind::Arrow)?;
            self.parse_expr()?
        };

        // Compute span covering the entire lambda
        let body_span = self.arena().get(body).map(|nd| nd.span).unwrap_or(fn_span);
        let lambda_span = Span::new(
            fn_span.file(),
            fn_span.byte_start(),
            body_span.byte_start() + body_span.byte_len() - fn_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLambda,
            lambda_span,
            ExprData::Lambda {
                generic_params,
                params,
                body,
                pipe_form: false,
            },
        ))
    }

    /// Parse a lambda expression with pipe-form syntax.
    ///
    /// Form: `|p1, p2, ...| expr`.
    /// Returns a `NodeKind::ExprLambda` with `pipe_form: true`.
    ///
    /// For phase-1:
    /// - Parameters are comma-separated identifiers (no type annotations).
    /// - Each identifier is parsed as an Ident pattern.
    /// - Generic parameters are NOT supported in pipe-form (always empty).
    pub(crate) fn parse_lambda_pipe(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let open_bar = self.expect(TokenKind::Pipe)?;
        let open_bar_span = open_bar.span;

        let mut params = Vec::new();

        // Parse comma-separated identifiers: ident, ident, ...
        loop {
            // Check for closing bar (empty params allowed: `|| expr`)
            if self.at(TokenKind::Pipe) {
                break;
            }

            let pattern = self.parse_pattern_atomic()?;
            params.push(pattern);

            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma
        }

        // Expect closing `|`
        self.expect(TokenKind::Pipe)?;

        // Parse body expression
        let body = self.parse_expr()?;

        // Compute span
        let body_span = self
            .arena()
            .get(body)
            .map(|nd| nd.span)
            .unwrap_or(open_bar_span);
        let lambda_span = Span::new(
            open_bar_span.file(),
            open_bar_span.byte_start(),
            body_span.byte_start() + body_span.byte_len() - open_bar_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLambda,
            lambda_span,
            ExprData::Lambda {
                generic_params: Vec::new(), // Pipe-form lambdas don't support generic params
                params,
                body,
                pipe_form: true,
            },
        ))
    }

    /// Parse a single pattern (atomic form for lambda parameters).
    ///
    /// For phase-1, only supports Ident patterns (including wildcard `_`).
    /// Returns a pattern node.
    fn parse_pattern_atomic(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        if let Some(tok) = self.peek() {
            let tok_kind = tok.kind;
            let span = tok.span;

            match tok_kind {
                TokenKind::Ident => {
                    self.bump();
                    let ident_id = self.arena_mut().alloc(NodeKind::Ident, span);
                    Ok(self.arena_mut().alloc_pattern(
                        NodeKind::PatIdent,
                        span,
                        PatternData::Ident {
                            name: ident_id,
                            mutable: false,
                        },
                    ))
                }
                _ => self.error_expected_lambda_pattern(),
            }
        } else {
            self.error_expected_lambda_pattern()
        }
    }

    /// Emit a P0100 ("expected pattern") diagnostic and return Err.
    fn error_expected_lambda_pattern(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            Span::new(self.file(), 0, 0)
        };
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                100,
            )
            .unwrap(),
        )
        .message("expected pattern".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::{Token, TokenKind};

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    fn parse(
        tokens: Vec<Token>,
    ) -> (
        AstArena,
        paideia_as_ast::NodeId,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr().expect("parse failed")
        };
        let diags = sink.diagnostics().to_vec();
        (arena, root, diags)
    }

    #[test]
    fn lambda_fn_one_param_typed() {
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::RParen, 9, 1),
            tok(TokenKind::Arrow, 11, 2),
            tok(TokenKind::IntLit, 14, 1), // 1
            tok(TokenKind::Eof, 15, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda {
                generic_params,
                params,
                pipe_form,
                ..
            } = expr_data
            {
                assert!(generic_params.is_empty());
                assert_eq!(params.len(), 1);
                assert!(!pipe_form);
            } else {
                panic!("expected ExprLambda");
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn lambda_fn_two_param_groups() {
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::RParen, 9, 1),
            tok(TokenKind::LParen, 11, 1),
            tok(TokenKind::Ident, 12, 1), // y
            tok(TokenKind::Colon, 13, 1),
            tok(TokenKind::Ident, 14, 3), // u64
            tok(TokenKind::RParen, 17, 1),
            tok(TokenKind::Arrow, 19, 2),
            tok(TokenKind::IntLit, 22, 1), // 1
            tok(TokenKind::Eof, 23, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda {
                generic_params: _,
                params,
                ..
            } = expr_data
            {
                assert_eq!(params.len(), 2);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_pipe_one_param() {
        let tokens = vec![
            tok(TokenKind::Pipe, 0, 1),
            tok(TokenKind::Ident, 1, 1), // x
            tok(TokenKind::Pipe, 2, 1),
            tok(TokenKind::IntLit, 4, 1), // 1
            tok(TokenKind::Eof, 5, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda {
                generic_params: _,
                params,
                pipe_form,
                ..
            } = expr_data
            {
                assert_eq!(params.len(), 1);
                assert!(pipe_form);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_pipe_two_params() {
        let tokens = vec![
            tok(TokenKind::Pipe, 0, 1),
            tok(TokenKind::Ident, 1, 1), // x
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::Ident, 4, 1), // y
            tok(TokenKind::Pipe, 5, 1),
            tok(TokenKind::Ident, 7, 1), // x
            tok(TokenKind::Plus, 9, 1),
            tok(TokenKind::Ident, 11, 1), // y
            tok(TokenKind::Eof, 12, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda {
                generic_params: _,
                params,
                pipe_form,
                ..
            } = expr_data
            {
                assert_eq!(params.len(), 2);
                assert!(pipe_form);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_body_with_call() {
        let tokens = vec![
            tok(TokenKind::Pipe, 0, 1),
            tok(TokenKind::Ident, 1, 1), // x
            tok(TokenKind::Pipe, 2, 1),
            tok(TokenKind::Ident, 4, 1), // f
            tok(TokenKind::LParen, 5, 1),
            tok(TokenKind::Ident, 6, 1), // x
            tok(TokenKind::RParen, 7, 1),
            tok(TokenKind::Eof, 8, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { body, .. } = expr_data {
                let body_node = arena.get(*body).unwrap();
                // Body should be a call expression
                assert_eq!(body_node.kind, NodeKind::ExprCall);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    // m3-002 tests: optional arrow before block body

    #[test]
    fn fn_block_body_arrow_elided() {
        // fn (x: i32) { x } (no arrow, block body)
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 7, 3), // i32
            tok(TokenKind::RParen, 10, 1),
            tok(TokenKind::LBrace, 12, 1),
            tok(TokenKind::Ident, 14, 1), // x
            tok(TokenKind::RBrace, 15, 1),
            tok(TokenKind::Eof, 16, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda {
                body, pipe_form, ..
            } = expr_data
            {
                assert!(!pipe_form);
                let body_node = arena.get(*body).unwrap();
                assert_eq!(body_node.kind, NodeKind::ExprBlock);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn fn_block_body_arrow_present() {
        // fn (x: i32) -> { x } (arrow present, block body)
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 7, 3), // i32
            tok(TokenKind::RParen, 10, 1),
            tok(TokenKind::Arrow, 12, 2),
            tok(TokenKind::LBrace, 15, 1),
            tok(TokenKind::Ident, 17, 1), // x
            tok(TokenKind::RBrace, 18, 1),
            tok(TokenKind::Eof, 19, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda {
                body, pipe_form, ..
            } = expr_data
            {
                assert!(!pipe_form);
                let body_node = arena.get(*body).unwrap();
                assert_eq!(body_node.kind, NodeKind::ExprBlock);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn fn_arrow_then_record_constructor_unchanged() {
        // fn (x: i32) -> Foo { x: 1 } (record constructor on Foo, unchanged)
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 7, 3), // i32
            tok(TokenKind::RParen, 10, 1),
            tok(TokenKind::Arrow, 12, 2),
            tok(TokenKind::Ident, 15, 3), // Foo
            tok(TokenKind::LBrace, 19, 1),
            tok(TokenKind::Ident, 21, 1), // x
            tok(TokenKind::Colon, 22, 1),
            tok(TokenKind::IntLit, 24, 1), // 1
            tok(TokenKind::RBrace, 25, 1),
            tok(TokenKind::Eof, 26, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { body, .. } = expr_data {
                let body_node = arena.get(*body).unwrap();
                // Body should be a record constructor (ExprRecordCons)
                assert_eq!(body_node.kind, NodeKind::ExprRecordCons);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn fn_arrow_present_non_block_body() {
        // fn (x: i32) -> x + 1 (arrow required for non-block expression)
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 7, 3), // i32
            tok(TokenKind::RParen, 10, 1),
            tok(TokenKind::Arrow, 12, 2),
            tok(TokenKind::Ident, 15, 1), // x
            tok(TokenKind::Plus, 17, 1),
            tok(TokenKind::IntLit, 19, 1), // 1
            tok(TokenKind::Eof, 20, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { body, .. } = expr_data {
                let body_node = arena.get(*body).unwrap();
                // Body should be an infix expression (x + 1)
                assert_eq!(body_node.kind, NodeKind::ExprInfix);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }
}
