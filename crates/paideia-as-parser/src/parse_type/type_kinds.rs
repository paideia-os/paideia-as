//! Compound type kinds: `record`, `enum`, `[T; N]`.
//! Split out of `parse_type.rs` (2026-07-08).

use paideia_as_ast::{EnumVariant, NodeKind, TypeData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_type_record(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
    pub(super) fn parse_type_enum(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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
    pub(super) fn parse_type_array(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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

}
