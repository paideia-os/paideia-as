//! Generic parameter parsing (`<T, N: Copy>`) and type-name path parsing.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{GenericParam, NodeId, NodeKind, TraitBound};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(crate) fn parse_generic_params(&mut self) -> Result<Vec<GenericParam>, ParseError> {
        // Expect opening `<`
        let _lt_tok = self.expect(TokenKind::Lt)?;
        let mut params = Vec::new();

        // Loop: parse comma-separated generic parameters
        loop {
            // Check for closing `>`
            if self.at(TokenKind::Gt) {
                break;
            }

            // Check for lifetime parameter (leading `'`)
            // Lifetimes appear as identifier tokens with lexeme starting with `'`
            let is_lifetime = if let Some(tok) = self.peek() {
                if tok.kind == TokenKind::Ident {
                    let lexeme = self.source_text_for_span(tok.span);
                    lexeme.starts_with('\'')
                } else {
                    false
                }
            } else {
                false
            };

            if is_lifetime {
                // This is a lifetime parameter
                if let Some(tok) = self.peek() {
                    let lexeme = self.source_text_for_span(tok.span).to_string();
                    self.bump(); // consume the lifetime token

                    // Extract the lifetime name (remove leading `'`)
                    let lifetime_name = if lexeme.len() > 1 {
                        lexeme[1..].to_string()
                    } else {
                        // Malformed lifetime (just `'`), skip for now
                        "".to_string()
                    };

                    params.push(GenericParam::Lifetime {
                        name: lifetime_name,
                    });

                    // Check for separator or end
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }

                    // Allow trailing comma before `>`
                    if self.at(TokenKind::Gt) {
                        break;
                    }

                    continue;
                }
            }

            // Parse type parameter name (Ident)
            let param_name_tok = match self.peek() {
                Some(tok) if tok.kind == TokenKind::Ident => {
                    self.bump();
                    tok
                }
                _ => {
                    // Missing or malformed parameter name
                    let span = self
                        .peek()
                        .map(|t| t.span)
                        .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 200)
                        .expect("valid P0200 code");
                    let diag = Diagnostic::error(code)
                        .message("expected generic parameter name in generic parameter list")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            };

            let param_name = self.arena_mut().alloc(NodeKind::Ident, param_name_tok.span);

            // Parse optional bounds: `:` followed by comma-separated trait names,
            // each optionally carrying a `<Name = Type, ...>` associated-type
            // projection list (and/or regular type args).
            let mut bounds: Vec<TraitBound> = Vec::new();
            if self.eat(TokenKind::Colon) {
                loop {
                    // Parse trait name as a path
                    let trait_name = self.parse_type_name_path()?;
                    let projections = self.parse_trait_bound_projections()?;
                    bounds.push(TraitBound {
                        path: trait_name,
                        projections,
                    });

                    // Check for comma (more bounds) or end of bounds
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }

                    // Check if the next token is `>` or `,` (trailing comma)
                    if self.at(TokenKind::Gt) {
                        break;
                    }
                }
            }

            params.push(GenericParam::Type {
                name: param_name,
                bounds,
            });

            // Check for separator or end
            if !self.eat(TokenKind::Comma) {
                break;
            }

            // Allow trailing comma before `>`
            if self.at(TokenKind::Gt) {
                break;
            }
        }

        // Expect closing `>`
        if !self.eat(TokenKind::Gt) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 200).expect("valid P0200 code");
            let diag = Diagnostic::error(code)
                .message("expected '>' to close generic parameter list")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(params)
    }

    /// Parse the optional bound-position `<...>` after a trait path, extracting
    /// associated-type projections as `(name_ident, ty)` pairs.
    ///
    /// Accepts both projections (`Name = Type`) and regular type arguments
    /// interleaved in the same list, comma-separated. Regular type arguments
    /// are parsed for well-formedness and then dropped — the elaborator
    /// reconstructs the trait's generic instantiation from context.
    ///
    /// Validation of `Name` against the referenced trait's associated-type
    /// set is a resolver concern (needs trait-registry access) and is not
    /// performed here (paideia-as#1496, PAS-DEBT-B2-002).
    pub(super) fn parse_trait_bound_projections(
        &mut self,
    ) -> Result<Vec<(NodeId, NodeId)>, ParseError> {
        let mut projections: Vec<(NodeId, NodeId)> = Vec::new();
        if !self.at(TokenKind::Lt) {
            return Ok(projections);
        }
        self.bump(); // consume `<`

        loop {
            if self.at(TokenKind::Gt) || self.at_eof() {
                break;
            }

            // Lookahead: `Ident =` marks a projection; anything else is a
            // regular generic type argument. `=` is TokenKind::Assign;
            // TokenKind::Eq is `==` (pre-fix, the parser tested `Eq` here
            // and so never entered the extraction branch — the actual
            // manifestation of PAS-DEBT-B2-002).
            let is_projection = matches!(self.peek().map(|t| t.kind), Some(TokenKind::Ident))
                && matches!(self.peek_at(1).map(|t| t.kind), Some(TokenKind::Assign));

            if is_projection {
                let name_tok = self.bump().expect("Ident guaranteed by peek");
                let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);
                self.bump(); // consume `=`
                let ty = self.parse_type()?;
                projections.push((name_id, ty));
            } else {
                // Regular type arg; parse for well-formedness and drop.
                let _ = self.parse_type()?;
            }

            if !self.eat(TokenKind::Comma) {
                break;
            }
            if self.at(TokenKind::Gt) {
                break;
            }
        }

        // Consume the closing `>` of the bound-position list.
        if self.at(TokenKind::Gt) {
            self.bump();
        }
        Ok(projections)
    }

    /// Parse a type name as a path for use in generic bounds.
    ///
    /// For phase-1 (m9-001), this parses a simple identifier or qualified path
    /// like `Trait` or `Module::Trait`.
    pub(super) fn parse_type_name_path(&mut self) -> Result<NodeId, ParseError> {
        if let Some(tok) = self.peek() {
            if tok.kind == TokenKind::Ident {
                self.bump();
                let id = self.arena_mut().alloc(NodeKind::Ident, tok.span);

                // Handle qualified paths: `Ident :: Ident`
                let mut segments = vec![id];
                while self.eat(TokenKind::ColonColon) {
                    if let Some(next_tok) = self.peek() {
                        if next_tok.kind == TokenKind::Ident {
                            self.bump();
                            let segment = self.arena_mut().alloc(NodeKind::Ident, next_tok.span);
                            segments.push(segment);
                        } else {
                            // Error: expected Ident after `::`
                            let span = next_tok.span;
                            let code = DiagnosticCode::new(Category::P, Severity::Error, 200)
                                .expect("valid P0200 code");
                            let diag = Diagnostic::error(code)
                                .message("expected identifier after '::' in trait bound path")
                                .with_span(span)
                                .finish();
                            self.emit_diagnostic(diag);
                            return Err(ParseError);
                        }
                    } else {
                        break;
                    }
                }

                // Allocate an ExprPath to represent the trait name
                // Use the first segment's span as the start
                let span_start = self
                    .arena()
                    .get(segments[0])
                    .map(|n| n.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let span_end = self
                    .arena()
                    .get(segments[segments.len() - 1])
                    .map(|n| n.span)
                    .unwrap_or(span_start);
                let span = Span::new(
                    span_start.file(),
                    span_start.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
                );

                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprPath,
                    span,
                    paideia_as_ast::ExprData::Path { segments },
                ))
            } else {
                let span = tok.span;
                let code = DiagnosticCode::new(Category::P, Severity::Error, 200)
                    .expect("valid P0200 code");
                let diag = Diagnostic::error(code)
                    .message("expected trait name in generic parameter bound")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
        } else {
            let span = Span::new(self.file(), 0, 0);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 200).expect("valid P0200 code");
            let diag = Diagnostic::error(code)
                .message("expected trait name in generic parameter bound")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            Err(ParseError)
        }
    }

}
