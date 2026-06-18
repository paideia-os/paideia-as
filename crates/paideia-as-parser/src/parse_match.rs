//! Match expression parsing.
//!
//! Implements §8 MatchExpr grammar: `match <expr> { <pat> => <expr>, ... }`.

use paideia_as_ast::{ExprData, MatchArm, NodeKind, PatternData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a match expression.
    ///
    /// Form: `match <expr> { <pat> => <expr>, <pat> => <expr>, ... }`.
    /// Returns a `NodeKind::ExprMatch`.
    ///
    /// For phase-1:
    /// - Pattern guards are deferred (parsed but not stored).
    /// - Patterns are limited to Ident and Literal (Wildcard is treated as Ident).
    pub(crate) fn parse_match(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let match_tok = self.expect(TokenKind::KwMatch)?;
        let match_span = match_tok.span;

        // Parse scrutinee expression
        let scrutinee = self.parse_expr()?;

        // Expect opening brace
        self.expect(TokenKind::LBrace)?;

        let mut arms = Vec::new();

        // Parse arms: pat => expr, pat => expr, ...
        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Parse pattern
            let pattern = self.parse_pattern_match()?;

            // Optional guard (deferred for phase-1)
            if self.at(TokenKind::KwIf) {
                self.bump(); // consume `if`
                // Parse the guard expression
                let _guard_expr = self.parse_expr()?;
                // For phase-1, we drop the guard; it's parsed but not stored.
            }

            // Expect `=>`
            self.expect(TokenKind::FatArrow)?;

            // Parse arm body
            let body = self.parse_expr()?;

            // Add the arm
            arms.push(MatchArm {
                pattern,
                guard: None, // Guards deferred to future PR
                body,
            });

            // Check for comma
            if !self.at(TokenKind::Comma) {
                // No comma: check for closing brace next
                if !self.at(TokenKind::RBrace) {
                    // Emit diagnostic for expected comma or brace?
                    // For now, let the closing brace expect handle it.
                }
            } else {
                self.bump(); // consume comma
                // After comma, check for trailing close
                if self.at(TokenKind::RBrace) {
                    break;
                }
            }
        }

        // Expect closing brace
        self.expect(TokenKind::RBrace)?;

        // Compute span
        let last_arm_body = arms
            .last()
            .and_then(|arm| self.arena().get(arm.body).map(|nd| nd.span))
            .unwrap_or(match_span);
        let match_expr_span = Span::new(
            match_span.file(),
            match_span.byte_start(),
            last_arm_body.byte_start() + last_arm_body.byte_len() - match_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprMatch,
            match_expr_span,
            ExprData::Match { scrutinee, arms },
        ))
    }

    /// Parse a pattern for use in match arms.
    ///
    /// For phase-1, supports:
    /// - Identifier (including wildcard `_`)
    /// - Integer literal
    /// - String literal
    /// - Character literal
    ///
    /// Returns a pattern node.
    fn parse_pattern_match(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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

                TokenKind::IntLit => {
                    self.bump();
                    let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span);
                    Ok(self.arena_mut().alloc_pattern(
                        NodeKind::PatLiteral,
                        span,
                        PatternData::Literal { lit: lit_id },
                    ))
                }

                TokenKind::StringLit => {
                    self.bump();
                    let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span);
                    Ok(self.arena_mut().alloc_pattern(
                        NodeKind::PatLiteral,
                        span,
                        PatternData::Literal { lit: lit_id },
                    ))
                }

                TokenKind::CharLit => {
                    self.bump();
                    let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span);
                    Ok(self.arena_mut().alloc_pattern(
                        NodeKind::PatLiteral,
                        span,
                        PatternData::Literal { lit: lit_id },
                    ))
                }

                _ => self.error_expected_match_pattern(),
            }
        } else {
            self.error_expected_match_pattern()
        }
    }

    /// Emit a P0100 ("expected pattern") diagnostic and return Err.
    fn error_expected_match_pattern(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
            let mut p = Parser::new(&tokens, FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr().expect("parse failed")
        };
        let diags = sink.diagnostics().to_vec();
        (arena, root, diags)
    }

    #[test]
    fn match_two_literal_arms() {
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5), // match
            tok(TokenKind::Ident, 6, 1),   // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1),    // 0
            tok(TokenKind::FatArrow, 12, 2),  // =>
            tok(TokenKind::StringLit, 15, 6), // "zero"
            tok(TokenKind::Comma, 21, 1),
            tok(TokenKind::Ident, 23, 1),     // _
            tok(TokenKind::FatArrow, 25, 2),  // =>
            tok(TokenKind::StringLit, 28, 7), // "other"
            tok(TokenKind::RBrace, 35, 1),
            tok(TokenKind::Eof, 36, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 2);
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn match_no_trailing_comma_before_brace() {
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5), // match
            tok(TokenKind::Ident, 6, 1),   // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1),   // 1
            tok(TokenKind::FatArrow, 12, 2), // =>
            tok(TokenKind::IntLit, 15, 1),   // 2
            tok(TokenKind::Comma, 16, 1),
            tok(TokenKind::IntLit, 18, 1),   // 3
            tok(TokenKind::FatArrow, 20, 2), // =>
            tok(TokenKind::IntLit, 23, 1),   // 4
            tok(TokenKind::RBrace, 24, 1),
            tok(TokenKind::Eof, 25, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 2);
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn match_one_arm() {
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5), // match
            tok(TokenKind::Ident, 6, 1),   // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 1),    // _
            tok(TokenKind::FatArrow, 12, 2), // =>
            tok(TokenKind::IntLit, 15, 1),   // 0
            tok(TokenKind::RBrace, 16, 1),
            tok(TokenKind::Eof, 17, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn match_arm_with_complex_body() {
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5), // match
            tok(TokenKind::Ident, 6, 1),   // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1),   // 0
            tok(TokenKind::FatArrow, 12, 2), // =>
            tok(TokenKind::Ident, 15, 1),    // a
            tok(TokenKind::Plus, 17, 1),
            tok(TokenKind::Ident, 19, 1), // b
            tok(TokenKind::Comma, 20, 1),
            tok(TokenKind::Ident, 22, 1),    // _
            tok(TokenKind::FatArrow, 24, 2), // =>
            tok(TokenKind::Ident, 27, 1),    // f
            tok(TokenKind::LParen, 28, 1),
            tok(TokenKind::RParen, 29, 1),
            tok(TokenKind::RBrace, 30, 1),
            tok(TokenKind::Eof, 31, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 2);
                // First arm body should be an infix expression (a + b)
                let first_body = arena.get(arms[0].body).unwrap();
                assert_eq!(first_body.kind, NodeKind::ExprInfix);
                // Second arm body should be a call expression f()
                let second_body = arena.get(arms[1].body).unwrap();
                assert_eq!(second_body.kind, NodeKind::ExprCall);
            } else {
                panic!("expected ExprMatch");
            }
        }
    }
}
