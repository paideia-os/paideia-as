//! Let-declaration parsing plus `@align` / `@ring` / `@link_section` / `@abi` symbol attributes.
//! Split out of `parse_item.rs` (2026-07-08).

use std::collections::HashSet;

use paideia_as_ast::{CallingConvention, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

/// Structured container for trailing symbol attributes on Let bindings.
/// Refactored in PA19-r19-001 to eliminate growing tuple.
pub(super) struct LetSymbolAttrs {
    pub align: Option<u32>,
    pub ring: Option<(u32, u32)>,
    pub link_section: Option<String>,
    pub abi: Option<CallingConvention>,
    /// `@no_frame` opt-out for the default SysV frame prologue/epilogue
    /// (paideia-as#1276, unblocks paideia-os#716). Bare flag — no arguments.
    ///
    /// Phase-1 landing (parser + AST/IR plumbing): the flag is captured here and
    /// propagated to [`paideia_as_ast::ItemData::Let::no_frame`] / IR `LetInfo::no_frame`,
    /// but the elaborator emit pass still ignores it. Non-function placement is not
    /// yet diagnosed (deferred to the phase that wires the emit change).
    pub no_frame: bool,
}

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_optional_symbol_attributes(&mut self) -> Result<LetSymbolAttrs, ParseError> {
        let mut align: Option<u32> = None;
        let mut ring: Option<(u32, u32)> = None;
        let mut link_section: Option<String> = None;
        let mut abi: Option<CallingConvention> = None;
        let mut no_frame: bool = false;
        let mut seen_attrs = HashSet::new();

        // Loop to accept attributes in any order
        loop {
            if !self.at(TokenKind::At) {
                break;
            }

            self.bump(); // consume `@`

            // Expect an identifier for the attribute name
            let attr_name_tok = self.expect(TokenKind::Ident)?;
            let attr_name = self.source_text_for_span(attr_name_tok.span);

            // Check for duplicate
            if seen_attrs.contains(attr_name) {
                match attr_name {
                    "link_section" => {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 283)
                            .expect("valid P0283 code");
                        let diag = Diagnostic::error(code)
                            .message("duplicate @link_section directive")
                            .with_span(attr_name_tok.span)
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }
                    _ => {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                            .expect("valid P0250 code");
                        let diag = Diagnostic::error(code)
                            .message(format!("duplicate @{} attribute", attr_name))
                            .with_span(attr_name_tok.span)
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }
                }
            }
            seen_attrs.insert(attr_name.to_string());

            match attr_name {
                "align" => {
                    align = Some(self.parse_align_attr()?);
                }
                "ring" => {
                    ring = Some(self.parse_ring_attr(attr_name_tok.span)?);
                }
                "link_section" => {
                    link_section = Some(self.parse_link_section_attr()?);
                }
                "abi" => {
                    abi = Some(self.parse_abi_attr()?);
                }
                "no_frame" => {
                    // paideia-as#1276 (phase 1): bare flag, no arguments.
                    // Recognized here so the elaborator can consult it at emit time
                    // in a later phase. Placement validation (must-be-function) lands
                    // with the emit change; phase 1 keeps the attribute inert.
                    no_frame = true;
                }
                _ => {
                    // P0250: unknown symbol attribute
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                        .expect("valid P0250 code");
                    let diag = Diagnostic::error(code)
                        .message(format!("unknown symbol attribute '@{}' (only 'align', 'ring', 'link_section', 'abi', and 'no_frame' supported)", attr_name))
                        .with_span(attr_name_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        }

        Ok(LetSymbolAttrs { align, ring, link_section, abi, no_frame })
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

    /// Parse `@abi("ms"|"sysv")` where the argument is a calling convention string.
    /// Validates: only "ms" or "sysv" (lowercase, case-sensitive).
    /// Emits P0285 for invalid strings, non-strings, empty strings, or missing parens.
    pub(super) fn parse_abi_attr(&mut self) -> Result<CallingConvention, ParseError> {
        // Expect `(`
        if !self.eat(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("malformed @abi(...) syntax: expected '(' after 'abi'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Check for string literal; emit P0285 if it's not a string
        if !self.at(TokenKind::StringLit) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("@abi value must be a string literal, expected \"ms\" or \"sysv\"")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            // Skip the invalid token
            self.bump();
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let str_span = str_tok.span;
        let str_text = self.source_text_for_span(str_span);

        // Extract string content (without quotes)
        let value = if str_text.starts_with('"') && str_text.ends_with('"') && str_text.len() >= 2 {
            str_text[1..str_text.len() - 1].to_string()
        } else {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("@abi value must be a valid string literal")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        // Validate: non-empty
        if value.is_empty() {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("@abi value must be non-empty; expected \"ms\" or \"sysv\"")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Validate: only "ms" or "sysv" (lowercase, case-sensitive)
        let cc = match value.as_str() {
            "ms" => CallingConvention::Ms,
            "sysv" => CallingConvention::Sysv,
            _ => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                    .expect("valid P0285 code");
                let diag = Diagnostic::error(code)
                    .message(format!("invalid @abi value \"{}\"; expected \"ms\" or \"sysv\"", value))
                    .with_span(str_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Expect `)`
        if !self.eat(TokenKind::RParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("malformed @abi(...) syntax: expected ')' after value")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(cc)
    }

    /// Parse a top-level let declaration with optional visibility: `[pub] let [mut] <Ident> <GenericParams>? (: Type)? = Expr @align(N)? @ring(...)? @link_section("name")? @abi("ms"|"sysv")?`
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

        // Parse optional symbol attributes (@align, @ring, @link_section, @abi, or @no_frame)
        let LetSymbolAttrs { align, ring, link_section, abi, no_frame } = self.parse_optional_symbol_attributes()?;

        // paideia-as#1276 phase 3: `@no_frame` is a function-only attribute — it
        // toggles the SysV frame-pointer prologue/epilogue that the elaborator
        // emits around `fn (...)` lambdas. Applying it to a non-function
        // binding (`pub let CONST : u64 = 42 @no_frame`) is a category error:
        // there is no lambda / no ret / no prologue to suppress. Reject at
        // parse time via P0250 so downstream phases can trust that any
        // `LetInfo::no_frame == true` corresponds to a Lambda RHS.
        if no_frame {
            let value_kind = self
                .arena()
                .get(value)
                .map(|n| n.kind);
            if value_kind != Some(NodeKind::ExprLambda) {
                let value_span = self
                    .arena()
                    .get(value)
                    .map(|n| n.span)
                    .unwrap_or(span_start);
                let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                    .expect("valid P0250 code");
                let diag = Diagnostic::error(code)
                    .message(
                        "@no_frame is a function-only attribute; it toggles the SysV frame-pointer prologue/epilogue and cannot be applied to a non-function binding"
                    )
                    .with_span(value_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        }

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
                link_section,
                abi,
                no_frame,
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

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, SourceMap, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};
    use std::path::PathBuf;

    /// Helper: parse source code and return (arena, parse result, diagnostics)
    fn parse_and_check(
        source: &str,
    ) -> (
        paideia_as_ast::AstArena,
        Result<paideia_as_ast::NodeId, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut source_map = SourceMap::new();
        let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
        let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
        let mut arena = paideia_as_ast::AstArena::new();
        let mut sink = VecSink::new();
        let mut lex = Lexer::new(file, &source_text);
        let mut collector = VecSink::new();
        let tokens = lex.collect_tokens(&mut collector);
        // Forward lexer diagnostics into the main sink.
        for d in collector.into_diagnostics() {
            let _ = sink.emit(d);
        }
        let result = {
            let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
            p.parse_source_file()
        };
        (arena, result, sink.into_diagnostics())
    }

    /// Assert that at least one diagnostic with the given P<number> code is present.
    fn assert_has_p_code(diags: &[paideia_as_diagnostics::Diagnostic], number: u16) {
        let matches: Vec<_> = diags
            .iter()
            .filter(|d| d.code().category().letter() == 'P' && d.code().number() == number)
            .collect();
        assert!(
            !matches.is_empty(),
            "expected at least one P{:04} diagnostic, got: {:?}",
            number,
            diags
                .iter()
                .map(|d| format!("{}{:04}", d.code().category().letter(), d.code().number()))
                .collect::<Vec<_>>()
        );
    }

    /// Assert that a diagnostic with the given P<number> code contains a substring.
    fn assert_p_code_message_contains(
        diags: &[paideia_as_diagnostics::Diagnostic],
        number: u16,
        substring: &str,
    ) {
        let matches: Vec<_> = diags
            .iter()
            .filter(|d| d.code().category().letter() == 'P' && d.code().number() == number)
            .collect();
        assert!(
            !matches.is_empty(),
            "expected at least one P{:04} diagnostic",
            number
        );
        let found = matches.iter().any(|d| d.message().contains(substring));
        assert!(
            found,
            "P{:04} diagnostic should contain '{}', got messages: {:?}",
            number,
            substring,
            matches.iter().map(|d| d.message()).collect::<Vec<_>>()
        );
    }

    /// Helper: parse a let declaration wrapped in a minimal module structure and extract the Let node.
    /// Takes just the let declaration (e.g., "let payload : [u8; 8] = uninit @link_section(\"name\")")
    /// and returns (arena, extracted_let_result, diagnostics).
    fn parse_let_in_module(
        source: &str,
    ) -> (
        paideia_as_ast::AstArena,
        Result<paideia_as_ast::NodeId, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let wrapped = format!(r#"module Test = structure {{ {} }}"#, source);
        let (arena, result, diags) = parse_and_check(&wrapped);

        let extracted_result = match result {
            Ok(root_id) => {
                // parse_source_file returns an implicit root Structure containing top-level items
                if let Some(root_node) = arena.get(root_id) {
                    if let paideia_as_ast::NodeKind::Structure = root_node.kind {
                        // Get items from root structure (should contain the Module)
                        if let Some(paideia_as_ast::ItemData::Structure { items: root_items, .. }) = arena.item_data(root_id) {
                            // First item should be the Module
                            if let Some(&module_id) = root_items.first() {
                                if let Some(module_node) = arena.get(module_id) {
                                    if let paideia_as_ast::NodeKind::Module = module_node.kind {
                                        // Navigate Module -> body -> Structure -> items -> Let
                                        if let Some(paideia_as_ast::ItemData::Module { body, .. }) = arena.item_data(module_id) {
                                            let struct_id = *body;
                                            if let Some(struct_node) = arena.get(struct_id) {
                                                if let paideia_as_ast::NodeKind::Structure = struct_node.kind {
                                                    if let Some(paideia_as_ast::ItemData::Structure { items: struct_items, .. }) = arena.item_data(struct_id) {
                                                        if let Some(&let_id) = struct_items.first() {
                                                            Ok(let_id)
                                                        } else {
                                                            Err(ParseError)
                                                        }
                                                    } else {
                                                        Err(ParseError)
                                                    }
                                                } else {
                                                    Err(ParseError)
                                                }
                                            } else {
                                                Err(ParseError)
                                            }
                                        } else {
                                            Err(ParseError)
                                        }
                                    } else {
                                        Err(ParseError)
                                    }
                                } else {
                                    Err(ParseError)
                                }
                            } else {
                                Err(ParseError)
                            }
                        } else {
                            Err(ParseError)
                        }
                    } else {
                        Err(ParseError)
                    }
                } else {
                    Err(ParseError)
                }
            }
            Err(e) => Err(e),
        };

        (arena, extracted_result, diags)
    }

    #[test]
    fn link_section_happy_path_dot_uefi_hdr() {
        let source = r#"let payload : [u8; 8] = uninit @link_section(".uefi_hdr")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { link_section, .. }) = arena.item_data(root) {
                assert_eq!(*link_section, Some(".uefi_hdr".to_string()));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    #[test]
    fn link_section_no_leading_dot_accepted() {
        let source = r#"let payload : [u8; 8] = uninit @link_section("uefi_hdr")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { link_section, .. }) = arena.item_data(root) {
                assert_eq!(*link_section, Some("uefi_hdr".to_string()));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    #[test]
    fn link_section_empty_string_p0282() {
        let source = r#"let payload : [u8; 8] = uninit @link_section("")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 282);
        assert_p_code_message_contains(&diags, 282, "non-empty");
    }

    #[test]
    fn link_section_invalid_char_p0282() {
        let source = r#"let payload : [u8; 8] = uninit @link_section(".foo/bar")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 282);
        assert_p_code_message_contains(&diags, 282, "invalid");
    }

    #[test]
    fn link_section_too_long_p0282() {
        // 33-char name
        let source = r#"let payload : [u8; 8] = uninit @link_section("a_very_long_section_name_is_too_long_here")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 282);
        assert_p_code_message_contains(&diags, 282, "32");
    }

    #[test]
    fn link_section_duplicate_p0283() {
        let source = r#"let payload : [u8; 8] = uninit @link_section(".foo") @link_section(".bar")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 283);
        assert_p_code_message_contains(&diags, 283, "duplicate");
    }

    #[test]
    fn link_section_with_align_both_accepted() {
        let source = r#"let payload : [u8; 8] = uninit @align(64) @link_section(".foo")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { align, link_section, .. }) = arena.item_data(root) {
                assert_eq!(*align, Some(64));
                assert_eq!(*link_section, Some(".foo".to_string()));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    #[test]
    fn link_section_before_align_also_accepted() {
        let source = r#"let payload : [u8; 8] = uninit @link_section(".foo") @align(64)"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { align, link_section, .. }) = arena.item_data(root) {
                assert_eq!(*align, Some(64));
                assert_eq!(*link_section, Some(".foo".to_string()));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    #[test]
    fn abi_ms_parses() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("ms")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { abi, .. }) = arena.item_data(root) {
                assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Ms));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    #[test]
    fn abi_sysv_parses() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("sysv")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { abi, .. }) = arena.item_data(root) {
                assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Sysv));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    #[test]
    fn abi_unknown_string_p0285() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("unknown")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 285);
        assert_p_code_message_contains(&diags, 285, "invalid");
    }

    #[test]
    fn abi_uppercase_ms_p0285() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("MS")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 285);
        assert_p_code_message_contains(&diags, 285, "invalid");
    }

    #[test]
    fn abi_empty_string_p0285() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 285);
        assert_p_code_message_contains(&diags, 285, "non-empty");
    }

    #[test]
    fn abi_non_string_arg_p0285() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi(42)"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 285);
    }

    #[test]
    fn abi_missing_parens_p0285() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi"ms""#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 285);
        assert_p_code_message_contains(&diags, 285, "expected '('");
    }

    #[test]
    fn abi_duplicate_p0250() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("ms") @abi("sysv")"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(result.is_err(), "expected parse error");
        assert_has_p_code(&diags, 250);
        assert_p_code_message_contains(&diags, 250, "duplicate");
    }

    #[test]
    fn abi_with_align_and_link_section_all_accepted() {
        let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @align(64) @link_section(".text_custom") @abi("ms")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok());

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        if let paideia_as_ast::NodeKind::Let = node.kind {
            if let Some(paideia_as_ast::ItemData::Let { align, link_section, abi, .. }) = arena.item_data(root) {
                assert_eq!(*align, Some(64));
                assert_eq!(*link_section, Some(".text_custom".to_string()));
                assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Ms));
            } else {
                panic!("expected ItemData::Let");
            }
        } else {
            panic!("expected NodeKind::Let");
        }
    }

    /// paideia-as#1276 phase 1: `@no_frame` bare-flag attribute parses and populates
    /// `ItemData::Let::no_frame = true`. The attribute is inert this landing — no emit
    /// change — but it must round-trip through the parser so paideia-os can annotate
    /// hand-crafted trampolines in phase 2 without waiting on the elaborator.
    ///
    /// Lambda body shape mirrors the sibling `abi_*_parses` tests: `fn(params) -> body_expr`
    /// (bare-expression body). Only the attribute state is under test here.
    #[test]
    fn parse_no_frame_attribute() {
        let source = r#"pub let foo : (u64) -> u64 = fn(x: u64) -> x @no_frame"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok(), "expected successful parse");

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert!(matches!(node.kind, paideia_as_ast::NodeKind::Let), "expected NodeKind::Let");
        match arena.item_data(root) {
            Some(paideia_as_ast::ItemData::Let { no_frame, public, .. }) => {
                assert!(*no_frame, "@no_frame should set ItemData::Let::no_frame = true");
                assert!(*public, "`pub let` should preserve public=true alongside @no_frame");
            }
            _ => panic!("expected ItemData::Let"),
        }
    }

    /// paideia-as#1276 phase 1: absence of `@no_frame` leaves the flag at its
    /// `false` default — the emit path stays walkable-by-default and existing
    /// bindings inherit no unintended opt-out.
    #[test]
    fn no_frame_absent_by_default() {
        let source = r#"pub let foo : (u64) -> u64 = fn(x: u64) -> x"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok(), "expected successful parse");

        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert!(matches!(node.kind, paideia_as_ast::NodeKind::Let), "expected NodeKind::Let");
        match arena.item_data(root) {
            Some(paideia_as_ast::ItemData::Let { no_frame, .. }) => {
                assert!(
                    !*no_frame,
                    "no_frame must default to false when the attribute is omitted"
                );
            }
            _ => panic!("expected ItemData::Let"),
        }
    }

    /// paideia-as#1276 phase 1: `@no_frame` composes cleanly with the other four
    /// symbol attributes (`@align`, `@link_section`, `@abi`) in a single trailing
    /// attribute run, in any order. Guards against a future refactor that
    /// accidentally short-circuits the attribute loop once `no_frame` is seen.
    #[test]
    fn no_frame_composes_with_other_attributes() {
        let source = r#"pub let foo : (u64) -> u64 = fn(x: u64) -> x @no_frame @abi("sysv")"#;
        let (arena, result, diags) = parse_let_in_module(source);

        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(result.is_ok(), "expected successful parse");

        let root = result.unwrap();
        match arena.item_data(root) {
            Some(paideia_as_ast::ItemData::Let { no_frame, abi, .. }) => {
                assert!(*no_frame, "@no_frame flag set");
                assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Sysv));
            }
            _ => panic!("expected ItemData::Let"),
        }
    }

    /// paideia-as#1276 phase 3: `@no_frame` on a non-function binding is a
    /// category error — the attribute toggles the SysV frame-pointer
    /// prologue/epilogue emitted around function bodies, and there is no
    /// prologue/epilogue to suppress for e.g. `pub let CONST : u64 = 42`.
    /// The parser rejects such placement with P0250 so the elaborator can
    /// trust that `LetInfo::no_frame == true` implies a Lambda RHS.
    #[test]
    fn no_frame_on_non_fn_binding_errors() {
        // Literal RHS — must be diagnosed.
        let source = r#"pub let CONST : u64 = 42 @no_frame"#;
        let (_arena, result, diags) = parse_let_in_module(source);

        assert!(
            result.is_err(),
            "@no_frame on a non-function binding must be a parse error, got Ok"
        );
        assert!(
            !diags.is_empty(),
            "expected at least one diagnostic for @no_frame on non-fn binding, got none"
        );
        let has_p0250 = diags.iter().any(|d| {
            let code = d.code();
            code.category() == paideia_as_diagnostics::Category::P
                && code.number() == 250
        });
        assert!(
            has_p0250,
            "expected P0250 diagnostic for @no_frame on non-fn binding, got: {:?}",
            diags
        );
    }
}

