//! Type parsing.
//!
//! Implements §8 Type grammar: function arrows, effect rows, capability sets,
//! linear classes, and quantified types.

use paideia_as_ast::{EnumVariant, LinClass, NodeKind, TypeData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a type according to §8 Type grammar.
    ///
    /// Dispatch:
    /// 1. **forall quantifier**: if `forall` keyword, consume and parse
    ///    bound variable (discarded in phase-1), then recursively parse inner type.
    /// 2. **Linearity class prefix**: if keyword or glyph marker (`linear`, `~`, etc.),
    ///    consume, recurse, and wrap in `TypeLinearClass`.
    /// 3. **LParen**: disambiguate paren, tuple, or function arrow.
    /// 4. **Ident**: base type name, optionally with type arguments.
    /// 5. **EffectOpen (`!{`)**: parse effect row.
    /// 6. **CapOpen (`@{`)**: parse capability set (phase-1: stored as `TypeEffectRow`).
    ///
    /// Returns the `NodeId` of the allocated type node.
    pub fn parse_type(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Step 1: Handle `forall` quantifier
        if self.at(TokenKind::KwForall) {
            self.bump(); // consume `forall`

            // Expect the quantified variable name
            self.expect(TokenKind::Ident)?; // discarded in phase-1; document

            // Expect `.` separator
            self.expect(TokenKind::Dot)?;

            // Recursively parse the inner type (the quantified var is not attached)
            return self.parse_type_unquantified();
        }

        // Step 2-6: Parse non-quantified type
        self.parse_type_unquantified()
    }

    /// Parse a type without a `forall` quantifier prefix.
    ///
    /// Handles:
    /// - Linearity class prefix
    /// - LParen (paren, tuple, arrow)
    /// - Ident (type name)
    /// - EffectOpen/CapOpen (effect/capability rows)
    fn parse_type_unquantified(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Step 1: Check for linearity class prefix
        if let Some(tok) = self.peek() {
            match tok.kind {
                TokenKind::KwOrdered
                | TokenKind::KwLinear
                | TokenKind::KwAffine
                | TokenKind::KwUnrestricted
                | TokenKind::LinearMark
                | TokenKind::AffineMark => {
                    let prefix_tok = self.bump().unwrap();
                    let class = match prefix_tok.kind {
                        TokenKind::KwOrdered => LinClass::Ordered,
                        TokenKind::KwLinear => LinClass::Linear,
                        TokenKind::KwAffine => LinClass::Affine,
                        TokenKind::KwUnrestricted => LinClass::Unrestricted,
                        TokenKind::LinearMark => LinClass::LinearMark,
                        TokenKind::AffineMark => LinClass::AffineMark,
                        _ => unreachable!(),
                    };

                    // Recursively parse the inner type
                    let inner = self.parse_type_unquantified()?;

                    // Allocate TypeLinearClass node
                    let span_start = prefix_tok.span;
                    let span_end = self
                        .arena()
                        .get(inner)
                        .map(|nd| nd.span)
                        .unwrap_or(span_start);
                    let span = Span::new(
                        span_start.file(),
                        span_start.byte_start(),
                        span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
                    );

                    return Ok(self.arena_mut().alloc_type(
                        NodeKind::TypeLinearClass,
                        span,
                        TypeData::LinearClass { class, inner },
                    ));
                }
                _ => {}
            }
        }

        // Step 2: Handle pointer type prefix `*` or reference type prefix `&`/`&mut` or primary type forms
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::Star) => {
                let star_tok = self.bump().unwrap();
                if !self.is_type_start(self.peek()) {
                    return self.error_malformed_ptr(star_tok.span);
                }
                let pointee = self.parse_type_unquantified()?;
                let span_end = self
                    .arena()
                    .get(pointee)
                    .map(|nd| nd.span)
                    .unwrap_or(star_tok.span);
                let span = Span::new(
                    star_tok.span.file(),
                    star_tok.span.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - star_tok.span.byte_start(),
                );
                Ok(self
                    .arena_mut()
                    .alloc_type(NodeKind::TypePtr, span, TypeData::Ptr { pointee }))
            }
            Some(TokenKind::Amp) => {
                let amp_tok = self.bump().unwrap();

                // Check for optional `mut` keyword
                let mutable = if self.at(TokenKind::KwMut) {
                    self.bump();
                    true
                } else {
                    false
                };

                // Check for lifetime (parse-clean: consume but don't elaborate)
                // A lifetime looks like: &'name Type
                // Phase 4 m5-003: If we see an Ident that looks like a lifetime (i.e., lexeme starts with ')
                // consume it and continue to parse the actual type.
                if self.at(TokenKind::Ident) {
                    if let Some(tok) = self.peek() {
                        let source = self.source();
                        let start = tok.span.byte_start() as usize;
                        let end = (tok.span.byte_start() + tok.span.byte_len()) as usize;
                        if start < source.len() && end <= source.len() {
                            let lexeme = &source[start..end];
                            if lexeme.starts_with('\'') {
                                // This is a lifetime token; consume it but don't elaborate
                                self.bump();
                            }
                        }
                    }
                }

                if !self.is_type_start(self.peek()) {
                    return self.error_malformed_ref(amp_tok.span);
                }

                let pointee = self.parse_type_unquantified()?;
                let span_end = self
                    .arena()
                    .get(pointee)
                    .map(|nd| nd.span)
                    .unwrap_or(amp_tok.span);
                let span = Span::new(
                    amp_tok.span.file(),
                    amp_tok.span.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - amp_tok.span.byte_start(),
                );
                Ok(self.arena_mut().alloc_type(
                    NodeKind::TypeRef,
                    span,
                    TypeData::Ref { pointee, mutable },
                ))
            }
            Some(TokenKind::KwRecord) => self.parse_type_record(),
            Some(TokenKind::KwEnum) => self.parse_type_enum(),
            Some(TokenKind::LParen) => self.parse_type_paren(),
            Some(TokenKind::LBracket) => self.parse_type_array(),
            Some(TokenKind::KwSelfType) => self.parse_self_qualified_path(),
            Some(TokenKind::Ident) => self.parse_type_name(),
            Some(TokenKind::EffectOpen) => self.parse_effect_row(),
            Some(TokenKind::CapOpen) => self.parse_cap_set(),
            _ => self.error_expected_type(),
        }
    }

    /// Parse a type prefixed by `(`, disambiguating between paren, tuple, and arrow.
    ///
    /// Cases:
    /// - `()` → empty tuple
    /// - `(T)` followed by `->` → function parameter (continue arrow parse)
    /// - `(T)` not followed by `->` → parenthesized type (return inner)
    /// - `(T1, T2, ...)` → tuple
    /// - `(T1, T2, ...) ->` → function arrow
    /// - `(name: T, ...) ->` → function arrow with named parameters (names discarded in phase-1)
    fn parse_type_paren(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
                    NodeKind::TypeArrow,
                    span,
                    TypeData::Arrow {
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
                    NodeKind::TypeArrow,
                    span,
                    TypeData::Arrow {
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
                NodeKind::TypeArrow,
                span,
                TypeData::Arrow {
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
    fn parse_type_name(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
    fn parse_self_qualified_path(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
    fn parse_type_or_named_param(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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

    /// Parse an effect row: `!{ eff1, eff2 | rest }`.
    ///
    /// Syntax: `EffectOpen (idents with optional Pipe tail) RBrace`.
    /// Empty effect set `!{}` is recognized.
    pub(crate) fn parse_effect_row(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let effect_open_tok = self.expect(TokenKind::EffectOpen)?;
        let span_start = effect_open_tok.span;

        // Check for empty effect set
        if self.at(TokenKind::RBrace) {
            let rbrace_tok = self.expect(TokenKind::RBrace)?;
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeEffectRow,
                span,
                TypeData::EffectRow {
                    items: vec![],
                    rest: None,
                },
            ));
        }

        let mut items = Vec::new();

        // Parse comma-separated effect identifiers
        loop {
            if self.at(TokenKind::Ident) {
                let ident_tok = self.bump().unwrap();
                let ident_id = self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
                items.push(ident_id);

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume `,`
            } else {
                break;
            }
        }

        let mut rest = None;

        // Check for pipe tail
        if self.at(TokenKind::Pipe) {
            self.bump(); // consume `|`

            if let Some(tok) = self.peek()
                && tok.kind == TokenKind::Ident
            {
                let rest_tok = self.bump().unwrap();
                let rest_id = self.arena_mut().alloc(NodeKind::Ident, rest_tok.span);
                rest = Some(rest_id);
            }
        }

        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span_end = rbrace_tok.span;

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeEffectRow,
            span,
            TypeData::EffectRow { items, rest },
        ))
    }

    /// Parse a capability set: `@{ cap1, cap2, ... }`.
    ///
    /// Phase-1 representation: each dotted path `Mmio.read_cap` is accumulated
    /// as a sequence of Ident nodes and stored in `TypeData::EffectRow` with
    /// `rest: None` (reusing the effect row variant). A dedicated TypeData
    /// variant for capability sets can be added in a later phase if needed.
    pub(crate) fn parse_cap_set(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let cap_open_tok = self.expect(TokenKind::CapOpen)?;
        let span_start = cap_open_tok.span;

        // Check for empty capability set
        if self.at(TokenKind::RBrace) {
            let rbrace_tok = self.expect(TokenKind::RBrace)?;
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeEffectRow,
                span,
                TypeData::EffectRow {
                    items: vec![],
                    rest: None,
                },
            ));
        }

        let mut items = Vec::new();

        // Parse comma-separated capability identifiers (with optional dot-separated segments)
        loop {
            if self.at(TokenKind::Ident) {
                let ident_tok = self.bump().unwrap();

                // For phase-1, accumulate a dotted path as separate Ident nodes.
                // E.g., `Mmio.read_cap` becomes two nodes: Mmio, read_cap.
                items.push(self.arena_mut().alloc(NodeKind::Ident, ident_tok.span));

                // Check for dot-separated path continuation
                while self.at(TokenKind::Dot) {
                    self.bump(); // consume `.`

                    if let Some(next_tok) = self.peek() {
                        if next_tok.kind == TokenKind::Ident {
                            let next_ident_tok = self.bump().unwrap();
                            items
                                .push(self.arena_mut().alloc(NodeKind::Ident, next_ident_tok.span));
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume `,`
            } else {
                break;
            }
        }

        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span_end = rbrace_tok.span;

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        // Phase-1: reuse TypeEffectRow for capability sets.
        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeEffectRow,
            span,
            TypeData::EffectRow { items, rest: None },
        ))
    }

    /// Check if the next token can start a type.
    fn is_type_start(&self, opt_tok: Option<&paideia_as_lexer::Token>) -> bool {
        if let Some(tok) = opt_tok {
            matches!(
                tok.kind,
                TokenKind::Ident
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::EffectOpen
                    | TokenKind::CapOpen
                    | TokenKind::Star
                    | TokenKind::Amp
                    | TokenKind::KwRecord
                    | TokenKind::KwEnum
                    | TokenKind::KwSelfType
                    | TokenKind::KwOrdered
                    | TokenKind::KwLinear
                    | TokenKind::KwAffine
                    | TokenKind::KwUnrestricted
                    | TokenKind::LinearMark
                    | TokenKind::AffineMark
            )
        } else {
            false
        }
    }

    /// Emit a P0100 ("expected type") diagnostic and return Err.
    fn error_expected_type(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
        .message("expected type".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Emit a P0195 ("malformed pointer type") diagnostic and return Err.
    fn error_malformed_ptr(
        &mut self,
        span: paideia_as_diagnostics::Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                195,
            )
            .unwrap(),
        )
        .message("expected type after '*'".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Emit a P0196 ("malformed reference type") diagnostic and return Err.
    fn error_malformed_ref(
        &mut self,
        span: paideia_as_diagnostics::Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                196,
            )
            .unwrap(),
        )
        .message("expected type after '&'".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Parse a record type: `record { field1: Type1, field2: Type2, ... }`.
    ///
    /// Consumes `record` keyword, expects LBrace, parses field declarations
    /// (Ident : Type pairs separated by commas, trailing comma allowed),
    /// and closes with RBrace.
    fn parse_type_record(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let record_tok = self.expect(TokenKind::KwRecord)?;
        let record_span = record_tok.span;

        // Expect opening brace
        if !self.at(TokenKind::LBrace) {
            return self.error_malformed_record(
                self.peek().map(|t| t.span).unwrap_or(record_span),
                "expected '{' after 'record'",
            );
        }
        self.bump(); // consume {

        let mut fields = Vec::new();

        // Parse fields: name : type, name : type, ...
        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Expect field name (Ident)
            let field_name_tok = self.expect(TokenKind::Ident)?;
            let field_name_id = self.arena_mut().alloc(NodeKind::Ident, field_name_tok.span);

            // Expect colon
            if !self.at(TokenKind::Colon) {
                return self.error_malformed_record(
                    self.peek().map(|t| t.span).unwrap_or(field_name_tok.span),
                    "expected ':' after field name",
                );
            }
            self.bump(); // consume :

            // Parse field type
            let field_type = self.parse_type_unquantified()?;

            fields.push((field_name_id, field_type));

            // Check for comma or closing brace
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma

            // Allow trailing comma before closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }
        }

        // Expect closing brace
        if !self.at(TokenKind::RBrace) {
            return self.error_malformed_record(
                self.peek().map(|t| t.span).unwrap_or(record_span),
                "expected '}' to close record type",
            );
        }
        let rbrace_tok = self.bump().unwrap();

        // Compute span
        let span = Span::new(
            record_span.file(),
            record_span.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - record_span.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_type(NodeKind::TypeRecord, span, TypeData::Record { fields }))
    }

    /// Emit a P0197 ("malformed record type") diagnostic and return Err.
    fn error_malformed_record(
        &mut self,
        span: paideia_as_diagnostics::Span,
        reason: &str,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                197,
            )
            .unwrap(),
        )
        .message(format!("malformed record type: {}", reason))
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Parse an enum type: `enum { Variant1, Variant2(T1, T2), Variant3 { f1: T1 }, ... }`.
    ///
    /// Consumes `enum` keyword, expects LBrace, parses variants (unit, tuple, or record payload),
    /// separated by Comma (trailing OK), closes RBrace.
    fn parse_type_enum(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let enum_tok = self.expect(TokenKind::KwEnum)?;
        let enum_span = enum_tok.span;

        // Expect opening brace
        if !self.at(TokenKind::LBrace) {
            return self.error_malformed_enum(
                self.peek().map(|t| t.span).unwrap_or(enum_span),
                "expected '{' after 'enum'",
            );
        }
        self.bump(); // consume {

        let mut variants = Vec::new();

        // Parse variants
        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Expect variant name (Ident)
            let variant_name_tok = self.expect(TokenKind::Ident)?;
            let variant_name_id = self
                .arena_mut()
                .alloc(NodeKind::Ident, variant_name_tok.span);

            // Peek ahead to determine variant shape: unit, tuple, or record
            let variant = if self.at(TokenKind::LParen) {
                // Tuple variant: Ident ( Type (, Type)* (,)? )
                self.bump(); // consume (

                let mut payload = Vec::new();

                // Parse tuple payload
                loop {
                    if self.at(TokenKind::RParen) {
                        break;
                    }

                    let ty = self.parse_type_unquantified()?;
                    payload.push(ty);

                    if !self.at(TokenKind::Comma) {
                        break;
                    }
                    self.bump(); // consume comma

                    // Allow trailing comma before closing paren
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                }

                // Expect closing paren
                if !self.at(TokenKind::RParen) {
                    return self.error_malformed_enum(
                        self.peek().map(|t| t.span).unwrap_or(variant_name_tok.span),
                        "expected ')' to close tuple variant",
                    );
                }
                self.bump(); // consume )

                EnumVariant::Tuple {
                    name: variant_name_id,
                    payload,
                }
            } else if self.at(TokenKind::LBrace) {
                // Record variant: Ident { Ident : Type (, ...)* (,)? }
                self.bump(); // consume {

                let mut fields = Vec::new();

                // Parse record payload
                loop {
                    if self.at(TokenKind::RBrace) {
                        break;
                    }

                    let field_name_tok = self.expect(TokenKind::Ident)?;
                    let field_name_id =
                        self.arena_mut().alloc(NodeKind::Ident, field_name_tok.span);

                    // Expect colon
                    if !self.at(TokenKind::Colon) {
                        return self.error_malformed_enum(
                            self.peek().map(|t| t.span).unwrap_or(field_name_tok.span),
                            "expected ':' after field name in record variant",
                        );
                    }
                    self.bump(); // consume :

                    let field_type = self.parse_type_unquantified()?;
                    fields.push((field_name_id, field_type));

                    if !self.at(TokenKind::Comma) {
                        break;
                    }
                    self.bump(); // consume comma

                    // Allow trailing comma before closing brace
                    if self.at(TokenKind::RBrace) {
                        break;
                    }
                }

                // Expect closing brace for record variant
                if !self.at(TokenKind::RBrace) {
                    return self.error_malformed_enum(
                        self.peek().map(|t| t.span).unwrap_or(variant_name_tok.span),
                        "expected '}' to close record variant",
                    );
                }
                self.bump(); // consume }

                EnumVariant::Record {
                    name: variant_name_id,
                    fields,
                }
            } else {
                // Unit variant: just Ident
                EnumVariant::Unit {
                    name: variant_name_id,
                }
            };

            variants.push(variant);

            // Check for comma or closing brace
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma

            // Allow trailing comma before closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }
        }

        // Expect closing brace
        if !self.at(TokenKind::RBrace) {
            return self.error_malformed_enum(
                self.peek().map(|t| t.span).unwrap_or(enum_span),
                "expected '}' to close enum type",
            );
        }
        let rbrace_tok = self.bump().unwrap();

        // Compute span
        let span = Span::new(
            enum_span.file(),
            enum_span.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - enum_span.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_type(NodeKind::TypeEnum, span, TypeData::Enum { variants }))
    }

    /// Emit a P0198 ("malformed enum type") diagnostic and return Err.
    fn error_malformed_enum(
        &mut self,
        span: paideia_as_diagnostics::Span,
        reason: &str,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                198,
            )
            .unwrap(),
        )
        .message(format!("malformed enum type: {}", reason))
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Parse a fixed-size array type: `[T; N]`.
    ///
    /// Syntax: `LBracket Type Semicolon Expr RBracket`.
    /// The length is parsed as a primary expression (any primary expression is valid
    /// syntactically; semantic constraints to constant values are enforced at
    /// type elaboration, not at parse time).
    ///
    /// Returns a TypeArray node with element type and length expression.
    ///
    /// Errors:
    /// - P0199: malformed array type (missing length, missing `;`, etc.)
    fn parse_type_array(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lbracket_tok = self.expect(TokenKind::LBracket)?;
        let span_start = lbracket_tok.span;

        // Parse element type
        let element = self.parse_type_unquantified()?;

        // Expect semicolon
        if !self.at(TokenKind::Semicolon) {
            return self.error_malformed_array(
                self.peek().map(|t| t.span).unwrap_or(span_start),
                "expected ';' after array element type",
            );
        }
        self.bump(); // consume `;`

        // Parse length expression (as a primary expression)
        let length = self.parse_primary()?;

        // Expect closing bracket
        let rbracket_tok = self.expect(TokenKind::RBracket)?;

        // Compute span
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbracket_tok.span.byte_start() + rbracket_tok.span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeArray,
            span,
            TypeData::Array { element, length },
        ))
    }

    /// Emit a P0199 ("malformed array type") diagnostic and return Err.
    fn error_malformed_array(
        &mut self,
        span: paideia_as_diagnostics::Span,
        reason: &str,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                199,
            )
            .unwrap(),
        )
        .message(format!("malformed array type: {}", reason))
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

    fn tok(kind: TokenKind, byte_start: u32) -> Token {
        Token::new(kind, Span::new(FileId::new(1).unwrap(), byte_start, 1))
    }

    fn parse_t(
        tokens: Vec<Token>,
    ) -> (
        AstArena,
        Result<paideia_as_ast::NodeId, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_type()
        };
        (arena, result, sink.diagnostics().to_vec())
    }

    #[test]
    fn parse_simple_type_name() {
        let tokens = vec![tok(TokenKind::Ident, 0), tok(TokenKind::Eof, 1)];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeName);
        if let Some(TypeData::Name { args, .. }) = arena.type_data(ty_id) {
            assert_eq!(args.len(), 0);
        } else {
            panic!("expected TypeName");
        }
    }

    #[test]
    fn parse_type_with_args() {
        // `Map(K, V)` → Ident LParen Ident Comma Ident RParen Eof
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // Map
            tok(TokenKind::LParen, 3), // (
            tok(TokenKind::Ident, 4),  // K
            tok(TokenKind::Comma, 5),  // ,
            tok(TokenKind::Ident, 7),  // V
            tok(TokenKind::RParen, 8), // )
            tok(TokenKind::Eof, 9),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Name { args, .. }) = arena.type_data(ty_id) {
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected TypeName with args");
        }
    }

    #[test]
    fn parse_tuple_type() {
        // `(u64, u64)` → LParen Ident Comma Ident RParen Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // u64
            tok(TokenKind::Comma, 4),
            tok(TokenKind::Ident, 6), // u64
            tok(TokenKind::RParen, 9),
            tok(TokenKind::Eof, 10),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeTuple);
        if let Some(TypeData::Tuple { elements }) = arena.type_data(ty_id) {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("expected TypeTuple");
        }
    }

    #[test]
    fn parse_arrow_type() {
        // `(u64) -> u64` → LParen Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // u64
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Ident, 9), // u64
            tok(TokenKind::Eof, 12),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow {
            params,
            effects,
            capabilities,
            ..
        }) = arena.type_data(ty_id)
        {
            assert_eq!(params.len(), 1);
            assert!(effects.is_none());
            assert!(capabilities.is_none());
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parse_arrow_with_effects() {
        // `(u64) -> u64 !{io}` → LParen Ident RParen Arrow Ident EffectOpen Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // u64
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Ident, 9), // u64
            tok(TokenKind::EffectOpen, 13),
            tok(TokenKind::Ident, 15), // io
            tok(TokenKind::RBrace, 17),
            tok(TokenKind::Eof, 18),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Arrow { effects, .. }) = arena.type_data(ty_id) {
            assert!(effects.is_some());
        } else {
            panic!("expected TypeArrow with effects");
        }
    }

    #[test]
    fn parse_arrow_with_capabilities() {
        // `(u64) -> u64 @{cap}` → LParen Ident RParen Arrow Ident CapOpen Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // u64
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Ident, 9), // u64
            tok(TokenKind::CapOpen, 13),
            tok(TokenKind::Ident, 15), // cap
            tok(TokenKind::RBrace, 18),
            tok(TokenKind::Eof, 19),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Arrow { capabilities, .. }) = arena.type_data(ty_id) {
            assert!(capabilities.is_some());
        } else {
            panic!("expected TypeArrow with capabilities");
        }
    }

    #[test]
    fn parse_arrow_full() {
        // `(u64, linear Cap) -> u64 !{io} @{Mmio.read_cap}`
        // LParen Ident Comma KwLinear Ident RParen Arrow Ident EffectOpen Ident RBrace CapOpen Ident Dot Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // u64
            tok(TokenKind::Comma, 4),
            tok(TokenKind::KwLinear, 6),
            tok(TokenKind::Ident, 12), // Cap
            tok(TokenKind::RParen, 15),
            tok(TokenKind::Arrow, 17),
            tok(TokenKind::Ident, 20), // u64
            tok(TokenKind::EffectOpen, 24),
            tok(TokenKind::Ident, 26), // io
            tok(TokenKind::RBrace, 28),
            tok(TokenKind::CapOpen, 30),
            tok(TokenKind::Ident, 32), // Mmio
            tok(TokenKind::Dot, 36),
            tok(TokenKind::Ident, 37), // read_cap
            tok(TokenKind::RBrace, 45),
            tok(TokenKind::Eof, 46),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow {
            params,
            effects,
            capabilities,
            ..
        }) = arena.type_data(ty_id)
        {
            assert_eq!(params.len(), 2);
            // Second param should be TypeLinearClass with Linear
            let param2 = params[1];
            let param2_node = arena.get(param2).unwrap();
            assert_eq!(param2_node.kind, NodeKind::TypeLinearClass);
            assert!(effects.is_some());
            assert!(capabilities.is_some());
        } else {
            panic!("expected TypeArrow full");
        }
    }

    #[test]
    fn parse_linear_class_keyword() {
        // `linear T` → KwLinear Ident Eof
        let tokens = vec![
            tok(TokenKind::KwLinear, 0),
            tok(TokenKind::Ident, 7),
            tok(TokenKind::Eof, 8),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeLinearClass);
        if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
            assert_eq!(*class, LinClass::Linear);
        } else {
            panic!("expected TypeLinearClass");
        }
    }

    #[test]
    fn parse_linear_class_glyph() {
        // `↓ T` → LinearMark Ident Eof
        let tokens = vec![
            tok(TokenKind::LinearMark, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeLinearClass);
        if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
            assert_eq!(*class, LinClass::LinearMark);
        } else {
            panic!("expected TypeLinearClass with LinearMark");
        }
    }

    #[test]
    fn parse_affine_glyph() {
        // `~ T` → AffineMark Ident Eof
        let tokens = vec![
            tok(TokenKind::AffineMark, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeLinearClass);
        if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
            assert_eq!(*class, LinClass::AffineMark);
        } else {
            panic!("expected TypeLinearClass with AffineMark");
        }
    }

    #[test]
    fn parse_forall_quantified() {
        // `forall e. (T) -> T !{Io | e}` (bound var discarded in phase-1)
        // KwForall Ident Dot LParen Ident RParen Arrow Ident EffectOpen Ident Pipe Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::KwForall, 0),
            tok(TokenKind::Ident, 7), // e
            tok(TokenKind::Dot, 8),
            tok(TokenKind::LParen, 10),
            tok(TokenKind::Ident, 11), // T
            tok(TokenKind::RParen, 12),
            tok(TokenKind::Arrow, 14),
            tok(TokenKind::Ident, 17), // T
            tok(TokenKind::EffectOpen, 19),
            tok(TokenKind::Ident, 21), // Io
            tok(TokenKind::Pipe, 23),
            tok(TokenKind::Ident, 25), // e
            tok(TokenKind::RBrace, 26),
            tok(TokenKind::Eof, 27),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        // The outer node should be an arrow (forall wrapper is discarded in phase-1)
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
    }

    #[test]
    fn parse_empty_effect_set() {
        // `!{}` → EffectOpen RBrace Eof
        let tokens = vec![
            tok(TokenKind::EffectOpen, 0),
            tok(TokenKind::RBrace, 2),
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeEffectRow);
        if let Some(TypeData::EffectRow { items, rest }) = arena.type_data(ty_id) {
            assert_eq!(items.len(), 0);
            assert!(rest.is_none());
        } else {
            panic!("expected TypeEffectRow empty");
        }
    }

    #[test]
    fn parse_empty_cap_set() {
        // `@{}` → CapOpen RBrace Eof
        let tokens = vec![
            tok(TokenKind::CapOpen, 0),
            tok(TokenKind::RBrace, 2),
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeEffectRow);
        if let Some(TypeData::EffectRow { items, rest }) = arena.type_data(ty_id) {
            assert_eq!(items.len(), 0);
            assert!(rest.is_none());
        } else {
            panic!("expected TypeEffectRow empty cap");
        }
    }

    // Tests for named-parameter function types (issue #154)

    #[test]
    fn parses_function_type_with_named_param() {
        // `(bar: MmioRegion) -> u32`
        // LParen Ident Colon Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // bar
            tok(TokenKind::Colon, 4), // :
            tok(TokenKind::Ident, 5), // MmioRegion
            tok(TokenKind::RParen, 16),
            tok(TokenKind::Arrow, 18),
            tok(TokenKind::Ident, 21), // u32
            tok(TokenKind::Eof, 24),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(
            diags.len(),
            0,
            "no diagnostics expected for named-param type"
        );
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(
            ty_node.kind,
            NodeKind::TypeArrow,
            "expected arrow type for named-param function"
        );
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 1, "expected 1 parameter");
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parses_function_type_with_two_named_params() {
        // `(a: u32, b: u64) -> u32`
        // LParen Ident Colon Ident Comma Ident Colon Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // a
            tok(TokenKind::Colon, 2), // :
            tok(TokenKind::Ident, 3), // u32
            tok(TokenKind::Comma, 6),
            tok(TokenKind::Ident, 8),  // b
            tok(TokenKind::Colon, 9),  // :
            tok(TokenKind::Ident, 10), // u64
            tok(TokenKind::RParen, 14),
            tok(TokenKind::Arrow, 16),
            tok(TokenKind::Ident, 19), // u32
            tok(TokenKind::Eof, 22),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 2, "expected 2 parameters");
        } else {
            panic!("expected TypeArrow with two params");
        }
    }

    #[test]
    fn parses_function_type_positional_regression() {
        // `(u32, u64) -> u32` (positional, no names) — should still work
        // LParen Ident Comma Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // u32
            tok(TokenKind::Comma, 4),
            tok(TokenKind::Ident, 6), // u64
            tok(TokenKind::RParen, 9),
            tok(TokenKind::Arrow, 11),
            tok(TokenKind::Ident, 14), // u32
            tok(TokenKind::Eof, 17),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(
            diags.len(),
            0,
            "no diagnostics expected for positional form"
        );
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 2, "expected 2 positional parameters");
        } else {
            panic!("expected TypeArrow positional");
        }
    }

    #[test]
    fn parses_function_type_mixed_named_and_positional() {
        // `(name: T, U) -> V` (mixed form: named then positional)
        // LParen Ident Colon Ident Comma Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1), // name
            tok(TokenKind::Colon, 5), // :
            tok(TokenKind::Ident, 6), // T
            tok(TokenKind::Comma, 7),
            tok(TokenKind::Ident, 9), // U
            tok(TokenKind::RParen, 10),
            tok(TokenKind::Arrow, 12),
            tok(TokenKind::Ident, 15), // V
            tok(TokenKind::Eof, 16),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "mixed form should parse cleanly");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 2, "expected 2 parameters (mixed)");
        } else {
            panic!("expected TypeArrow mixed");
        }
    }

    #[test]
    fn parses_function_type_zero_args_with_paren() {
        // `() -> u32` (empty params) — should still work
        // LParen RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::RParen, 1),
            tok(TokenKind::Arrow, 2),
            tok(TokenKind::Ident, 4), // u32
            tok(TokenKind::Eof, 7),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected for empty params");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 0, "expected 0 parameters");
        } else {
            panic!("expected TypeArrow empty");
        }
    }

    #[test]
    fn parses_function_type_nested_named_param_types() {
        // `(f: (n: u32) -> u32) -> u32`
        // LParen Ident Colon LParen Ident Colon Ident RParen Arrow Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1),   // f
            tok(TokenKind::Colon, 2),   // :
            tok(TokenKind::LParen, 3),  // (
            tok(TokenKind::Ident, 4),   // n
            tok(TokenKind::Colon, 5),   // :
            tok(TokenKind::Ident, 6),   // u32
            tok(TokenKind::RParen, 9),  // )
            tok(TokenKind::Arrow, 11),  // ->
            tok(TokenKind::Ident, 14),  // u32
            tok(TokenKind::RParen, 17), // )
            tok(TokenKind::Arrow, 19),  // ->
            tok(TokenKind::Ident, 22),  // u32
            tok(TokenKind::Eof, 25),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(
            diags.len(),
            0,
            "no diagnostics for nested named-param types"
        );
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 1, "expected 1 parameter (a function type)");
            // Check that the param itself is an arrow
            let param_type = params[0];
            let param_node = arena.get(param_type).unwrap();
            assert_eq!(param_node.kind, NodeKind::TypeArrow);
        } else {
            panic!("expected outer TypeArrow");
        }
    }

    // === Pointer type tests ===

    #[test]
    fn parse_ptr_simple() {
        // `*u64` → Star Ident Eof
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::Eof, 5),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypePtr);
        if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
            let pointee_node = arena.get(*pointee).unwrap();
            assert_eq!(pointee_node.kind, NodeKind::TypeName);
        } else {
            panic!("expected TypePtr");
        }
    }

    #[test]
    fn parse_ptr_nested() {
        // `**u8` → Star Star Ident Eof
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::Star, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::Eof, 4),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypePtr);
        if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
            let inner_node = arena.get(*pointee).unwrap();
            assert_eq!(inner_node.kind, NodeKind::TypePtr);
        } else {
            panic!("expected outer TypePtr");
        }
    }

    #[test]
    fn parse_ptr_tuple() {
        // `*(u8, u64)` → Star LParen Ident Comma Ident RParen Eof
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::LParen, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::Comma, 4),
            tok(TokenKind::Ident, 6),
            tok(TokenKind::RParen, 9),
            tok(TokenKind::Eof, 10),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypePtr);
        if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
            let tuple_node = arena.get(*pointee).unwrap();
            assert_eq!(tuple_node.kind, NodeKind::TypeTuple);
        } else {
            panic!("expected TypePtr");
        }
    }

    #[test]
    fn parse_ptr_fn() {
        // `*((u64) -> u64)` → Star LParen LParen Ident RParen Arrow Ident RParen Eof
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::LParen, 1),
            tok(TokenKind::LParen, 2),
            tok(TokenKind::Ident, 3),
            tok(TokenKind::RParen, 6),
            tok(TokenKind::Arrow, 8),
            tok(TokenKind::Ident, 11),
            tok(TokenKind::RParen, 14),
            tok(TokenKind::Eof, 15),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypePtr);
        if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
            let fn_node = arena.get(*pointee).unwrap();
            assert_eq!(fn_node.kind, NodeKind::TypeArrow);
        } else {
            panic!("expected TypePtr");
        }
    }

    #[test]
    fn parse_ptr_in_arrow_param() {
        // `(*u8) -> u64` → LParen Star Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Star, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Ident, 9),
            tok(TokenKind::Eof, 12),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 1, "expected 1 parameter");
            let param_node = arena.get(params[0]).unwrap();
            assert_eq!(param_node.kind, NodeKind::TypePtr);
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parse_ptr_in_arrow_ret() {
        // `(u64) -> *u8` → LParen Ident RParen Arrow Star Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Star, 9),
            tok(TokenKind::Ident, 10),
            tok(TokenKind::Eof, 12),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { ret, .. }) = arena.type_data(ty_id) {
            let ret_node = arena.get(*ret).unwrap();
            assert_eq!(ret_node.kind, NodeKind::TypePtr);
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parse_ptr_p0195_no_operand() {
        // `*` Eof → expect P0195 diagnostic
        let tokens = vec![tok(TokenKind::Star, 0), tok(TokenKind::Eof, 1)];
        let (_arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 1, "expected 1 diagnostic");
        let diag = &diags[0];
        assert_eq!(diag.code().number(), 195, "expected P0195");
        assert!(result.is_err(), "expected parse error");
    }

    #[test]
    fn parse_ptr_p0195_before_arrow() {
        // `*` Arrow Ident Eof → expect P0195 diagnostic
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::Arrow, 1),
            tok(TokenKind::Ident, 4),
            tok(TokenKind::Eof, 7),
        ];
        let (_arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 1, "expected 1 diagnostic");
        let diag = &diags[0];
        assert_eq!(diag.code().number(), 195, "expected P0195");
        assert!(result.is_err(), "expected parse error");
    }

    // === Round-trip tests (parse + print_type) ===

    #[test]
    fn roundtrip_ptr_simple() {
        // `*u8` parsed and printed should remain `*u8`
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, _diags) = parse_t(tokens);

        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let printed = paideia_as_ast::pretty::print_type(&arena, ty_id);
        assert!(printed.contains("Ptr"));
    }

    #[test]
    fn roundtrip_ptr_nested() {
        // `**u8` parsed should have nested TypePtr nodes
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::Star, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::Eof, 4),
        ];
        let (arena, result, _diags) = parse_t(tokens);

        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let printed = paideia_as_ast::pretty::print_type(&arena, ty_id);
        // Should have outer Ptr wrapping inner Ptr
        assert!(printed.contains("Ptr"));
    }

    #[test]
    fn roundtrip_ptr_in_tuple() {
        // `*(u8, u64)` parsed should have Ptr wrapping Tuple
        let tokens = vec![
            tok(TokenKind::Star, 0),
            tok(TokenKind::LParen, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::Comma, 4),
            tok(TokenKind::Ident, 6),
            tok(TokenKind::RParen, 9),
            tok(TokenKind::Eof, 10),
        ];
        let (arena, result, _diags) = parse_t(tokens);

        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypePtr);
        if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
            let inner_node = arena.get(*pointee).unwrap();
            assert_eq!(inner_node.kind, NodeKind::TypeTuple);
        } else {
            panic!("expected TypePtr");
        }
    }

    #[test]
    fn roundtrip_ptr_in_arrow() {
        // `(*u8) -> *u64` parsed should have Ptr in both params and return
        // Tokens: LParen Star Ident RParen Arrow Star Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Star, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Star, 9),
            tok(TokenKind::Ident, 10),
            tok(TokenKind::Eof, 12),
        ];
        let (arena, result, _diags) = parse_t(tokens);

        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, ret, .. }) = arena.type_data(ty_id) {
            // First param should be *u8
            assert_eq!(params.len(), 1);
            let param_node = arena.get(params[0]).unwrap();
            assert_eq!(param_node.kind, NodeKind::TypePtr);
            // Return type should be *u64
            let ret_node = arena.get(*ret).unwrap();
            assert_eq!(ret_node.kind, NodeKind::TypePtr);
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parse_enum_unit_variants_only() {
        // `enum { A, B, C }` → KwEnum LBrace Ident Comma Ident Comma Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // A
            tok(TokenKind::Comma, 8),
            tok(TokenKind::Ident, 10), // B
            tok(TokenKind::Comma, 11),
            tok(TokenKind::Ident, 13), // C
            tok(TokenKind::RBrace, 14),
            tok(TokenKind::Eof, 15),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeEnum);
        if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
            assert_eq!(variants.len(), 3);
            // All should be unit variants
            for var in variants {
                if let paideia_as_ast::EnumVariant::Unit { .. } = var {
                    // OK
                } else {
                    panic!("expected unit variant");
                }
            }
        } else {
            panic!("expected TypeEnum");
        }
    }

    #[test]
    fn parse_enum_tuple_variants() {
        // `enum { Some(u64), None }` → KwEnum LBrace Ident LParen Ident RParen Comma Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // Some
            tok(TokenKind::LParen, 11),
            tok(TokenKind::Ident, 12), // u64
            tok(TokenKind::RParen, 15),
            tok(TokenKind::Comma, 16),
            tok(TokenKind::Ident, 18), // None
            tok(TokenKind::RBrace, 22),
            tok(TokenKind::Eof, 23),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
            assert_eq!(variants.len(), 2);
            // First should be tuple variant
            if let paideia_as_ast::EnumVariant::Tuple { payload, .. } = &variants[0] {
                assert_eq!(payload.len(), 1);
            } else {
                panic!("expected tuple variant");
            }
            // Second should be unit variant
            if let paideia_as_ast::EnumVariant::Unit { .. } = &variants[1] {
                // OK
            } else {
                panic!("expected unit variant");
            }
        } else {
            panic!("expected TypeEnum");
        }
    }

    #[test]
    fn parse_enum_record_variants() {
        // `enum { Pair { a: u8, b: u8 } }`
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // Pair
            tok(TokenKind::LBrace, 12),
            tok(TokenKind::Ident, 14), // a
            tok(TokenKind::Colon, 15),
            tok(TokenKind::Ident, 17), // u8
            tok(TokenKind::Comma, 19),
            tok(TokenKind::Ident, 21), // b
            tok(TokenKind::Colon, 22),
            tok(TokenKind::Ident, 24), // u8
            tok(TokenKind::RBrace, 26),
            tok(TokenKind::RBrace, 27),
            tok(TokenKind::Eof, 28),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
            assert_eq!(variants.len(), 1);
            if let paideia_as_ast::EnumVariant::Record { fields, .. } = &variants[0] {
                assert_eq!(fields.len(), 2);
            } else {
                panic!("expected record variant");
            }
        } else {
            panic!("expected TypeEnum");
        }
    }

    #[test]
    fn parse_enum_mixed_variants() {
        // `enum { Unit, T(u8), R { x: u8 } }`
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // Unit
            tok(TokenKind::Comma, 11),
            tok(TokenKind::Ident, 13), // T
            tok(TokenKind::LParen, 14),
            tok(TokenKind::Ident, 15), // u8
            tok(TokenKind::RParen, 17),
            tok(TokenKind::Comma, 18),
            tok(TokenKind::Ident, 20), // R
            tok(TokenKind::LBrace, 22),
            tok(TokenKind::Ident, 24), // x
            tok(TokenKind::Colon, 25),
            tok(TokenKind::Ident, 27), // u8
            tok(TokenKind::RBrace, 29),
            tok(TokenKind::RBrace, 30),
            tok(TokenKind::Eof, 31),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
            assert_eq!(variants.len(), 3);
            // First unit
            assert!(matches!(
                variants[0],
                paideia_as_ast::EnumVariant::Unit { .. }
            ));
            // Second tuple
            assert!(matches!(
                variants[1],
                paideia_as_ast::EnumVariant::Tuple { .. }
            ));
            // Third record
            assert!(matches!(
                variants[2],
                paideia_as_ast::EnumVariant::Record { .. }
            ));
        } else {
            panic!("expected TypeEnum");
        }
    }

    #[test]
    fn parse_enum_trailing_comma() {
        // `enum { A, B, }` → KwEnum LBrace Ident Comma Ident Comma RBrace Eof
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // A
            tok(TokenKind::Comma, 8),
            tok(TokenKind::Ident, 10), // B
            tok(TokenKind::Comma, 11),
            tok(TokenKind::RBrace, 12),
            tok(TokenKind::Eof, 13),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
            assert_eq!(variants.len(), 2);
        } else {
            panic!("expected TypeEnum");
        }
    }

    #[test]
    fn parse_enum_p0198_missing_lbrace() {
        // `enum (` → missing { after enum
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LParen, 5),
            tok(TokenKind::Eof, 6),
        ];
        let (arena, result, diags) = parse_t(tokens);

        // Should error with P0198 (malformed enum)
        assert!(result.is_err());
        assert!(diags.len() > 0);
        // The diagnostic code should be 198 for malformed enum
        assert!(diags.iter().any(|d| d.code().number() == 198));
    }

    #[test]
    fn parse_enum_p0198_missing_rparen() {
        // `enum { Some(u64 }` → missing ) in tuple variant
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // Some
            tok(TokenKind::LParen, 11),
            tok(TokenKind::Ident, 12), // u64
            tok(TokenKind::RBrace, 16),
            tok(TokenKind::Eof, 17),
        ];
        let (arena, result, diags) = parse_t(tokens);

        // Should error with P0198
        assert!(result.is_err());
        assert!(diags.len() > 0);
        assert_eq!(diags[0].code().number(), 198);
    }

    #[test]
    fn parse_enum_p0198_missing_rbrace() {
        // `enum { A, B` → missing closing }
        let tokens = vec![
            tok(TokenKind::KwEnum, 0),
            tok(TokenKind::LBrace, 5),
            tok(TokenKind::Ident, 7), // A
            tok(TokenKind::Comma, 8),
            tok(TokenKind::Ident, 10), // B
            tok(TokenKind::Eof, 11),
        ];
        let (arena, result, diags) = parse_t(tokens);

        // Should error with P0198
        assert!(result.is_err());
        assert!(diags.len() > 0);
        assert_eq!(diags[0].code().number(), 198);
    }

    // === Reference type tests (Phase 4 m4-001) ===

    #[test]
    fn parse_ref_simple() {
        // `&u64` → Amp Ident Eof
        let tokens = vec![
            tok(TokenKind::Amp, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::Eof, 5),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeRef);
        if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
            assert!(!mutable, "expected immutable reference");
            let pointee_node = arena.get(*pointee).unwrap();
            assert_eq!(pointee_node.kind, NodeKind::TypeName);
        } else {
            panic!("expected TypeRef");
        }
    }

    #[test]
    fn parse_ref_mut() {
        // `&mut u64` → Amp KwMut Ident Eof
        let tokens = vec![
            tok(TokenKind::Amp, 0),
            tok(TokenKind::KwMut, 1),
            tok(TokenKind::Ident, 4),
            tok(TokenKind::Eof, 8),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeRef);
        if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
            assert!(mutable, "expected mutable reference");
            let pointee_node = arena.get(*pointee).unwrap();
            assert_eq!(pointee_node.kind, NodeKind::TypeName);
        } else {
            panic!("expected TypeRef");
        }
    }

    #[test]
    fn parse_ref_nested() {
        // `&&u8` → Amp Amp Ident Eof
        let tokens = vec![
            tok(TokenKind::Amp, 0),
            tok(TokenKind::Amp, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::Eof, 4),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeRef);
        if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
            assert!(!mutable, "expected immutable reference");
            let inner_node = arena.get(*pointee).unwrap();
            assert_eq!(inner_node.kind, NodeKind::TypeRef);
        } else {
            panic!("expected outer TypeRef");
        }
    }

    #[test]
    fn parse_ref_with_lifetime() {
        // `&'a u64` → Amp Ident(lifetime) Ident Eof (parse-clean: lifetime consumed but not elaborated)
        let tokens = vec![
            tok(TokenKind::Amp, 0),
            tok(TokenKind::Ident, 1), // 'a
            tok(TokenKind::Ident, 3), // u64
            tok(TokenKind::Eof, 7),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeRef);
        if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
            assert!(!mutable, "expected immutable reference");
            let pointee_node = arena.get(*pointee).unwrap();
            assert_eq!(pointee_node.kind, NodeKind::TypeName);
        } else {
            panic!("expected TypeRef");
        }
    }

    #[test]
    fn parse_ref_in_arrow_param() {
        // `(&u8) -> u64` → LParen Amp Ident RParen Arrow Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Amp, 1),
            tok(TokenKind::Ident, 2),
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Ident, 9),
            tok(TokenKind::Eof, 12),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { params, .. }) = arena.type_data(ty_id) {
            assert_eq!(params.len(), 1, "expected 1 parameter");
            let param_node = arena.get(params[0]).unwrap();
            assert_eq!(param_node.kind, NodeKind::TypeRef);
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parse_ref_in_arrow_ret() {
        // `(u64) -> &u8` → LParen Ident RParen Arrow Amp Ident Eof
        let tokens = vec![
            tok(TokenKind::LParen, 0),
            tok(TokenKind::Ident, 1),
            tok(TokenKind::RParen, 4),
            tok(TokenKind::Arrow, 6),
            tok(TokenKind::Amp, 9),
            tok(TokenKind::Ident, 10),
            tok(TokenKind::Eof, 12),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArrow);
        if let Some(TypeData::Arrow { ret, .. }) = arena.type_data(ty_id) {
            let ret_node = arena.get(*ret).unwrap();
            assert_eq!(ret_node.kind, NodeKind::TypeRef);
        } else {
            panic!("expected TypeArrow");
        }
    }

    #[test]
    fn parse_ref_of_record() {
        // `&record { a: u8 }` → Amp KwRecord LBrace Ident Colon Ident RBrace Eof
        let tokens = vec![
            tok(TokenKind::Amp, 0),
            tok(TokenKind::KwRecord, 1),
            tok(TokenKind::LBrace, 7),
            tok(TokenKind::Ident, 9), // a
            tok(TokenKind::Colon, 10),
            tok(TokenKind::Ident, 12), // u8
            tok(TokenKind::RBrace, 14),
            tok(TokenKind::Eof, 15),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeRef);
        if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
            assert!(!mutable, "expected immutable reference");
            let record_node = arena.get(*pointee).unwrap();
            assert_eq!(record_node.kind, NodeKind::TypeRecord);
        } else {
            panic!("expected TypeRef");
        }
    }

    #[test]
    fn parse_ref_p0196_no_type() {
        // `&` Eof → expect P0196 diagnostic
        let tokens = vec![tok(TokenKind::Amp, 0), tok(TokenKind::Eof, 1)];
        let (_arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 1, "expected 1 diagnostic");
        let diag = &diags[0];
        assert_eq!(diag.code().number(), 196, "expected P0196");
        assert!(result.is_err(), "expected parse error");
    }

    #[test]
    fn parse_ref_p0196_mut_no_type() {
        // `&mut` Eof → expect P0196 diagnostic
        let tokens = vec![
            tok(TokenKind::Amp, 0),
            tok(TokenKind::KwMut, 1),
            tok(TokenKind::Eof, 4),
        ];
        let (_arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 1, "expected 1 diagnostic");
        let diag = &diags[0];
        assert_eq!(diag.code().number(), 196, "expected P0196");
        assert!(result.is_err(), "expected parse error");
    }

    // === Fixed-size array type tests ===

    #[test]
    fn parse_array_u8_zero() {
        // `[u8; 0]` → LBracket Ident Semicolon IntLit RBracket Eof
        let tokens = vec![
            tok(TokenKind::LBracket, 0),
            tok(TokenKind::Ident, 1), // u8
            tok(TokenKind::Semicolon, 3),
            tok(TokenKind::IntLit, 4), // 0
            tok(TokenKind::RBracket, 5),
            tok(TokenKind::Eof, 6),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArray);
        if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
            let elem_node = arena.get(*element).unwrap();
            assert_eq!(elem_node.kind, NodeKind::TypeName);
            let len_node = arena.get(*length).unwrap();
            assert_eq!(len_node.kind, NodeKind::ExprLiteral);
        } else {
            panic!("expected TypeArray");
        }
    }

    #[test]
    fn parse_array_u8_sixteen() {
        // `[u8; 16]` → LBracket Ident Semicolon IntLit RBracket Eof
        let tokens = vec![
            tok(TokenKind::LBracket, 0),
            tok(TokenKind::Ident, 1), // u8
            tok(TokenKind::Semicolon, 3),
            tok(TokenKind::IntLit, 4), // 16
            tok(TokenKind::RBracket, 6),
            tok(TokenKind::Eof, 7),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArray);
        if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
            let elem_node = arena.get(*element).unwrap();
            assert_eq!(elem_node.kind, NodeKind::TypeName);
            let len_node = arena.get(*length).unwrap();
            assert_eq!(len_node.kind, NodeKind::ExprLiteral);
        } else {
            panic!("expected TypeArray");
        }
    }

    #[test]
    fn parse_array_u64_five() {
        // `[u64; 5]` → LBracket Ident Semicolon IntLit RBracket Eof
        let tokens = vec![
            tok(TokenKind::LBracket, 0),
            tok(TokenKind::Ident, 1), // u64
            tok(TokenKind::Semicolon, 4),
            tok(TokenKind::IntLit, 5), // 5
            tok(TokenKind::RBracket, 6),
            tok(TokenKind::Eof, 7),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArray);
        if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
            let elem_node = arena.get(*element).unwrap();
            assert_eq!(elem_node.kind, NodeKind::TypeName);
            let len_node = arena.get(*length).unwrap();
            assert_eq!(len_node.kind, NodeKind::ExprLiteral);
        } else {
            panic!("expected TypeArray");
        }
    }

    #[test]
    fn parse_nested_array() {
        // `[[u8; 4]; 4]` → LBracket LBracket Ident Semicolon IntLit RBracket Semicolon IntLit RBracket Eof
        let tokens = vec![
            tok(TokenKind::LBracket, 0),
            tok(TokenKind::LBracket, 1),
            tok(TokenKind::Ident, 2), // u8
            tok(TokenKind::Semicolon, 4),
            tok(TokenKind::IntLit, 5), // 4
            tok(TokenKind::RBracket, 6),
            tok(TokenKind::Semicolon, 7),
            tok(TokenKind::IntLit, 8), // 4
            tok(TokenKind::RBracket, 9),
            tok(TokenKind::Eof, 10),
        ];
        let (arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let ty_id = result.unwrap();
        let ty_node = arena.get(ty_id).unwrap();
        assert_eq!(ty_node.kind, NodeKind::TypeArray);
        if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
            // element should be another TypeArray
            let elem_node = arena.get(*element).unwrap();
            assert_eq!(elem_node.kind, NodeKind::TypeArray);
            // length should be a literal
            let len_node = arena.get(*length).unwrap();
            assert_eq!(len_node.kind, NodeKind::ExprLiteral);
        } else {
            panic!("expected TypeArray");
        }
    }

    #[test]
    fn parse_array_p0199_missing_length() {
        // `[u8;]` (missing length) → expect P0199 diagnostic
        let tokens = vec![
            tok(TokenKind::LBracket, 0),
            tok(TokenKind::Ident, 1), // u8
            tok(TokenKind::Semicolon, 3),
            tok(TokenKind::RBracket, 4),
            tok(TokenKind::Eof, 5),
        ];
        let (_arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 1, "expected 1 diagnostic");
        let diag = &diags[0];
        assert_eq!(
            diag.code().number(),
            100,
            "expected P0100 (expected expression)"
        );
        assert!(result.is_err(), "expected parse error");
    }

    #[test]
    fn parse_array_p0199_missing_semicolon() {
        // `[u8 16]` (missing semicolon) → expect P0199 diagnostic
        let tokens = vec![
            tok(TokenKind::LBracket, 0),
            tok(TokenKind::Ident, 1),  // u8
            tok(TokenKind::IntLit, 3), // 16 (no semicolon before this)
            tok(TokenKind::RBracket, 4),
            tok(TokenKind::Eof, 5),
        ];
        let (_arena, result, diags) = parse_t(tokens);

        assert_eq!(diags.len(), 1, "expected 1 diagnostic");
        let diag = &diags[0];
        assert_eq!(diag.code().number(), 199, "expected P0199");
        assert!(result.is_err(), "expected parse error");
    }
}
