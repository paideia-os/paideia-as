//! Outer `#[...]` and inner `#![...]` attribute parsing.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{AttrValue, ItemAttribute, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_attributes(&mut self) -> Result<Vec<ItemAttribute>, ParseError> {
        let mut attributes = vec![];

        while self.at(TokenKind::Hash) {
            self.bump(); // consume `#`

            if !self.at(TokenKind::LBracket) {
                // Recover: skip malformed attribute
                continue;
            }
            self.bump(); // consume `[`

            // Check for `derive`
            if self.at(TokenKind::Ident) {
                let lexeme = if let Some(tok) = self.peek() {
                    self.source_text_for_span(tok.span)
                } else {
                    ""
                };

                if lexeme == "derive" {
                    self.bump(); // consume `derive`

                    // Expect `(`
                    self.expect(TokenKind::LParen)?;

                    // Parse comma-separated list of trait names
                    let mut trait_names = vec![];
                    loop {
                        if self.at(TokenKind::RParen) {
                            break;
                        }

                        // Expect an identifier (trait name)
                        if self.at(TokenKind::Ident) {
                            let trait_tok = self.expect(TokenKind::Ident)?;
                            let trait_id = self.arena_mut().alloc(NodeKind::Ident, trait_tok.span);
                            trait_names.push(trait_id);

                            // Check for comma
                            if self.at(TokenKind::Comma) {
                                self.bump();
                            } else if !self.at(TokenKind::RParen) {
                                // Error: expected comma or )
                                let span = self
                                    .peek()
                                    .map(|t| t.span)
                                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                                let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                                    .expect("valid P0100 code");
                                let diag = Diagnostic::error(code)
                                    .message("expected `,` or `)` in derive attribute")
                                    .with_span(span)
                                    .finish();
                                self.emit_diagnostic(diag);
                                return Err(ParseError);
                            }
                        } else {
                            let span = self
                                .peek()
                                .map(|t| t.span)
                                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                                .expect("valid P0100 code");
                            let diag = Diagnostic::error(code)
                                .message("expected trait name in derive attribute")
                                .with_span(span)
                                .finish();
                            self.emit_diagnostic(diag);
                            return Err(ParseError);
                        }
                    }

                    self.expect(TokenKind::RParen)?; // consume `)`
                    self.expect(TokenKind::RBracket)?; // consume `]`

                    attributes.push(ItemAttribute::Derive { trait_names });
                } else {
                    // Unknown attribute type; skip it
                    // Consume up to the closing bracket
                    let mut bracket_depth = 1;
                    while !self.at_eof() && bracket_depth > 0 {
                        if self.at(TokenKind::LBracket) {
                            bracket_depth += 1;
                        } else if self.at(TokenKind::RBracket) {
                            bracket_depth -= 1;
                        }
                        self.bump();
                    }
                }
            } else {
                // Malformed attribute; skip
                break;
            }
        }

        Ok(attributes)
    }

    /// Parse a struct type declaration: `struct <Ident> <GenericParams>? { field: type, ... }`
    ///
    /// Parses struct field declarations in the form `name: type`, separated by commas.
    /// Trailing commas are allowed.
    /// Attributes (e.g., `#[derive(...)]`) are parsed before the struct keyword.
    pub(super) fn parse_inner_attribute(&mut self) -> Result<ItemAttribute, ParseError> {
        let _hash_span = self.expect(TokenKind::Hash)?.span;
        self.expect(TokenKind::Bang)?;
        self.expect(TokenKind::LBracket)?;

        // Parse attribute name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);
        let name_text = self.source_text_for_span(name_tok.span).to_string();

        self.expect(TokenKind::Assign)?;

        // Parse attribute value: Int, String, or Ident
        let value = if self.at(TokenKind::IntLit) {
            let int_tok = self.expect(TokenKind::IntLit)?;
            let int_text = self.source_text_for_span(int_tok.span);
            let int_val: i64 = int_text.parse().unwrap_or(0); // Default to 0 on parse error

            // Validate bits attribute
            if name_text == "bits" {
                if int_val == 16 {
                    // 16-bit mode is not supported; emit B1700
                    let code = DiagnosticCode::new(Category::B, Severity::Error, 1700)
                        .expect("valid B1700 code");
                    let diag = Diagnostic::error(code)
                        .message("16-bit architecture not supported; use 32 or 64")
                        .with_span(int_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                } else if int_val != 32 && int_val != 64 {
                    // Invalid bits value; emit P0240
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 240)
                        .expect("valid P0240 code");
                    let diag = Diagnostic::error(code)
                        .message("invalid #![bits] value; expected 32 or 64")
                        .with_span(int_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                }
            }

            AttrValue::Int(int_val)
        } else if self.at(TokenKind::StringLit) {
            let str_tok = self.expect(TokenKind::StringLit)?;

            // PA-r16-004-backtrack-a (#1033): Validate target_features attribute.
            // Parse comma-separated feature tokens and emit P0241 for unknown tokens.
            if name_text == "target_features" {
                let str_text = self.source_text_for_span(str_tok.span);
                // Remove surrounding quotes from the string literal.
                let content = if str_text.starts_with('"') && str_text.ends_with('"') {
                    &str_text[1..str_text.len() - 1]
                } else {
                    str_text
                };

                // Split by comma and validate each token
                let mut errors = Vec::new();
                for token in content.split(',') {
                    let trimmed = token.trim();
                    if !trimmed.is_empty() {
                        // Use CpuFeature::from_token to check validity
                        if paideia_as_ir::instruction::CpuFeature::from_token(trimmed).is_none() {
                            errors.push(trimmed.to_string());
                        }
                    }
                }

                // Emit all errors after collecting them
                for error_token in errors {
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 241)
                        .expect("valid P0241 code");
                    let diag = Diagnostic::error(code)
                        .message(format!(
                            "unknown CPU feature token '{}'; supported: cx16, popcnt, bmi1, sse4.2, avx, avx512f",
                            error_token
                        ))
                        .with_span(str_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                }
            }

            let str_id = self.arena_mut().alloc(NodeKind::Placeholder, str_tok.span);
            AttrValue::Str(str_id)
        } else if self.at(TokenKind::Ident) {
            let ident_tok = self.expect(TokenKind::Ident)?;
            let ident_id = self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
            AttrValue::Ident(ident_id)
        } else {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 240).expect("valid P0240 code");
            let diag = Diagnostic::error(code)
                .message("expected integer, string, or identifier for attribute value")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        self.expect(TokenKind::RBracket)?;

        Ok(ItemAttribute::InnerAttr {
            name: name_id,
            value,
        })
    }

}
