//! Let-declaration parsing plus `@align` / `@ring` symbol attributes.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_optional_symbol_attributes(&mut self) -> Result<(Option<u32>, Option<(u32, u32)>), ParseError> {
        if !self.at(TokenKind::At) {
            return Ok((None, None));
        }

        self.bump(); // consume `@`

        // Expect an identifier for the attribute name
        let attr_name_tok = self.expect(TokenKind::Ident)?;
        let attr_name = self.source_text_for_span(attr_name_tok.span);

        match attr_name {
            "align" => {
                let align = self.parse_align_attr()?;
                Ok((Some(align), None))
            }
            "ring" => {
                let ring = self.parse_ring_attr(attr_name_tok.span)?;
                Ok((None, Some(ring)))
            }
            _ => {
                // P0250: unknown symbol attribute
                let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                    .expect("valid P0250 code");
                let diag = Diagnostic::error(code)
                    .message(format!("unknown symbol attribute '@{}' (only 'align' and 'ring' supported)", attr_name))
                    .with_span(attr_name_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
        }
    }

    /// Parse `@align(N)` where N is a power-of-two integer literal.
    pub(super) fn parse_align_attr(&mut self) -> Result<u32, ParseError> {
        // Expect `(`
        if !self.eat(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 251)
                .expect("valid P0251 code");
            let diag = Diagnostic::error(code)
                .message("malformed @align(N) syntax: expected '(' after 'align'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Parse the integer literal
        let lit_tok = self.expect(TokenKind::IntLit)?;
        let lit_text = self.source_text_for_span(lit_tok.span);

        let value: u32 = lit_text.parse().map_err(|_| {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 252)
                .expect("valid P0252 code");
            let diag = Diagnostic::error(code)
                .message("@align value must be a valid integer in range [1, 2^30]")
                .with_span(lit_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            ParseError
        })?;

        // Validate: power of two and in range [1, 2^30]
        if value == 0 || value > (1u32 << 30) || (value & (value - 1)) != 0 {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 252)
                .expect("valid P0252 code");
            let diag = Diagnostic::error(code)
                .message(format!("@align value must be a power of two in range [1, 2^30], got {}", value))
                .with_span(lit_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Expect `)`
        if !self.eat(TokenKind::RParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 251)
                .expect("valid P0251 code");
            let diag = Diagnostic::error(code)
                .message("malformed @align(N) syntax: expected ')' after value")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(value)
    }

    /// Parse `@ring(slots=N, slot_size=M)` (called after @ and 'ring' are already consumed).
    pub(super) fn parse_ring_attr(&mut self, attr_name_span: Span) -> Result<(u32, u32), ParseError> {
        // Expect `(`
        if !self.eat(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                .expect("valid P0253 code");
            let diag = Diagnostic::error(code)
                .message("malformed @ring(...) syntax: expected '(' after 'ring'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let mut slots = None;
        let mut slot_size = None;

        // Parse comma-separated key=value pairs
        loop {
            // Parse key (identifier)
            let key_tok = self.expect(TokenKind::Ident)?;
            let key = self.source_text_for_span(key_tok.span).to_string();

            // Expect `=`
            if !self.eat(TokenKind::Assign) {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                    .expect("valid P0253 code");
                let diag = Diagnostic::error(code)
                    .message("malformed @ring(...) syntax: expected '=' after key")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }

            // Parse value (integer literal)
            let val_tok = self.expect(TokenKind::IntLit)?;
            let val_text = self.source_text_for_span(val_tok.span).to_string();

            let value: u32 = val_text.parse().map_err(|_| {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                    .expect("valid P0253 code");
                let diag = Diagnostic::error(code)
                    .message("@ring value must be a valid integer")
                    .with_span(val_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                ParseError
            })?;

            // Store key=value
            match key.as_str() {
                "slots" => slots = Some(value),
                "slot_size" => slot_size = Some(value),
                _ => {
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                        .expect("valid P0253 code");
                    let diag = Diagnostic::error(code)
                        .message(format!("unknown @ring key '{}' (expected 'slots' or 'slot_size')", key))
                        .with_span(key_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }

            // Check for comma or closing paren
            if self.eat(TokenKind::Comma) {
                // Continue to next key=value pair
                continue;
            } else if self.at(TokenKind::RParen) {
                break;
            } else {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                    .expect("valid P0253 code");
                let diag = Diagnostic::error(code)
                    .message("malformed @ring(...) syntax: expected ',' or ')'")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        }

        // Expect `)`
        if !self.eat(TokenKind::RParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                .expect("valid P0253 code");
            let diag = Diagnostic::error(code)
                .message("malformed @ring(...) syntax: expected ')' after values")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Validate that both keys are present
        let slots_val = match slots {
            Some(s) => s,
            None => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                    .expect("valid P0253 code");
                let diag = Diagnostic::error(code)
                    .message("malformed @ring(...) syntax: missing 'slots' parameter")
                    .with_span(attr_name_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        let slot_size_val = match slot_size {
            Some(ss) => ss,
            None => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 253)
                    .expect("valid P0253 code");
                let diag = Diagnostic::error(code)
                    .message("malformed @ring(...) syntax: missing 'slot_size' parameter")
                    .with_span(attr_name_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Validate slots: must be power of two and > 0
        if slots_val == 0 || (slots_val & (slots_val - 1)) != 0 {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 260)
                .expect("valid P0260 code");
            let diag = Diagnostic::error(code)
                .message(format!("@ring slots must be a power of two and > 0, got {}", slots_val))
                .with_span(attr_name_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Validate slot_size: must be > 0
        if slot_size_val == 0 {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 261)
                .expect("valid P0261 code");
            let diag = Diagnostic::error(code)
                .message("@ring slot_size must be > 0")
                .with_span(attr_name_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Check for overflow: slots * slot_size should fit in u64
        let _total_size = (slots_val as u64).checked_mul(slot_size_val as u64);
        if _total_size.is_none() {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 261)
                .expect("valid P0261 code");
            let diag = Diagnostic::error(code)
                .message("@ring total size (slots * slot_size) overflows")
                .with_span(attr_name_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok((slots_val, slot_size_val))
    }

    /// Parse a top-level let declaration with optional visibility: `[pub] let [mut] <Ident> <GenericParams>? (: Type)? = Expr @align(N)? @ring(...)?`
    pub(super) fn parse_let_decl_with_visibility(&mut self, public: bool) -> Result<NodeId, ParseError> {
        let let_tok = self.expect(TokenKind::KwLet)?;
        let span_start = let_tok.span;

        // `pub` is consumed by the caller (parse_item dispatcher) and passed in.
        // Do NOT re-check for KwPub here.

        // Check for optional `mut` keyword
        let mutable = if self.at(TokenKind::KwMut) {
            self.bump();
            true
        } else {
            false
        };

        // Try to parse a pattern first (could be a tuple, struct, enum variant, etc.)
        // If that fails, fall back to parsing a simple identifier.
        // Peek ahead to see if we have a pattern or just a name.
        let mut pattern_or_name = None;

        // Check if the next token looks like a pattern start
        if let Some(tok) = self.peek() {
            match tok.kind {
                // These are pattern starters
                TokenKind::LParen => {
                    // This is a pattern
                    pattern_or_name = Some(self.parse_pattern()?);
                }
                TokenKind::Ident => {
                    // Could be a pattern or just an identifier name.
                    // Peek at the next token to disambiguate.
                    if let Some(next_tok) = self.peek_at(1) {
                        match next_tok.kind {
                            // These indicate a pattern
                            TokenKind::ColonColon | TokenKind::LBrace => {
                                pattern_or_name = Some(self.parse_pattern()?);
                            }
                            // Otherwise just an identifier
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // If we didn't parse a pattern, just parse a simple identifier
        let name_id = if let Some(pat) = pattern_or_name {
            pat
        } else {
            let name_tok = self.expect(TokenKind::Ident)?;
            self.arena_mut().alloc(NodeKind::Ident, name_tok.span)
        };

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // Optional type annotation
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect `=`
        self.expect(TokenKind::Assign)?;

        // Parse value expression
        let value = self.parse_expr()?;

        // Parse optional symbol attributes (@align or @ring)
        let (align, ring) = self.parse_optional_symbol_attributes()?;

        // Consume optional `;`
        self.eat(TokenKind::Semicolon);

        // Compute span
        let value_span = self
            .arena()
            .get(value)
            .map(|n| n.span)
            .unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            value_span.byte_start() + value_span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Let,
            span,
            ItemData::Let {
                public,
                mutable,
                name: name_id,
                generic_params,
                ty,
                value,
                align,
                ring,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Wrapper for backward compatibility and simplicity when public visibility is not needed.
    pub(super) fn parse_let_decl(&mut self) -> Result<NodeId, ParseError> {
        self.parse_let_decl_with_visibility(false)
    }

}
