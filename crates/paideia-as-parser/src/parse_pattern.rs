//! Pattern parsing for let bindings and match arms.
//!
//! Implements §8 Pattern grammar: Ident, Wildcard, Tuple, Struct, EnumVariant,
//! Literal, Or, Binding, Range, Reference, and Slice patterns (with Rest sub-
//! pattern). PAS-DEBT-B2-013 (#1506) closed the last four shapes.

use paideia_as_ast::{NodeKind, PatternData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a pattern for let bindings.
    ///
    /// Supports:
    /// - `_` → Wildcard
    /// - `ident` → Ident pattern
    /// - `(pat1, pat2, ...)` → Tuple pattern
    /// - `TypeName { field: pat, ... }` → Struct pattern
    /// - `EnumName::Variant(pat...)` → EnumVariant pattern
    /// - `0`, `true`, etc. → Literal pattern
    ///
    /// Returns a pattern node on success.
    pub fn parse_pattern(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.parse_pattern_or()
    }

    /// Parse an or-pattern: `pat1 | pat2 | ...`
    fn parse_pattern_or(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let first = self.parse_pattern_binding()?;

        // Check for `|` to form an or-pattern
        if self.at(TokenKind::Pipe) {
            let mut alternatives = vec![first];
            let span_start = self
                .arena()
                .get(first)
                .map(|n| n.span)
                .unwrap_or_else(|| self.current_span());

            while self.eat(TokenKind::Pipe) {
                let alt = self.parse_pattern_binding()?;
                alternatives.push(alt);
            }

            let span_end = self
                .arena()
                .get(*alternatives.last().unwrap())
                .map(|n| n.span)
                .unwrap_or(span_start);
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
            );

            Ok(self.arena_mut().alloc_pattern(
                NodeKind::PatOr,
                span,
                PatternData::Or { alternatives },
            ))
        } else {
            Ok(first)
        }
    }

    /// Parse a binding pattern: `name @ pat` or just the inner pattern.
    fn parse_pattern_binding(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let inner = self.parse_pattern_range()?;

        // Check for `@` to form a binding pattern
        if self.at(TokenKind::At) {
            self.bump(); // consume `@`

            let pat = self.parse_pattern_primary()?;

            let span = Span::new(
                self.arena()
                    .get(inner)
                    .map(|n| n.span)
                    .unwrap_or_else(|| self.current_span())
                    .file(),
                self.arena()
                    .get(inner)
                    .map(|n| n.span)
                    .unwrap_or_else(|| self.current_span())
                    .byte_start(),
                self.arena()
                    .get(pat)
                    .map(|n| n.span)
                    .unwrap_or_else(|| self.current_span())
                    .byte_start()
                    + self
                        .arena()
                        .get(pat)
                        .map(|n| n.span)
                        .unwrap_or_else(|| self.current_span())
                        .byte_len()
                    - self
                        .arena()
                        .get(inner)
                        .map(|n| n.span)
                        .unwrap_or_else(|| self.current_span())
                        .byte_start(),
            );

            Ok(self.arena_mut().alloc_pattern(
                NodeKind::PatBinding,
                span,
                PatternData::Binding {
                    name: inner,
                    inner: pat,
                },
            ))
        } else {
            Ok(inner)
        }
    }

    /// Parse a range pattern layer: `a..b`, `..b`, `a..`, or fall through
    /// to primary. Range is non-associative and binds tighter than or/binding.
    fn parse_pattern_range(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Prefix form: `..end` or bare `..`.
        if self.at(TokenKind::DotDot) {
            let dot_tok = self.expect(TokenKind::DotDot)?;
            let dot_span = dot_tok.span;

            // Endpoint present iff the next token can start a primary pattern.
            let (end_opt, span_end) = if self.can_start_pattern_primary() {
                let end = self.parse_pattern_primary()?;
                let sp = self
                    .arena()
                    .get(end)
                    .map(|n| n.span)
                    .unwrap_or(dot_span);
                (Some(end), sp)
            } else {
                (None, dot_span)
            };

            let span = Span::new(
                dot_span.file(),
                dot_span.byte_start(),
                span_end.byte_start() + span_end.byte_len() - dot_span.byte_start(),
            );
            return Ok(self.arena_mut().alloc_pattern(
                NodeKind::PatRange,
                span,
                PatternData::Range {
                    start: None,
                    end: end_opt,
                },
            ));
        }

        let lhs = self.parse_pattern_primary()?;

        // Postfix form: `start..` or `start..end`.
        if self.at(TokenKind::DotDot) {
            self.bump(); // consume `..`
            let start_span = self
                .arena()
                .get(lhs)
                .map(|n| n.span)
                .unwrap_or_else(|| self.current_span());

            let (end_opt, span_end) = if self.can_start_pattern_primary() {
                let end = self.parse_pattern_primary()?;
                let sp = self
                    .arena()
                    .get(end)
                    .map(|n| n.span)
                    .unwrap_or(start_span);
                (Some(end), sp)
            } else {
                (None, start_span)
            };

            let span = Span::new(
                start_span.file(),
                start_span.byte_start(),
                span_end.byte_start() + span_end.byte_len() - start_span.byte_start(),
            );
            return Ok(self.arena_mut().alloc_pattern(
                NodeKind::PatRange,
                span,
                PatternData::Range {
                    start: Some(lhs),
                    end: end_opt,
                },
            ));
        }

        Ok(lhs)
    }

    /// Return true when the current token starts a pattern primary. Used by
    /// range parsing to decide whether an endpoint is present.
    fn can_start_pattern_primary(&self) -> bool {
        match self.peek().map(|t| t.kind) {
            Some(
                TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::Amp
                | TokenKind::Ident
                | TokenKind::IntLit
                | TokenKind::StringLit
                | TokenKind::CharLit
                | TokenKind::ByteLit
                | TokenKind::ByteStringLit,
            ) => true,
            _ => false,
        }
    }

    /// Parse a primary pattern (not or/binding/range).
    fn parse_pattern_primary(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        if let Some(tok) = self.peek() {
            let tok_kind = tok.kind;
            let span = tok.span;

            match tok_kind {
                // Tuple: `(pat1, pat2, ...)`
                TokenKind::LParen => self.parse_pattern_tuple(),

                // Slice: `[pat1, pat2, ..]` (PAS-DEBT-B2-013)
                TokenKind::LBracket => self.parse_pattern_slice(),

                // Reference: `&pat` or `&mut pat` (PAS-DEBT-B2-013)
                TokenKind::Amp => self.parse_pattern_reference(),

                // Literal: int, etc.
                TokenKind::IntLit => {
                    self.bump();
                    let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span);
                    Ok(self.arena_mut().alloc_pattern(
                        NodeKind::PatLiteral,
                        span,
                        PatternData::Literal { lit: lit_id },
                    ))
                }

                // StringLit, CharLit, ByteLit, ByteStringLit
                TokenKind::StringLit
                | TokenKind::CharLit
                | TokenKind::ByteLit
                | TokenKind::ByteStringLit => {
                    self.bump();
                    let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span);
                    Ok(self.arena_mut().alloc_pattern(
                        NodeKind::PatLiteral,
                        span,
                        PatternData::Literal { lit: lit_id },
                    ))
                }

                // Ident, Struct, EnumVariant, Wildcard
                TokenKind::Ident => {
                    self.bump();
                    let ident_id = self.arena_mut().alloc(NodeKind::Ident, span);

                    // Peek ahead to distinguish between:
                    // 1. Plain ident: `x` → Ident pattern
                    // 2. `::Variant(...)` → EnumVariant pattern
                    // 3. `{ field: pat, ... }` → Struct pattern
                    if self.at(TokenKind::ColonColon) {
                        // EnumVariant: parse the rest
                        self.bump(); // consume `::`

                        let variant_tok = self.expect(TokenKind::Ident)?;
                        let variant_id = self.arena_mut().alloc(NodeKind::Ident, variant_tok.span);

                        // Parse arguments: `(pat1, pat2, ...)` or nothing
                        let args = if self.at(TokenKind::LParen) {
                            self.bump(); // consume `(`
                            let mut args = vec![];
                            while !self.at(TokenKind::RParen) {
                                let arg = self.parse_pattern()?;
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

                        let variant_path = variant_id;

                        let span_final = Span::new(
                            span.file(),
                            span.byte_start(),
                            last_span.byte_start() + last_span.byte_len() - span.byte_start(),
                        );

                        Ok(self.arena_mut().alloc_pattern(
                            NodeKind::PatEnumVariant,
                            span_final,
                            PatternData::EnumVariant {
                                path: variant_path,
                                args,
                            },
                        ))
                    } else if self.at(TokenKind::LBrace) {
                        // Struct pattern: parse fields
                        self.bump(); // consume `{`

                        let mut fields = vec![];
                        while !self.at(TokenKind::RBrace) {
                            let field_tok = self.expect(TokenKind::Ident)?;
                            let field_name =
                                self.arena_mut().alloc(NodeKind::Ident, field_tok.span);

                            self.expect(TokenKind::Colon)?;

                            let field_pattern = self.parse_pattern()?;

                            fields.push((field_name, field_pattern));

                            if !self.at(TokenKind::RBrace) {
                                self.expect(TokenKind::Comma)?;
                            }
                        }

                        let rbrace_tok = self.expect(TokenKind::RBrace)?;

                        let struct_path = ident_id;

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
                                path: struct_path,
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

                _ => {
                    // Unexpected token
                    let code = paideia_as_diagnostics::DiagnosticCode::new(
                        paideia_as_diagnostics::Category::P,
                        paideia_as_diagnostics::Severity::Error,
                        100,
                    )
                    .expect("valid P0100 code");
                    let diag = paideia_as_diagnostics::Diagnostic::error(code)
                        .message("expected pattern (ident, wildcard, tuple, struct, enum variant, or literal)")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    Err(ParseError)
                }
            }
        } else {
            let span = self.current_span();
            let code = paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                100,
            )
            .expect("valid P0100 code");
            let diag = paideia_as_diagnostics::Diagnostic::error(code)
                .message("expected pattern, found EOF")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            Err(ParseError)
        }
    }

    /// Parse a reference pattern: `&pat` or `&mut pat` (PAS-DEBT-B2-013).
    fn parse_pattern_reference(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let amp_tok = self.expect(TokenKind::Amp)?;
        let amp_span = amp_tok.span;
        let mutable = self.eat(TokenKind::KwMut);

        let inner = self.parse_pattern_primary()?;
        let inner_span = self
            .arena()
            .get(inner)
            .map(|n| n.span)
            .unwrap_or(amp_span);
        let span = Span::new(
            amp_span.file(),
            amp_span.byte_start(),
            inner_span.byte_start() + inner_span.byte_len() - amp_span.byte_start(),
        );
        Ok(self.arena_mut().alloc_pattern(
            NodeKind::PatReference,
            span,
            PatternData::Reference { inner, mutable },
        ))
    }

    /// Parse a slice pattern: `[p1, p2, ..]` / `[first, .., last]`
    /// (PAS-DEBT-B2-013). Rest sub-pattern (`..` optionally bound as
    /// `..name`) may appear anywhere in the element list.
    fn parse_pattern_slice(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lbrk_tok = self.expect(TokenKind::LBracket)?;
        let span_start = lbrk_tok.span;

        let mut elements: Vec<paideia_as_ast::NodeId> = Vec::new();
        let mut saw_rest = false;

        // Empty slice `[]`.
        if self.at(TokenKind::RBracket) {
            let rbrk_tok = self.expect(TokenKind::RBracket)?;
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbrk_tok.span.byte_start() + rbrk_tok.span.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_pattern(
                NodeKind::PatSlice,
                span,
                PatternData::Slice { elements },
            ));
        }

        loop {
            if self.at(TokenKind::DotDot) {
                let dot_tok = self.expect(TokenKind::DotDot)?;
                let mut rest_span = dot_tok.span;
                // Optional binder `..name` (element order sensitive; the
                // lexer accepts `Ident` immediately after `..`).
                let binder = if self.at(TokenKind::Ident) {
                    let name_tok = self.expect(TokenKind::Ident)?;
                    let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);
                    rest_span = Span::new(
                        rest_span.file(),
                        rest_span.byte_start(),
                        name_tok.span.byte_start() + name_tok.span.byte_len()
                            - rest_span.byte_start(),
                    );
                    Some(name_id)
                } else {
                    None
                };

                if saw_rest {
                    let code = paideia_as_diagnostics::DiagnosticCode::new(
                        paideia_as_diagnostics::Category::P,
                        paideia_as_diagnostics::Severity::Error,
                        104,
                    )
                    .expect("valid P0104 code");
                    let diag = paideia_as_diagnostics::Diagnostic::error(code)
                        .message("slice pattern may contain at most one `..` rest")
                        .with_span(rest_span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
                saw_rest = true;

                let rest_id = self.arena_mut().alloc_pattern(
                    NodeKind::PatRest,
                    rest_span,
                    PatternData::Rest { binder },
                );
                elements.push(rest_id);
            } else {
                let elem = self.parse_pattern()?;
                elements.push(elem);
            }

            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume `,`
            if self.at(TokenKind::RBracket) {
                break; // trailing comma
            }
        }

        let rbrk_tok = self.expect(TokenKind::RBracket)?;
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrk_tok.span.byte_start() + rbrk_tok.span.byte_len() - span_start.byte_start(),
        );
        Ok(self.arena_mut().alloc_pattern(
            NodeKind::PatSlice,
            span,
            PatternData::Slice { elements },
        ))
    }

    /// Parse a tuple pattern: `(pat1, pat2, ...)`
    fn parse_pattern_tuple(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lparen_tok = self.expect(TokenKind::LParen)?;
        let span_start = lparen_tok.span;

        let mut elements = vec![];

        // Handle empty tuple `()`
        if self.at(TokenKind::RParen) {
            let rparen_tok = self.expect(TokenKind::RParen)?;
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rparen_tok.span.byte_start() + rparen_tok.span.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_pattern(
                NodeKind::PatTuple,
                span,
                PatternData::Tuple { elements },
            ));
        }

        // Parse elements
        loop {
            let elem = self.parse_pattern()?;
            elements.push(elem);

            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma

            // Check for trailing comma
            if self.at(TokenKind::RParen) {
                break;
            }
        }

        let rparen_tok = self.expect(TokenKind::RParen)?;
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rparen_tok.span.byte_start() + rparen_tok.span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_pattern(
            NodeKind::PatTuple,
            span,
            PatternData::Tuple { elements },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, Severity, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};

    fn parse_pat(
        source: &str,
    ) -> (
        paideia_as_ast::NodeId,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut source_map = paideia_as_diagnostics::SourceMap::new();
        let file = source_map.add_file(std::path::PathBuf::from("test.pdx"), source.to_string());
        let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
        let mut arena = paideia_as_ast::AstArena::new();
        let mut sink = VecSink::new();
        let mut lex = Lexer::new(file, &source_text);
        let mut collector = VecSink::new();
        let tokens = lex.collect_tokens(&mut collector);
        // Forward lexer diagnostics into the main sink
        for d in collector.into_diagnostics() {
            let _ = sink.emit(d);
        }
        let result = {
            let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
            p.parse_pattern()
        };
        let diags = sink.into_diagnostics();
        match result {
            Ok(id) => (id, diags),
            Err(_) => panic!("parse_pattern failed with error"),
        }
    }

    #[test]
    fn parse_pattern_ident() {
        let (pat_id, _diags) = parse_pat("x");
        let arena = paideia_as_ast::AstArena::new();
        // Basic sanity check that we got a node
        assert_ne!(pat_id.get(), 0);
    }

    #[test]
    fn parse_pattern_wildcard() {
        let (pat_id, _diags) = parse_pat("_");
        assert_ne!(pat_id.get(), 0);
    }

    #[test]
    fn parse_pattern_tuple_2() {
        let (pat_id, _diags) = parse_pat("(x, y)");
        assert_ne!(pat_id.get(), 0);
    }

    #[test]
    fn parse_pattern_record() {
        let (pat_id, _diags) = parse_pat("TypeName { field: x }");
        assert_ne!(pat_id.get(), 0);
    }

    #[test]
    fn parse_pattern_enum_variant() {
        let (pat_id, _diags) = parse_pat("EnumName::Variant(x, y)");
        assert_ne!(pat_id.get(), 0);
    }

    #[test]
    fn parse_pattern_literal() {
        let (pat_id, _diags) = parse_pat("42");
        assert_ne!(pat_id.get(), 0);
    }
}
