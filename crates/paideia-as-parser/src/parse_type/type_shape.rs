//! Type shape parsing: parenthesized/tuple/fn-ptr, named type, path with self, and function-parameter form.
//! Split out of `parse_type.rs` (2026-07-08).

use paideia_as_ast::{NodeKind, TypeData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_type_paren(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lparen_tok = self.expect(TokenKind::LParen)?;
        let span_start = lparen_tok.span;

        // Check for empty tuple `()` or empty parameter list for function type
        if self.at(TokenKind::RParen) {
            let rparen_tok = self.expect(TokenKind::RParen)?;
            let span_end = rparen_tok.span;

            // Check for arrow (function type with zero parameters)
            if self.at(TokenKind::Arrow) {
                self.bump(); // consume `->`

                // Parse return type
                let ret = self.parse_type()?;
                let mut ret_span_end = self.arena().get(ret).map(|nd| nd.span).unwrap_or(span_end);

                // Parse optional effect set
                let effects = if self.at(TokenKind::EffectOpen) {
                    Some(self.parse_effect_row()?)
                } else {
                    None
                };
                if let Some(eff_id) = effects {
                    ret_span_end = self
                        .arena()
                        .get(eff_id)
                        .map(|nd| nd.span)
                        .unwrap_or(ret_span_end);
                }

                // Parse optional capability set
                let capabilities = if self.at(TokenKind::CapOpen) {
                    Some(self.parse_cap_set()?)
                } else {
                    None
                };
                if let Some(cap_id) = capabilities {
                    ret_span_end = self
                        .arena()
                        .get(cap_id)
                        .map(|nd| nd.span)
                        .unwrap_or(ret_span_end);
                }

                let span = Span::new(
                    span_start.file(),
                    span_start.byte_start(),
                    ret_span_end.byte_start() + ret_span_end.byte_len() - span_start.byte_start(),
                );
                return Ok(self.arena_mut().alloc_type(
                    NodeKind::TypeFnPtr,
                    span,
                    TypeData::FnPtr {
                        params: vec![],
                        ret,
                        effects,
                        capabilities,
                    },
                ));
            }

            // No arrow, just an empty tuple
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeTuple,
                span,
                TypeData::Tuple { elements: vec![] },
            ));
        }

        // Parse first parameter, checking for named-parameter form (name: Type)
        let first_type = self.parse_type_or_named_param()?;
        let mut elements = vec![first_type];

        // Check for comma (tuple) or closing paren
        let mut span_end = self
            .arena()
            .get(first_type)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        if self.at(TokenKind::Comma) {
            // Tuple: parse comma-separated types until RParen
            loop {
                // Consume the comma we just checked (or the one after the previous element)
                self.bump(); // consume `,`

                // Check for trailing comma before RParen
                if self.at(TokenKind::RParen) {
                    break;
                }

                let elem_type = self.parse_type_or_named_param()?;
                span_end = self
                    .arena()
                    .get(elem_type)
                    .map(|nd| nd.span)
                    .unwrap_or(span_end);
                elements.push(elem_type);

                // Check if there's another comma or if we're done
                if !self.at(TokenKind::Comma) {
                    break;
                }
            }

            let rparen_tok = self.expect(TokenKind::RParen)?;
            span_end = rparen_tok.span;

            // Check for arrow (function type with tuple parameters)
            if self.at(TokenKind::Arrow) {
                self.bump(); // consume `->`

                // Parse return type
                let ret = self.parse_type()?;
                span_end = self.arena().get(ret).map(|nd| nd.span).unwrap_or(span_end);

                // Parse optional effect set
                let effects = if self.at(TokenKind::EffectOpen) {
                    Some(self.parse_effect_row()?)
                } else {
                    None
                };
                if let Some(eff_id) = effects {
                    span_end = self
                        .arena()
                        .get(eff_id)
                        .map(|nd| nd.span)
                        .unwrap_or(span_end);
                }

                // Parse optional capability set
                let capabilities = if self.at(TokenKind::CapOpen) {
                    Some(self.parse_cap_set()?)
                } else {
                    None
                };
                if let Some(cap_id) = capabilities {
                    span_end = self
                        .arena()
                        .get(cap_id)
                        .map(|nd| nd.span)
                        .unwrap_or(span_end);
                }

                let span = Span::new(
                    span_start.file(),
                    span_start.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
                );
                return Ok(self.arena_mut().alloc_type(
                    NodeKind::TypeFnPtr,
                    span,
                    TypeData::FnPtr {
                        params: elements,
                        ret,
                        effects,
                        capabilities,
                    },
                ));
            }

            // Not an arrow, just a tuple
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeTuple,
                span,
                TypeData::Tuple { elements },
            ));
        }

        // Expect closing paren
        let rparen_tok = self.expect(TokenKind::RParen)?;
        span_end = rparen_tok.span;

        // Check for arrow (function type with single parameter)
        if self.at(TokenKind::Arrow) {
            self.bump(); // consume `->`

            // Parse return type
            let ret = self.parse_type()?;
            span_end = self.arena().get(ret).map(|nd| nd.span).unwrap_or(span_end);

            // Parse optional effect set
            let effects = if self.at(TokenKind::EffectOpen) {
                Some(self.parse_effect_row()?)
            } else {
                None
            };
            if let Some(eff_id) = effects {
                span_end = self
                    .arena()
                    .get(eff_id)
                    .map(|nd| nd.span)
                    .unwrap_or(span_end);
            }

            // Parse optional capability set
            let capabilities = if self.at(TokenKind::CapOpen) {
                Some(self.parse_cap_set()?)
            } else {
                None
            };
            if let Some(cap_id) = capabilities {
                span_end = self
                    .arena()
                    .get(cap_id)
                    .map(|nd| nd.span)
                    .unwrap_or(span_end);
            }

            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeFnPtr,
                span,
                TypeData::FnPtr {
                    params: elements,
                    ret,
                    effects,
                    capabilities,
                },
            ));
        }

        // Otherwise, it's a parenthesized type (single element, not a tuple)
        if elements.len() == 1 {
            Ok(elements.into_iter().next().unwrap())
        } else {
            // Should not happen given the logic above
            unreachable!("single element without comma should not reach here")
        }
    }

    /// Parse a type name: `Ident` or `Ident(T1, T2, ...)`.
    pub(super) fn parse_type_name(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let ident_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
        let mut span_end = ident_tok.span;

        let mut args = Vec::new();

        // Check for type arguments `(T1, T2, ...)`
        if self.at(TokenKind::LParen) {
            self.bump(); // consume `(`

            // Check for empty args
            if !self.at(TokenKind::RParen) {
                loop {
                    let arg_type = self.parse_type()?;
                    span_end = self
                        .arena()
                        .get(arg_type)
                        .map(|nd| nd.span)
                        .unwrap_or(span_end);
                    args.push(arg_type);

                    if !self.at(TokenKind::Comma) {
                        break;
                    }
                    self.bump(); // consume `,`
                }
            }

            let rparen_tok = self.expect(TokenKind::RParen)?;
            span_end = rparen_tok.span;
        }

        let span = Span::new(
            ident_tok.span.file(),
            ident_tok.span.byte_start(),
            span_end.byte_start() + span_end.byte_len() - ident_tok.span.byte_start(),
        );

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeName,
            span,
            TypeData::Name {
                name: name_id,
                args,
            },
        ))
    }

    /// Parse a Self-qualified path: `Self::Item`
    ///
    /// This recognizes the syntax for referencing an associated type within a trait context.
    /// Phase 4 minimum: parse-only; resolver will validate that `item` refers to a valid
    /// associated type on the trait.
    ///
    /// Returns a TypeSelfQualifiedPath node with the associated type name.
    pub(super) fn parse_self_qualified_path(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let self_tok = self.expect(TokenKind::KwSelfType)?;
        let span_start = self_tok.span;

        // Expect `::`
        if !self.at(TokenKind::ColonColon) {
            return self.error_expected_type();
        }
        self.bump(); // consume `::`

        // Parse the associated type name
        let assoc_type_tok = match self.expect(TokenKind::Ident) {
            Ok(tok) => tok,
            Err(_) => {
                return self.error_expected_type();
            }
        };
        let item_id = self.arena_mut().alloc(NodeKind::Ident, assoc_type_tok.span);

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            assoc_type_tok.span.byte_start() + assoc_type_tok.span.byte_len()
                - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeSelfQualifiedPath,
            span,
            TypeData::SelfQualifiedPath { item: item_id },
        ))
    }

    /// Parse a type parameter in function-type position, handling named parameters.
    ///
    /// This is used when parsing function-type parameter lists. It handles:
    /// - `name: Type` → parses `name:` and then the type; returns just the type (name discarded in phase-1).
    /// - `Type` → parses as a regular type.
    ///
    /// This allows function types like `(bar: MmioRegion, off: u32) -> u32` to parse
    /// correctly, with parameter names being syntactically accepted but not stored in
    /// the AST (since they carry no semantic information in phase-1).
    pub(super) fn parse_type_or_named_param(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Peek ahead to check for named-parameter form: `Ident Colon Type`
        // If the current token is Ident and the next token is Colon, this is a named parameter.
        if self.at(TokenKind::Ident)
            && let Some(next_tok) = self.peek_at(1)
            && next_tok.kind == TokenKind::Colon
        {
            // This is a named parameter: consume the `Ident` and `:`, then parse the type
            self.bump(); // consume `Ident`
            self.bump(); // consume `:`
            // The type is parsed; the name is implicitly discarded in phase-1
            return self.parse_type();
        }

        // Default: parse as a regular type
        self.parse_type()
    }

}
