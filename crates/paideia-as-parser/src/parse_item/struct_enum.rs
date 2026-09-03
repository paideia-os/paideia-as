//! Struct and enum declaration parsing.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{FieldAttr, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::endian_attr::{parse_endian_attr, validate_endian_field_type};
use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    // Visibility widened from `pub(super)` to `pub(crate)` for
    // paideia-as#1373 (v0.28-M1-004) so `crate::packed_struct` can
    // delegate the body-parse after consuming `@packed_struct[(align=N)]`.
    pub(crate) fn parse_struct_decl(&mut self) -> Result<NodeId, ParseError> {
        // Parse leading attributes
        let attributes = self.parse_attributes()?;

        let struct_tok = self.expect(TokenKind::KwStruct)?;
        let span_start = struct_tok.span;

        // Parse struct name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // Expect opening brace
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();

        // Parse fields: [@endian(be|le)] name : type, [@endian(be|le)] name : type, ...
        //
        // paideia-as#1372 (v0.28-M1-003): an optional `@endian(be|le)`
        // per-field attribute may precede each field name. It is parser-
        // accepted here and stashed on the AST's `struct_field_attrs`
        // side-table (keyed by the field-name NodeId); the elaborator
        // consumes it in a later milestone to insert byte-swaps on load
        // and store. Only integral-scalar field types are accepted;
        // richer types (records, enums, tuples, arrays, pointers, refs)
        // are rejected with P0301.
        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Optional `@endian(be|le)` field-attribute (paideia-as#1372).
            let endian_attr = parse_endian_attr(self)?;

            // Expect field name (Ident)
            let field_name_tok = self.expect(TokenKind::Ident)?;
            let field_name_id = self.arena_mut().alloc(NodeKind::Ident, field_name_tok.span);

            // Expect colon
            if !self.at(TokenKind::Colon) {
                let span = self.peek().map(|t| t.span).unwrap_or(field_name_tok.span);
                let diag = Diagnostic::error(
                    DiagnosticCode::new(Category::P, Severity::Error, 277)
                        .expect("valid P0277 code"),
                )
                .message("malformed struct field: expected ':' after field name")
                .with_span(span)
                .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
            self.bump(); // consume :

            // Parse field type
            let field_type = self.parse_type()?;

            // paideia-as#1372: if `@endian(...)` was seen, validate that
            // the field type is an integral scalar and record on the
            // arena's side-table. Diagnostics are buffered through a
            // callback so the immutable arena/source borrows required by
            // `validate_endian_field_type` do not overlap the mutable
            // borrow needed by `emit_diagnostic`.
            if let Some(endianness) = endian_attr {
                let mut pending: Vec<Diagnostic> = Vec::new();
                let accepted = {
                    let source = self.source();
                    let ast = self.arena();
                    validate_endian_field_type(ast, source, field_type, &mut |d| {
                        pending.push(d)
                    })
                };
                for d in pending {
                    self.emit_diagnostic(d);
                }
                if accepted {
                    self.arena_mut()
                        .struct_field_attrs_mut()
                        .push(field_name_id, FieldAttr::Endian(endianness));
                }
            }

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
            let span = self.peek().map(|t| t.span).unwrap_or(span_start);
            let diag = Diagnostic::error(
                DiagnosticCode::new(Category::P, Severity::Error, 277)
                    .expect("valid P0277 code"),
            )
            .message("malformed struct field: expected '}' to close struct body")
            .with_span(span)
            .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let rbrace_tok = self.bump().unwrap();

        // Compute span
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Struct,
            span,
            ItemData::Struct {
                name: name_id,
                generic_params,
                fields,
                attributes,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse an enum type declaration: `enum <Ident> <GenericParams>? { Variant* }`
    ///
    /// Parses enum name, generic parameters, and variants (unit, tuple, or record).
    /// Attributes (e.g., `#[derive(...)]`) are parsed before the enum keyword.
    pub(super) fn parse_enum_decl(&mut self) -> Result<NodeId, ParseError> {
        // Parse leading attributes
        let attributes = self.parse_attributes()?;

        let enum_tok = self.expect(TokenKind::KwEnum)?;
        let span_start = enum_tok.span;

        // Parse enum name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // Expect opening brace
        if !self.at(TokenKind::LBrace) {
            return self.error_malformed_enum(
                self.peek().map(|t| t.span).unwrap_or(span_start),
                "expected '{' after enum name",
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

                paideia_as_ast::EnumVariant::Tuple {
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

                paideia_as_ast::EnumVariant::Record {
                    name: variant_name_id,
                    fields,
                }
            } else {
                // Unit variant: just Ident
                paideia_as_ast::EnumVariant::Unit {
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
                self.peek().map(|t| t.span).unwrap_or(span_start),
                "expected '}' to close enum",
            );
        }
        let rbrace_tok = self.bump().unwrap();

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
        );

        // Allocate the enum item with parsed variants
        let item = self.arena_mut().alloc_item(
            NodeKind::Enum,
            span,
            ItemData::Enum {
                name: name_id,
                generic_params,
                variants,
                attributes,
                doc: None,
            },
        );
        Ok(item)
    }

}
