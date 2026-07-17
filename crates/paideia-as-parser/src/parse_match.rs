//! Match expression parsing.
//!
//! Implements §8 MatchExpr grammar: `match <expr> [@jump_table] { <pat> => <expr>, ... }`.

use paideia_as_ast::{ExprData, MatchArm, MatchAttrs, NodeKind, PatternData};
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

        // Parse optional attributes: @jump_table
        let mut attrs = MatchAttrs::default();
        if self.at(TokenKind::At) {
            self.bump(); // consume `@`
            if let Some(attr_name) = self.peek_ident_text() {
                let attr_span = self.current_span();
                let attr_name_owned = attr_name.to_string();
                self.bump(); // consume attribute name

                match attr_name_owned.as_str() {
                    "jump_table" => {
                        attrs.jump_table = true;
                    }
                    _ => {
                        // P0270: unknown attribute name
                        let diag = paideia_as_diagnostics::Diagnostic::error(
                            paideia_as_diagnostics::DiagnosticCode::new(
                                paideia_as_diagnostics::Category::P,
                                paideia_as_diagnostics::Severity::Error,
                                270,
                            )
                            .expect("P0270 should be valid"),
                        )
                        .message(format!("unknown match attribute '{}'", attr_name_owned))
                        .with_span(attr_span)
                        .finish();
                        self.emit_diagnostic(diag);
                    }
                }
            } else if let Some(tok) = self.peek() {
                // P0271: malformed attribute syntax
                let diag = paideia_as_diagnostics::Diagnostic::error(
                    paideia_as_diagnostics::DiagnosticCode::new(
                        paideia_as_diagnostics::Category::P,
                        paideia_as_diagnostics::Severity::Error,
                        271,
                    )
                    .expect("P0271 should be valid"),
                )
                .message("malformed match attribute".to_string())
                .with_span(tok.span)
                .finish();
                self.emit_diagnostic(diag);
            }
        }

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

            // Optional guard
            let guard = if self.at(TokenKind::KwIf) {
                self.bump(); // consume `if`
                Some(self.parse_expr()?)
            } else {
                None
            };

            // Expect `=>`
            self.expect(TokenKind::FatArrow)?;

            // Parse arm body
            let body = self.parse_expr()?;

            // Add the arm
            arms.push(MatchArm {
                pattern,
                guard,
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
            ExprData::Match {
                scrutinee,
                arms,
                attrs,
            },
        ))
    }

    /// Parse a pattern for use in match arms.
    ///
    /// Supports:
    /// - Identifier (including wildcard `_`)
    /// - Integer literal
    /// - String literal
    /// - Character literal
    /// - Enum variant patterns (bare or path-qualified):
    ///   - Bare variant with args: `Ok(x)`, `Err(e)`
    ///   - Path-qualified: `Result::Ok(x)`, `Option::None`
    ///   - Bare variant without args: `Pending`
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

                    // Check for enum variant patterns:
                    // 1. `::Variant(...)` → EnumVariant with path-qualified form
                    // 2. `(...)` → EnumVariant with bare variant (args present)
                    // Otherwise → plain Ident pattern
                    if self.at(TokenKind::ColonColon) {
                        // Path-qualified variant: `Type::Variant(...)`
                        self.bump(); // consume `::`

                        let variant_tok = self.expect(TokenKind::Ident)?;
                        let variant_id = self.arena_mut().alloc(NodeKind::Ident, variant_tok.span);

                        // Parse arguments: `(pat1, pat2, ...)` or nothing
                        let args = if self.at(TokenKind::LParen) {
                            self.bump(); // consume `(`
                            let mut args = vec![];
                            while !self.at(TokenKind::RParen) {
                                let arg = self.parse_pattern_match()?;
                                args.push(arg);
                                if !self.at(TokenKind::RParen) {
                                    self.expect(TokenKind::Comma)?;
                                }
                            }
                            self.expect(TokenKind::RParen)?;
                            args
                        } else {
                            vec![]
                        };

                        let last_span = if let Some(last_arg) = args.last() {
                            self.arena()
                                .get(*last_arg)
                                .map(|n| n.span)
                                .unwrap_or(variant_tok.span)
                        } else {
                            variant_tok.span
                        };

                        let span_final = Span::new(
                            span.file(),
                            span.byte_start(),
                            last_span.byte_start() + last_span.byte_len() - span.byte_start(),
                        );

                        Ok(self.arena_mut().alloc_pattern(
                            NodeKind::PatEnumVariant,
                            span_final,
                            PatternData::EnumVariant {
                                path: variant_id,
                                args,
                            },
                        ))
                    } else if self.at(TokenKind::LParen) {
                        // Bare variant with args: `Ok(x)`, `Err(e)`
                        self.bump(); // consume `(`
                        let mut args = vec![];
                        while !self.at(TokenKind::RParen) {
                            let arg = self.parse_pattern_match()?;
                            args.push(arg);
                            if !self.at(TokenKind::RParen) {
                                self.expect(TokenKind::Comma)?;
                            }
                        }
                        let rparen_tok = self.expect(TokenKind::RParen)?;

                        let span_final = Span::new(
                            span.file(),
                            span.byte_start(),
                            rparen_tok.span.byte_start() + rparen_tok.span.byte_len()
                                - span.byte_start(),
                        );

                        Ok(self.arena_mut().alloc_pattern(
                            NodeKind::PatEnumVariant,
                            span_final,
                            PatternData::EnumVariant {
                                path: ident_id,
                                args,
                            },
                        ))
                    } else if self.at(TokenKind::LBrace) {
                        // Record pattern: `Point { x: a, y: b }`
                        self.bump(); // consume `{`

                        let mut fields = vec![];
                        while !self.at(TokenKind::RBrace) {
                            let field_tok = self.expect(TokenKind::Ident)?;
                            let field_name =
                                self.arena_mut().alloc(NodeKind::Ident, field_tok.span);

                            // Check for shorthand `{ x, y }` vs `{ x: pat, y: pat }`
                            if self.at(TokenKind::Colon) {
                                self.bump(); // consume `:`
                                let field_pattern = self.parse_pattern_match()?;
                                fields.push((field_name, field_pattern));
                            } else if self.at(TokenKind::Comma) || self.at(TokenKind::RBrace) {
                                // Shorthand: synthesize a PatIdent pattern with the same name
                                let field_text = field_tok.span;
                                let shorthand_ident = self.arena_mut().alloc(NodeKind::Ident, field_text);
                                let field_pattern = self.arena_mut().alloc_pattern(
                                    NodeKind::PatIdent,
                                    field_text,
                                    PatternData::Ident {
                                        name: shorthand_ident,
                                        mutable: false,
                                    },
                                );
                                fields.push((field_name, field_pattern));
                            } else {
                                return self.error_expected_match_pattern();
                            }

                            if !self.at(TokenKind::RBrace) {
                                self.expect(TokenKind::Comma)?;
                            }
                        }

                        let rbrace_tok = self.expect(TokenKind::RBrace)?;

                        let span_final = Span::new(
                            span.file(),
                            span.byte_start(),
                            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len()
                                - span.byte_start(),
                        );

                        Ok(self.arena_mut().alloc_pattern(
                            NodeKind::PatStruct,
                            span_final,
                            PatternData::Struct {
                                path: ident_id,
                                fields: fields
                                    .into_iter()
                                    .map(|(name, pattern)| paideia_as_ast::PatField {
                                        name,
                                        pattern,
                                    })
                                    .collect(),
                            },
                        ))
                    } else {
                        // Plain ident pattern
                        Ok(self.arena_mut().alloc_pattern(
                            NodeKind::PatIdent,
                            span,
                            PatternData::Ident {
                                name: ident_id,
                                mutable: false,
                            },
                        ))
                    }
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
        // Create a minimal source that covers all token positions
        // For match_two_literal_arms test: "match x { 0 => "zero", _ => "other" }"
        let source = "match x { 0 => \"zero\", _ => \"other\" }";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(
                &tokens,
                source,
                FileId::new(1).unwrap(),
                &mut arena,
                &mut sink,
            );
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

    #[test]
    fn match_with_jump_table_attribute() {
        // Test that @jump_table attribute is accepted between scrutinee and {
        let source = "match x @jump_table { 0 => 1, _ => 2 }";
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::At, 8, 1),       // @
            tok(TokenKind::Ident, 9, 10),   // jump_table
            tok(TokenKind::LBrace, 20, 1),
            tok(TokenKind::IntLit, 22, 1),  // 0
            tok(TokenKind::FatArrow, 24, 2), // =>
            tok(TokenKind::IntLit, 27, 1),  // 1
            tok(TokenKind::Comma, 28, 1),
            tok(TokenKind::Ident, 30, 1),   // _
            tok(TokenKind::FatArrow, 32, 2), // =>
            tok(TokenKind::IntLit, 35, 1),  // 2
            tok(TokenKind::RBrace, 37, 1),
            tok(TokenKind::Eof, 38, 0),
        ];

        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, source, FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr().expect("parse failed")
        };
        let diags = sink.diagnostics().to_vec();

        // No diagnostics expected: valid attribute
        assert_eq!(diags.len(), 0, "expected no diagnostics for valid @jump_table");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { attrs, .. } = expr_data {
                assert!(
                    attrs.jump_table,
                    "jump_table attribute should be set to true"
                );
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn match_with_unknown_attribute_p0270() {
        // Test that unknown attribute @bogus produces P0270
        let source = "match x @bogus { 0 => 1, _ => 2 }";
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::At, 8, 1),       // @
            tok(TokenKind::Ident, 9, 5),    // bogus
            tok(TokenKind::LBrace, 15, 1),
            tok(TokenKind::IntLit, 17, 1),  // 0
            tok(TokenKind::FatArrow, 19, 2), // =>
            tok(TokenKind::IntLit, 22, 1),  // 1
            tok(TokenKind::Comma, 23, 1),
            tok(TokenKind::Ident, 25, 1),   // _
            tok(TokenKind::FatArrow, 27, 2), // =>
            tok(TokenKind::IntLit, 30, 1),  // 2
            tok(TokenKind::RBrace, 32, 1),
            tok(TokenKind::Eof, 33, 0),
        ];

        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let _root = {
            let mut p = Parser::new(&tokens, source, FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr().expect("parse failed")
        };
        let diags = sink.diagnostics().to_vec();

        // P0270 expected: unknown attribute
        assert!(
            diags.iter().any(|d| d.code().number() == 270),
            "expected P0270 for unknown attribute @bogus"
        );
    }

    #[test]
    fn match_with_malformed_attribute_p0271() {
        // Test that malformed @ followed by non-identifier produces P0271
        let source = "match x @ { 0 => 1, _ => 2 }";
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::At, 8, 1),       // @ — no identifier follows
            tok(TokenKind::LBrace, 10, 1),
            tok(TokenKind::IntLit, 12, 1),  // 0
            tok(TokenKind::FatArrow, 14, 2), // =>
            tok(TokenKind::IntLit, 17, 1),  // 1
            tok(TokenKind::Comma, 18, 1),
            tok(TokenKind::Ident, 20, 1),   // _
            tok(TokenKind::FatArrow, 22, 2), // =>
            tok(TokenKind::IntLit, 25, 1),  // 2
            tok(TokenKind::RBrace, 27, 1),
            tok(TokenKind::Eof, 28, 0),
        ];

        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, source, FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr().expect("parse should succeed despite P0271")
        };
        let diags = sink.diagnostics().to_vec();

        // P0271 expected: @ not followed by identifier
        assert!(
            diags.iter().any(|d| d.code().number() == 271),
            "expected P0271 for malformed attribute (@ without identifier)"
        );
        // Parse should still succeed and build an AST
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
    }

    #[test]
    fn parse_pattern_match_enum_variant_bare_with_arg() {
        // Test parsing: match x { Ok(y) => ... }
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 2),   // Ok
            tok(TokenKind::LParen, 12, 1),  // (
            tok(TokenKind::Ident, 13, 1),   // y
            tok(TokenKind::RParen, 14, 1),  // )
            tok(TokenKind::FatArrow, 16, 2), // =>
            tok(TokenKind::IntLit, 19, 1),  // 0
            tok(TokenKind::RBrace, 20, 1),
            tok(TokenKind::Eof, 21, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for enum variant pattern");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                let pattern_node = arena.get(arms[0].pattern).unwrap();
                assert_eq!(
                    pattern_node.kind,
                    NodeKind::PatEnumVariant,
                    "expected PatEnumVariant for Ok(y) pattern"
                );
                if let Some(PatternData::EnumVariant { path, args }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(args.len(), 1, "expected 1 argument in Ok(y)");
                    // Verify that path is an Ident node (not Placeholder)
                    let path_node = arena.get(*path).unwrap();
                    assert_eq!(path_node.kind, NodeKind::Ident, "path should be Ident node");
                } else {
                    panic!("expected EnumVariant pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn parse_pattern_match_enum_variant_qualified() {
        // Test parsing: match x { Result::Ok(y) => ... }
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),    // match
            tok(TokenKind::Ident, 6, 1),      // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 6),     // Result
            tok(TokenKind::ColonColon, 16, 2), // ::
            tok(TokenKind::Ident, 18, 2),     // Ok
            tok(TokenKind::LParen, 20, 1),    // (
            tok(TokenKind::Ident, 21, 1),     // y
            tok(TokenKind::RParen, 22, 1),    // )
            tok(TokenKind::FatArrow, 24, 2),  // =>
            tok(TokenKind::IntLit, 27, 1),    // 0
            tok(TokenKind::RBrace, 28, 1),
            tok(TokenKind::Eof, 29, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for path-qualified variant");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                let pattern_node = arena.get(arms[0].pattern).unwrap();
                assert_eq!(
                    pattern_node.kind,
                    NodeKind::PatEnumVariant,
                    "expected PatEnumVariant for Result::Ok(y) pattern"
                );
                if let Some(PatternData::EnumVariant { path, args }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(args.len(), 1, "expected 1 argument in Ok(y)");
                    let path_node = arena.get(*path).unwrap();
                    assert_eq!(path_node.kind, NodeKind::Ident, "path should be Ident node");
                } else {
                    panic!("expected EnumVariant pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn parse_pattern_match_enum_variant_multiple_args() {
        // Test parsing: match x { Pair(a, b) => ... }
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),   // match
            tok(TokenKind::Ident, 6, 1),     // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 4),    // Pair
            tok(TokenKind::LParen, 14, 1),   // (
            tok(TokenKind::Ident, 15, 1),    // a
            tok(TokenKind::Comma, 16, 1),
            tok(TokenKind::Ident, 18, 1),    // b
            tok(TokenKind::RParen, 19, 1),   // )
            tok(TokenKind::FatArrow, 21, 2), // =>
            tok(TokenKind::IntLit, 24, 1),   // 0
            tok(TokenKind::RBrace, 25, 1),
            tok(TokenKind::Eof, 26, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for multi-arg variant");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                if let Some(PatternData::EnumVariant { path: _, args }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(args.len(), 2, "expected 2 arguments in Pair(a, b)");
                } else {
                    panic!("expected EnumVariant pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn parse_pattern_match_record_pattern() {
        // Test parsing: match r { Point { x: a, y: b } => ... }
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),   // match
            tok(TokenKind::Ident, 6, 1),     // r
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 5),    // Point
            tok(TokenKind::LBrace, 16, 1),   // {
            tok(TokenKind::Ident, 18, 1),    // x
            tok(TokenKind::Colon, 19, 1),    // :
            tok(TokenKind::Ident, 21, 1),    // a
            tok(TokenKind::Comma, 22, 1),
            tok(TokenKind::Ident, 24, 1),    // y
            tok(TokenKind::Colon, 25, 1),    // :
            tok(TokenKind::Ident, 27, 1),    // b
            tok(TokenKind::RBrace, 28, 1),   // }
            tok(TokenKind::FatArrow, 30, 2), // =>
            tok(TokenKind::IntLit, 33, 1),   // 0
            tok(TokenKind::RBrace, 34, 1),
            tok(TokenKind::Eof, 35, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for record pattern");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                if let Some(PatternData::Struct { path: _, fields }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(fields.len(), 2, "expected 2 fields in Point pattern");
                } else {
                    panic!("expected Struct pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn parse_pattern_match_record_shorthand() {
        // Test parsing: match p { Point { x, y } => ... } (shorthand)
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),   // match
            tok(TokenKind::Ident, 6, 1),     // p
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 5),    // Point
            tok(TokenKind::LBrace, 16, 1),   // {
            tok(TokenKind::Ident, 18, 1),    // x
            tok(TokenKind::Comma, 19, 1),
            tok(TokenKind::Ident, 21, 1),    // y
            tok(TokenKind::RBrace, 22, 1),   // }
            tok(TokenKind::FatArrow, 24, 2), // =>
            tok(TokenKind::IntLit, 27, 1),   // 0
            tok(TokenKind::RBrace, 28, 1),
            tok(TokenKind::Eof, 29, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for shorthand record pattern");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                if let Some(PatternData::Struct { path: _, fields }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(fields.len(), 2, "expected 2 fields in shorthand pattern");
                    // Check that both fields have PatIdent patterns
                    for (i, field) in fields.iter().enumerate() {
                        if let Some(field_pat_data) = arena.pattern_data(field.pattern) {
                            match field_pat_data {
                                PatternData::Ident { .. } => {}
                                _ => panic!("field {} should have Ident pattern", i),
                            }
                        } else {
                            panic!("field {} pattern not found", i);
                        }
                    }
                } else {
                    panic!("expected Struct pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn parse_pattern_match_nested_enum_of_record() {
        // Test parsing: match r { Ok(Point { x, y }) => ... }
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),   // match
            tok(TokenKind::Ident, 6, 1),     // r
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 2),    // Ok
            tok(TokenKind::LParen, 12, 1),   // (
            tok(TokenKind::Ident, 13, 5),    // Point
            tok(TokenKind::LBrace, 19, 1),   // {
            tok(TokenKind::Ident, 21, 1),    // x
            tok(TokenKind::Comma, 22, 1),
            tok(TokenKind::Ident, 24, 1),    // y
            tok(TokenKind::RBrace, 25, 1),   // }
            tok(TokenKind::RParen, 26, 1),   // )
            tok(TokenKind::FatArrow, 28, 2), // =>
            tok(TokenKind::IntLit, 31, 1),   // 0
            tok(TokenKind::RBrace, 32, 1),
            tok(TokenKind::Eof, 33, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for nested enum-record pattern");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                if let Some(PatternData::EnumVariant { path: _, args }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(args.len(), 1, "expected 1 argument in Ok(...)");
                    if let Some(inner_data) = arena.pattern_data(args[0]) {
                        match inner_data {
                            PatternData::Struct { fields, .. } => {
                                assert_eq!(fields.len(), 2, "expected 2 fields in inner Point pattern");
                            }
                            _ => panic!("expected Struct pattern as argument to Ok"),
                        }
                    } else {
                        panic!("inner pattern not found");
                    }
                } else {
                    panic!("expected EnumVariant pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn parse_pattern_match_three_level_nesting() {
        // Test parsing: match x { Ok(Some(a)) => ... }
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),   // match
            tok(TokenKind::Ident, 6, 1),     // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 2),    // Ok
            tok(TokenKind::LParen, 12, 1),   // (
            tok(TokenKind::Ident, 13, 4),    // Some
            tok(TokenKind::LParen, 17, 1),   // (
            tok(TokenKind::Ident, 18, 1),    // a
            tok(TokenKind::RParen, 19, 1),   // )
            tok(TokenKind::RParen, 20, 1),   // )
            tok(TokenKind::FatArrow, 22, 2), // =>
            tok(TokenKind::IntLit, 25, 1),   // 0
            tok(TokenKind::RBrace, 26, 1),
            tok(TokenKind::Eof, 27, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for three-level nested pattern");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                if let Some(PatternData::EnumVariant { path: _, args: outer_args }) =
                    arena.pattern_data(arms[0].pattern)
                {
                    assert_eq!(outer_args.len(), 1, "expected 1 argument in Ok(...)");
                    if let Some(inner_data) = arena.pattern_data(outer_args[0]) {
                        match inner_data {
                            PatternData::EnumVariant { args: inner_args, .. } => {
                                assert_eq!(inner_args.len(), 1, "expected 1 argument in Some(...)");
                            }
                            _ => panic!("expected EnumVariant pattern as argument to Ok"),
                        }
                    } else {
                        panic!("middle pattern not found");
                    }
                } else {
                    panic!("expected EnumVariant pattern data");
                }
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn guard_is_stored_after_pattern() {
        // Test: match x { 0 if y => 1, _ => 2 } stores guard
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1),  // 0
            tok(TokenKind::KwIf, 12, 2),    // if
            tok(TokenKind::Ident, 15, 1),   // y
            tok(TokenKind::FatArrow, 17, 2), // =>
            tok(TokenKind::IntLit, 20, 1),  // 1
            tok(TokenKind::Comma, 21, 1),
            tok(TokenKind::Ident, 23, 1),   // _
            tok(TokenKind::FatArrow, 25, 2), // =>
            tok(TokenKind::IntLit, 28, 1),  // 2
            tok(TokenKind::RBrace, 29, 1),
            tok(TokenKind::Eof, 30, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 2);
                assert!(
                    arms[0].guard.is_some(),
                    "first arm should have guard stored"
                );
                assert!(
                    arms[1].guard.is_none(),
                    "second arm should have no guard"
                );
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn guard_absent_yields_none() {
        // Test: match x { 0 => 1 } → arm.guard.is_none()
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1),  // 0
            tok(TokenKind::FatArrow, 12, 2), // =>
            tok(TokenKind::IntLit, 15, 1),  // 1
            tok(TokenKind::RBrace, 16, 1),
            tok(TokenKind::Eof, 17, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 1);
                assert!(
                    arms[0].guard.is_none(),
                    "arm without guard should have guard.is_none()"
                );
            } else {
                panic!("expected ExprMatch");
            }
        }
    }

    #[test]
    fn guard_on_default() {
        // Test: match x { _ if y => 1, _ => 2 } → arms[0].guard.is_some(), arms[1].guard.is_none()
        let tokens = vec![
            tok(TokenKind::KwMatch, 0, 5),  // match
            tok(TokenKind::Ident, 6, 1),    // x
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 1),   // _
            tok(TokenKind::KwIf, 12, 2),    // if
            tok(TokenKind::Ident, 15, 1),   // y
            tok(TokenKind::FatArrow, 17, 2), // =>
            tok(TokenKind::IntLit, 20, 1),  // 1
            tok(TokenKind::Comma, 21, 1),
            tok(TokenKind::Ident, 23, 1),   // _
            tok(TokenKind::FatArrow, 25, 2), // =>
            tok(TokenKind::IntLit, 28, 1),  // 2
            tok(TokenKind::RBrace, 29, 1),
            tok(TokenKind::Eof, 30, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprMatch);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Match { arms, .. } = expr_data {
                assert_eq!(arms.len(), 2);
                assert!(
                    arms[0].guard.is_some(),
                    "first arm (wildcard with guard) should have guard"
                );
                assert!(
                    arms[1].guard.is_none(),
                    "second arm (wildcard without guard) should not have guard"
                );
            } else {
                panic!("expected ExprMatch");
            }
        }
    }
}
