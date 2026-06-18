//! Unsafe block parsing.
//!
//! Implements §8 UnsafeExpr grammar: `unsafe { effects: ..., capabilities: ..., justification: ..., block: ... }`.
//!
//! The unsafe block must contain four mandatory fields in any order:
//! - `effects: { Ident, Ident, ... }`
//! - `capabilities: { Ident, Ident, ... }`
//! - `justification: "string literal"`
//! - `block: { Stmt+ }`
//!
//! Phase-1 implementation: Fields must appear in the order declared in §8.
//! If any field is missing when the closing `}` is encountered, emit exactly
//! one U1600 diagnostic listing all missing fields, spanning the closing brace.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse an unsafe block: `unsafe { effects: {...}, capabilities: {...}, justification: "...", block: {...} }`.
    ///
    /// **Algorithm:**
    /// 1. Expect `KwUnsafe`.
    /// 2. Expect `LBrace`.
    /// 3. Parse four mandatory fields in the order: effects, capabilities, justification, block.
    ///    - Each field is identified by an Ident followed by `:`.
    ///    - Ident names are NOT validated at parse time (to avoid needing source text lookup).
    ///    - Expected order: effects, capabilities, justification, block.
    ///    - If a field body doesn't match the expected shape (e.g., `effects` should be `{...}`),
    ///      emit U1600 and return Err.
    /// 4. Track which fields have been seen.
    /// 5. If the closing `}` is reached before all 4 fields, emit exactly one U1600
    ///    diagnostic listing the missing fields and spanning the closing brace.
    /// 6. Allocate `ExprData::Unsafe { effects, capabilities, justification, block }`.
    ///
    /// Returns the `NodeId` of the allocated expression on success.
    pub(crate) fn parse_unsafe(&mut self) -> Result<NodeId, ParseError> {
        let unsafe_tok = self.expect(TokenKind::KwUnsafe)?;
        let span_start = unsafe_tok.span;

        // Expect opening brace
        self.expect(TokenKind::LBrace)?;

        let mut effects_opt = None;
        let mut capabilities_opt = None;
        let mut justification_opt = None;
        let mut block_opt = None;

        // Track which fields we've seen for error reporting
        let mut fields_seen = [false; 4]; // effects, capabilities, justification, block

        // Parse fields until closing brace
        loop {
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Expect an identifier for the field name
            if !self.at(TokenKind::Ident) {
                let code =
                    DiagnosticCode::new(Category::P, Severity::Error, 151).expect("valid P code");
                let diag = Diagnostic::error(code)
                    .message("expected field name (effects, capabilities, justification, or block)")
                    .with_span(self.peek().map(|t| t.span).unwrap_or(span_start))
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }

            let field_name_tok = self.bump().unwrap();
            let field_name_start = field_name_tok.span;

            // Expect colon
            if !self.eat(TokenKind::Colon) {
                let code =
                    DiagnosticCode::new(Category::P, Severity::Error, 152).expect("valid P code");
                let diag = Diagnostic::error(code)
                    .message("expected `:` after field name")
                    .with_span(self.peek().map(|t| t.span).unwrap_or(field_name_start))
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }

            // Determine which field this is by position and parse its body
            // We use field order: effects, capabilities, justification, block
            let field_index = if !fields_seen[0] {
                0 // Next field should be effects
            } else if !fields_seen[1] {
                1 // Next field should be capabilities
            } else if !fields_seen[2] {
                2 // Next field should be justification
            } else if !fields_seen[3] {
                3 // Next field should be block
            } else {
                // All fields already seen
                let code =
                    DiagnosticCode::new(Category::P, Severity::Error, 153).expect("valid P code");
                let diag = Diagnostic::error(code)
                    .message("too many fields in unsafe block (expected 4)")
                    .with_span(field_name_start)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            };

            match field_index {
                0 => {
                    // Parse effects: { Ident, Ident, ... }
                    if !self.at(TokenKind::EffectOpen) && !self.at(TokenKind::LBrace) {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 154)
                            .expect("valid P code");
                        let diag = Diagnostic::error(code)
                            .message("expected `!{` or `{` for effects field")
                            .with_span(self.peek().map(|t| t.span).unwrap_or(field_name_start))
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }

                    if self.at(TokenKind::EffectOpen) {
                        effects_opt = Some(self.parse_effect_row()?);
                    } else {
                        // Accept bare { } as well
                        self.expect(TokenKind::LBrace)?;
                        let mut items = Vec::new();
                        while !self.at(TokenKind::RBrace) {
                            if self.at(TokenKind::Ident) {
                                let ident_tok = self.bump().unwrap();
                                let ident_id =
                                    self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
                                items.push(ident_id);

                                if !self.at(TokenKind::Comma) {
                                    break;
                                }
                                self.bump();
                            } else {
                                break;
                            }
                        }
                        let rbrace_tok = self.expect(TokenKind::RBrace)?;
                        let span = Span::new(
                            field_name_start.file(),
                            field_name_start.byte_start(),
                            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len()
                                - field_name_start.byte_start(),
                        );
                        effects_opt = Some(self.arena_mut().alloc_type(
                            NodeKind::TypeEffectRow,
                            span,
                            paideia_as_ast::TypeData::EffectRow { items, rest: None },
                        ));
                    }
                    fields_seen[0] = true;
                }
                1 => {
                    // Parse capabilities: { Ident, Ident, ... }
                    if !self.at(TokenKind::CapOpen) && !self.at(TokenKind::LBrace) {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 154)
                            .expect("valid P code");
                        let diag = Diagnostic::error(code)
                            .message("expected `@{` or `{` for capabilities field")
                            .with_span(self.peek().map(|t| t.span).unwrap_or(field_name_start))
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }

                    if self.at(TokenKind::CapOpen) {
                        capabilities_opt = Some(self.parse_cap_set()?);
                    } else {
                        // Accept bare { } as well
                        self.expect(TokenKind::LBrace)?;
                        let mut items = Vec::new();
                        while !self.at(TokenKind::RBrace) {
                            if self.at(TokenKind::Ident) {
                                let ident_tok = self.bump().unwrap();
                                let ident_id =
                                    self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
                                items.push(ident_id);

                                if !self.at(TokenKind::Comma) {
                                    break;
                                }
                                self.bump();
                            } else {
                                break;
                            }
                        }
                        let rbrace_tok = self.expect(TokenKind::RBrace)?;
                        let span = Span::new(
                            field_name_start.file(),
                            field_name_start.byte_start(),
                            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len()
                                - field_name_start.byte_start(),
                        );
                        capabilities_opt = Some(self.arena_mut().alloc_type(
                            NodeKind::TypeEffectRow,
                            span,
                            paideia_as_ast::TypeData::EffectRow { items, rest: None },
                        ));
                    }
                    fields_seen[1] = true;
                }
                2 => {
                    // Parse justification: StringLit
                    if !self.at(TokenKind::StringLit) {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 155)
                            .expect("valid P code");
                        let diag = Diagnostic::error(code)
                            .message("expected string literal for justification field")
                            .with_span(self.peek().map(|t| t.span).unwrap_or(field_name_start))
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }

                    let lit_tok = self.bump().unwrap();
                    let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, lit_tok.span);
                    justification_opt = Some(self.arena_mut().alloc_expr(
                        NodeKind::ExprLiteral,
                        lit_tok.span,
                        ExprData::Literal { lit: lit_id },
                    ));
                    fields_seen[2] = true;
                }
                3 => {
                    // Parse block: { Stmt+ }
                    self.expect(TokenKind::LBrace)?;

                    let mut block_body = Vec::new();

                    loop {
                        if self.at(TokenKind::RBrace) {
                            break;
                        }

                        let stmt = self.parse_stmt()?;
                        block_body.push(stmt);
                    }

                    // At least one statement is required
                    if block_body.is_empty() {
                        let rbrace_tok = self.peek().ok_or(ParseError)?;
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 156)
                            .expect("valid P code");
                        let diag = Diagnostic::error(code)
                            .message("unsafe block must contain at least one statement")
                            .with_span(rbrace_tok.span)
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }

                    self.expect(TokenKind::RBrace)?;
                    block_opt = Some(block_body);
                    fields_seen[3] = true;
                }
                _ => unreachable!(),
            }

            // Check for comma separator (optional between fields)
            self.eat(TokenKind::Comma);
        }

        // Check for missing fields
        let missing_fields: Vec<&str> = vec![
            ("effects", fields_seen[0]),
            ("capabilities", fields_seen[1]),
            ("justification", fields_seen[2]),
            ("block", fields_seen[3]),
        ]
        .into_iter()
        .filter(|(_, seen)| !seen)
        .map(|(name, _)| name)
        .collect();

        if !missing_fields.is_empty() {
            let rbrace_tok = self.expect(TokenKind::RBrace)?;
            let missing_str = missing_fields.join(", ");
            // Use U1600 diagnostic code (unsafe-field violation)
            let code =
                DiagnosticCode::new(Category::U, Severity::Error, 1600).expect("valid U code");
            let diag = Diagnostic::error(code)
                .message(format!(
                    "unsafe block missing required fields: {}",
                    missing_str
                ))
                .with_span(rbrace_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Expect closing brace
        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span_end = rbrace_tok.span;

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        // Extract effects from the parsed row (if present)
        // For phase-1, we store the effect row node as a single-element vector
        // since ExprData::Unsafe expects Vec<NodeId> for effects and capabilities.
        // A future refinement could store the items directly.
        let effects_vec = if let Some(eff_node) = effects_opt {
            vec![eff_node]
        } else {
            vec![]
        };

        let capabilities_vec = if let Some(cap_node) = capabilities_opt {
            vec![cap_node]
        } else {
            vec![]
        };

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprUnsafe,
            span,
            ExprData::Unsafe {
                effects: effects_vec,
                capabilities: capabilities_vec,
                justification: justification_opt.ok_or(ParseError)?,
                block: block_opt.ok_or(ParseError)?,
            },
        ))
    }
}

// Tests will be in integration tests; parse_unsafe is internal to the parser module.
