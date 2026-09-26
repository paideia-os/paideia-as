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
                        param_names: vec![],
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

        // Parse first parameter, checking for named-parameter form (name: Type).
        // PAS-DEBT-B2-009: collect names in parallel so FnPtr construction
        // below can thread them into `param_names`; a Tuple discards names.
        let (first_type, first_name) = self.parse_type_or_named_param()?;
        let mut elements = vec![first_type];
        let mut names: Vec<Option<paideia_as_ast::NodeId>> = vec![first_name];

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

                let (elem_type, elem_name) = self.parse_type_or_named_param()?;
                span_end = self
                    .arena()
                    .get(elem_type)
                    .map(|nd| nd.span)
                    .unwrap_or(span_end);
                elements.push(elem_type);
                names.push(elem_name);

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
                // PAS-DEBT-B2-009: if any name is Some, hand param_names to
                // the FnPtr; otherwise leave it empty (backwards-compat shape
                // consumers may pattern-match).
                let param_names = if names.iter().any(|n| n.is_some()) {
                    names.clone()
                } else {
                    Vec::new()
                };
                return Ok(self.arena_mut().alloc_type(
                    NodeKind::TypeFnPtr,
                    span,
                    TypeData::FnPtr {
                        params: elements,
                        param_names,
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
            // PAS-DEBT-B2-009: single-param path — preserve name if any.
            let param_names = if names.iter().any(|n| n.is_some()) {
                names.clone()
            } else {
                Vec::new()
            };
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeFnPtr,
                span,
                TypeData::FnPtr {
                    params: elements,
                    param_names,
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
    /// Returns `(type_id, name_id_opt)` where `name_id_opt` is `Some(Ident)`
    /// for the `name: Type` form and `None` for a bare type. Callers that
    /// consume the result as a tuple element discard the name; callers that
    /// consume it as a FnPtr parameter thread the name into
    /// `TypeData::FnPtr::param_names` (PAS-DEBT-B2-009).
    ///
    /// Pre-fix (2026-09-25 debt-catalog audit) the name was consumed and
    /// dropped on the floor, so `(bar: MmioRegion) -> u32` was AST-
    /// indistinguishable from `(MmioRegion) -> u32`.
    pub(super) fn parse_type_or_named_param(
        &mut self,
    ) -> Result<(paideia_as_ast::NodeId, Option<paideia_as_ast::NodeId>), ParseError> {
        // Peek ahead to check for named-parameter form: `Ident Colon Type`.
        if self.at(TokenKind::Ident)
            && let Some(next_tok) = self.peek_at(1)
            && next_tok.kind == TokenKind::Colon
        {
            let name_tok = self.bump().expect("Ident token guaranteed by at()");
            let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);
            self.bump(); // consume `:`
            let ty = self.parse_type()?;
            return Ok((ty, Some(name_id)));
        }

        // Default: bare type, no name.
        let ty = self.parse_type()?;
        Ok((ty, None))
    }

    /// Parse a closure type: `|T1, T2, ...| -> R !{...} @{...}`.
    ///
    /// Closure types are parameterized function types with implicit environment
    /// capture. Syntax mirrors FnPtr but uses `|...|` instead of `(...)`.
    pub(super) fn parse_type_closure(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lpipe_tok = self.expect(TokenKind::Pipe)?;
        let span_start = lpipe_tok.span;

        // Parse parameter list (or empty)
        let mut params = Vec::new();
        if !self.at(TokenKind::Pipe) {
            loop {
                let param_type = self.parse_type()?;
                params.push(param_type);

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume `,`
            }
        }

        let rpipe_tok = self.expect(TokenKind::Pipe)?;
        let mut span_end = rpipe_tok.span;

        // Expect arrow
        self.expect(TokenKind::Arrow)?;

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

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeClosure,
            span,
            TypeData::Closure {
                params,
                ret,
                effects,
                capabilities,
            },
        ))
    }

    /// Parse a zero-parameter closure type from `||` (OrOr token).
    ///
    /// When the lexer scans `||`, it produces a single `OrOr` token (logical-or operator).
    /// At type-start positions, we need to recognize this as a zero-parameter closure type
    /// `|| -> R` and parse it as such.
    ///
    /// This method is called when we see `OrOr` at a type-start position and handles
    /// it as if it were two separate `Pipe` tokens: one opening, one closing (empty params).
    pub(super) fn parse_type_closure_from_oror(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let oror_tok = self.bump().expect("caller ensured OrOr token is present");
        let span_start = oror_tok.span;

        // OrOr represents || (two pipes), so we've already seen the opening pipe.
        // The closing pipe is implicit (part of OrOr).
        // Zero parameters → expect Arrow directly.
        let params = Vec::new();

        // Expect arrow
        self.expect(TokenKind::Arrow)?;

        // Parse return type
        let ret = self.parse_type()?;
        let mut span_end = self.arena().get(ret).map(|nd| nd.span).unwrap_or(span_start);

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

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeClosure,
            span,
            TypeData::Closure {
                params,
                ret,
                effects,
                capabilities,
            },
        ))
    }

}
