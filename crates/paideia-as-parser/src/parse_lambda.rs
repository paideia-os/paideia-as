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

        // Parse zero or more parameter groups: (pat : ty, ...) (pat : ty, ...) ...
        // Each group can contain comma-separated parameters.
        'outer: loop {
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

            // Parse comma-separated parameters within this group
            'inner: loop {
                // Parse pattern (for phase-1, just accept Ident)
                let pattern = self.parse_pattern_atomic()?;
                params.push(pattern);

                self.expect(TokenKind::Colon)?;

                // Parse type using the full type parser
                let ty = self.parse_type()?;
                // Store the pattern → type mapping in the arena's hints table
                self.arena_mut()
                    .pattern_type_hints_mut()
                    .insert(pattern, ty);

                // Check for comma (continue inner loop) or RParen (exit inner loop)
                if self.at(TokenKind::Comma) {
                    self.bump(); // consume comma
                    // Check if next token is RParen (trailing comma case)
                    if self.at(TokenKind::RParen) {
                        break 'inner;
                    }
                    // Otherwise continue parsing the next parameter in this group
                    continue 'inner;
                } else {
                    // No comma, must be RParen
                    break 'inner;
                }
            }

            // Arity check: if more than 6 params, emit P0276 and return Err
            if params.len() > 6 {
                // The 7th parameter is at index 6
                let seventh_param = params[6];
                let seventh_span = self
                    .arena()
                    .get(seventh_param)
                    .map(|nd| nd.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let diag = paideia_as_diagnostics::Diagnostic::error(
                    paideia_as_diagnostics::DiagnosticCode::new(
                        paideia_as_diagnostics::Category::P,
                        paideia_as_diagnostics::Severity::Error,
                        276,
                    )
                    .unwrap(),
                )
                .message("lambda has more than 6 parameters".to_string())
                .with_span(seventh_span)
                .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }

            self.expect(TokenKind::RParen)?;

            // Check for another parameter group
            if !self.at(TokenKind::LParen) {
                break 'outer;
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

    /// Parse a zero-parameter lambda from `||` (OrOr token).
    ///
    /// When the lexer scans `||`, it produces a single `OrOr` token (logical-or operator).
    /// At expr-start positions, we need to recognize this as a zero-parameter lambda `|| body`
    /// and parse it as such.
    ///
    /// This method is called when we see `OrOr` at an expr-start position and handles
    /// it as if it were two separate `Pipe` tokens: one opening, one closing (empty params).
    pub(crate) fn parse_lambda_pipe_from_oror(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let oror_tok = self.bump().expect("caller ensured OrOr token is present");
        let open_bar_span = oror_tok.span;

        // OrOr represents || (two pipes), so we've already seen the opening pipe.
        // The closing pipe is implicit (part of OrOr).
        // Zero parameters → parse body directly.
        let params = Vec::new();

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
                generic_params: Vec::new(),
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

    // New tests for multi-parameter flat syntax (Issue #1041)

    #[test]
    fn lambda_fn_two_params_flat_comma_separated() {
        // fn (a: u64, b: u64) -> 1
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // a
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::Comma, 9, 1),
            tok(TokenKind::Ident, 11, 1), // b
            tok(TokenKind::Colon, 12, 1),
            tok(TokenKind::Ident, 13, 3), // u64
            tok(TokenKind::RParen, 16, 1),
            tok(TokenKind::Arrow, 18, 2),
            tok(TokenKind::IntLit, 21, 1), // 1
            tok(TokenKind::Eof, 22, 0),
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
                assert_eq!(params.len(), 2, "should have 2 params");
                assert!(!pipe_form);
            } else {
                panic!("expected ExprLambda");
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn lambda_fn_three_params_flat() {
        // fn (a: u64, b: u64, c: u64) -> 1
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // a
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::Comma, 9, 1),
            tok(TokenKind::Ident, 11, 1), // b
            tok(TokenKind::Colon, 12, 1),
            tok(TokenKind::Ident, 13, 3), // u64
            tok(TokenKind::Comma, 16, 1),
            tok(TokenKind::Ident, 18, 1), // c
            tok(TokenKind::Colon, 19, 1),
            tok(TokenKind::Ident, 20, 3), // u64
            tok(TokenKind::RParen, 23, 1),
            tok(TokenKind::Arrow, 25, 2),
            tok(TokenKind::IntLit, 28, 1), // 1
            tok(TokenKind::Eof, 29, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { params, .. } = expr_data {
                assert_eq!(params.len(), 3);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_fn_trailing_comma() {
        // fn (a: u64, b: u64,) -> 1
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // a
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::Comma, 9, 1),
            tok(TokenKind::Ident, 11, 1), // b
            tok(TokenKind::Colon, 12, 1),
            tok(TokenKind::Ident, 13, 3), // u64
            tok(TokenKind::Comma, 16, 1), // trailing comma
            tok(TokenKind::RParen, 17, 1),
            tok(TokenKind::Arrow, 19, 2),
            tok(TokenKind::IntLit, 22, 1), // 1
            tok(TokenKind::Eof, 23, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "trailing comma should be accepted");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { params, .. } = expr_data {
                assert_eq!(params.len(), 2);
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_fn_empty_params() {
        // fn () -> 42
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::RParen, 4, 1),
            tok(TokenKind::Arrow, 6, 2),
            tok(TokenKind::IntLit, 9, 2), // 42
            tok(TokenKind::Eof, 11, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "empty params should be accepted");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { params, .. } = expr_data {
                assert!(params.is_empty(), "should have 0 params");
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_fn_mixed_curried_and_flat() {
        // fn (a: u64, b: u64) (c: u64) -> 1
        // Should have 3 params total
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // a
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::Comma, 9, 1),
            tok(TokenKind::Ident, 11, 1), // b
            tok(TokenKind::Colon, 12, 1),
            tok(TokenKind::Ident, 13, 3), // u64
            tok(TokenKind::RParen, 16, 1),
            tok(TokenKind::LParen, 18, 1),
            tok(TokenKind::Ident, 19, 1), // c
            tok(TokenKind::Colon, 20, 1),
            tok(TokenKind::Ident, 21, 3), // u64
            tok(TokenKind::RParen, 24, 1),
            tok(TokenKind::Arrow, 26, 2),
            tok(TokenKind::IntLit, 29, 1), // 1
            tok(TokenKind::Eof, 30, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "mixed curried/flat should be accepted");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLambda);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Lambda { params, .. } = expr_data {
                assert_eq!(params.len(), 3, "should accumulate into 3 params");
            } else {
                panic!("expected ExprLambda");
            }
        }
    }

    #[test]
    fn lambda_fn_seven_params_rejects() {
        // fn (a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> 1
        // Should reject with P0276 on the 7th parameter
        let tokens = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::Ident, 4, 1), // a
            tok(TokenKind::Colon, 5, 1),
            tok(TokenKind::Ident, 6, 3), // u64
            tok(TokenKind::Comma, 9, 1),
            tok(TokenKind::Ident, 11, 1), // b
            tok(TokenKind::Colon, 12, 1),
            tok(TokenKind::Ident, 13, 3), // u64
            tok(TokenKind::Comma, 16, 1),
            tok(TokenKind::Ident, 18, 1), // c
            tok(TokenKind::Colon, 19, 1),
            tok(TokenKind::Ident, 20, 3), // u64
            tok(TokenKind::Comma, 23, 1),
            tok(TokenKind::Ident, 25, 1), // d
            tok(TokenKind::Colon, 26, 1),
            tok(TokenKind::Ident, 27, 3), // u64
            tok(TokenKind::Comma, 30, 1),
            tok(TokenKind::Ident, 32, 1), // e
            tok(TokenKind::Colon, 33, 1),
            tok(TokenKind::Ident, 34, 3), // u64
            tok(TokenKind::Comma, 37, 1),
            tok(TokenKind::Ident, 39, 1), // f
            tok(TokenKind::Colon, 40, 1),
            tok(TokenKind::Ident, 41, 3), // u64
            tok(TokenKind::Comma, 44, 1),
            tok(TokenKind::Ident, 46, 1), // g (7th)
            tok(TokenKind::Colon, 47, 1),
            tok(TokenKind::Ident, 48, 3), // u64
            tok(TokenKind::RParen, 51, 1),
            tok(TokenKind::Arrow, 53, 2),
            tok(TokenKind::IntLit, 56, 1), // 1
            tok(TokenKind::Eof, 57, 0),
        ];

        // Parse manually to check for parse error + diagnostic
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr()
        };
        let diags = sink.diagnostics().to_vec();

        // Should have a parse error and 1 error diagnostic (P0276)
        assert!(result.is_err(), "parse should fail");
        assert_eq!(diags.len(), 1, "expected P0276 error");
        assert!(
            diags[0].code().to_string().contains("P0276"),
            "expected P0276 code, got: {}",
            diags[0].code()
        );
    }
}
