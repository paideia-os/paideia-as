//! Memory-layout trailing attribute parsers for let bindings:
//! `@align(N)`, `@ring(slots=N, slot_size=M)`, `@link_section("name")`.
//!
//! Split out of `let_item.rs` (paideia-as#1408, 2026-09-07). Each parser here
//! is called from [`crate::parse_item::let_item::Parser::parse_optional_symbol_attributes`]
//! after the leading `@name` has been consumed; the parsers below own the
//! `( ... )` argument list and its diagnostics only.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
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

    /// Parse `@link_section("name")` where name is a string literal.
    /// Validates: name matches [A-Za-z0-9._\-], length 1..=32, non-empty.
    pub(super) fn parse_link_section_attr(&mut self) -> Result<String, ParseError> {
        // Expect `(`
        if !self.eat(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 282)
                .expect("valid P0282 code");
            let diag = Diagnostic::error(code)
                .message("malformed @link_section(\"name\") syntax: expected '(' after 'link_section'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Expect a string literal
        let str_tok = self.expect(TokenKind::StringLit)?;
        let str_span = str_tok.span;
        let str_text = self.source_text_for_span(str_span);

        // Extract string content (without quotes)
        let name = if str_text.starts_with('"') && str_text.ends_with('"') && str_text.len() >= 2 {
            str_text[1..str_text.len() - 1].to_string()
        } else {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 282)
                .expect("valid P0282 code");
            let diag = Diagnostic::error(code)
                .message("@link_section name must be a valid string literal")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        // Validate: non-empty
        if name.is_empty() {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 282)
                .expect("valid P0282 code");
            let diag = Diagnostic::error(code)
                .message("@link_section name must be non-empty")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Validate: length <= 32
        if name.len() > 32 {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 282)
                .expect("valid P0282 code");
            let diag = Diagnostic::error(code)
                .message(format!("@link_section name must be <= 32 characters, got {}", name.len()))
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Validate: only [A-Za-z0-9._\-]
        for c in name.chars() {
            if !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-' {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 282)
                    .expect("valid P0282 code");
                let diag = Diagnostic::error(code)
                    .message(format!("@link_section name must contain only alphanumeric, '.', '_', and '-', got invalid char '{}'", c))
                    .with_span(str_span)
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
            let code = DiagnosticCode::new(Category::P, Severity::Error, 282)
                .expect("valid P0282 code");
            let diag = Diagnostic::error(code)
                .message("malformed @link_section(\"name\") syntax: expected ')' after name")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(name)
    }
}
