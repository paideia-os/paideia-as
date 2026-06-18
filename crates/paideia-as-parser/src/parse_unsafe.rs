//! Unsafe block parsing.
//!
//! Implements §8 UnsafeExpr grammar: `unsafe { effects: ..., capabilities: ..., justification: ..., block: ... }`.
//!
//! The unsafe block must contain four mandatory fields in any order:
//! - `effects: { Ident, Ident, ... }`
//! - `capabilities: { Ident (. Ident)*, Ident (. Ident)*, ... }`
//! - `justification: "string literal"`
//! - `block: { Stmt+ }`
//!
//! Phase-1 implementation: Fields must appear in the order declared in §8.
//! If any field is missing when the closing `}` is encountered, emit exactly
//! one U1600 diagnostic listing all missing fields, spanning the closing brace.
//!
//! Capabilities support dotted paths (e.g., `Mmio.read_cap`) in phase-1 and later.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a single capability path, which can be:
    /// - bare identifier: `raw_cap`
    /// - dotted path: `Mmio.read_cap` (accumulated as sequential Ident nodes)
    ///
    /// Returns a Vec of NodeIds representing the path segments.
    /// For phase-1, we accumulate the segments just like `parse_cap_set()` does.
    fn parse_capability_path(&mut self) -> Result<Vec<NodeId>, ParseError> {
        let mut items = Vec::new();

        if self.at(TokenKind::Ident) {
            let ident_tok = self.bump().unwrap();
            items.push(self.arena_mut().alloc(NodeKind::Ident, ident_tok.span));

            // Check for dot-separated path continuation
            while self.at(TokenKind::Dot) {
                self.bump(); // consume `.`

                if let Some(next_tok) = self.peek() {
                    if next_tok.kind == TokenKind::Ident {
                        let next_ident_tok = self.bump().unwrap();
                        items.push(self.arena_mut().alloc(NodeKind::Ident, next_ident_tok.span));
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        Ok(items)
    }

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
                    // Parse capabilities: { CapPath, CapPath, ... }
                    // where CapPath is either a bare identifier or a dotted path (Mmio.read_cap)
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
                            // Parse capability paths (dotted or bare identifiers)
                            let cap_path = self.parse_capability_path()?;
                            items.extend(cap_path);

                            if !self.at(TokenKind::Comma) {
                                break;
                            }
                            self.bump();
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

                        // Unsafe block statements: pass in_action_context=false
                        let stmt = self.parse_stmt(false)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, VecSink};
    use paideia_as_lexer::{Token, TokenKind};

    fn tok(kind: TokenKind, byte_start: u32) -> Token {
        Token::new(kind, Span::new(FileId::new(1).unwrap(), byte_start, 1))
    }

    fn parse_unsafe_block(
        tokens: Vec<Token>,
    ) -> (
        AstArena,
        Result<NodeId, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_unsafe()
        };
        (arena, result, sink.diagnostics().to_vec())
    }

    #[test]
    fn unsafe_capabilities_accepts_bare_ident() {
        // unsafe { effects: {}, capabilities: {raw_cap}, justification: "...", block: { sfence } }
        let tokens = vec![
            tok(TokenKind::KwUnsafe, 0),
            tok(TokenKind::LBrace, 7),
            tok(TokenKind::Ident, 9), // effects
            tok(TokenKind::Colon, 16),
            tok(TokenKind::LBrace, 18),
            tok(TokenKind::RBrace, 19),
            tok(TokenKind::Comma, 20),
            tok(TokenKind::Ident, 22), // capabilities
            tok(TokenKind::Colon, 34),
            tok(TokenKind::LBrace, 36),
            tok(TokenKind::Ident, 37), // raw_cap
            tok(TokenKind::RBrace, 45),
            tok(TokenKind::Comma, 46),
            tok(TokenKind::Ident, 48), // justification
            tok(TokenKind::Colon, 61),
            tok(TokenKind::StringLit, 63),
            tok(TokenKind::Comma, 85),
            tok(TokenKind::Ident, 87), // block
            tok(TokenKind::Colon, 92),
            tok(TokenKind::LBrace, 94),
            tok(TokenKind::Ident, 96), // sfence
            tok(TokenKind::RBrace, 102),
            tok(TokenKind::RBrace, 104),
            tok(TokenKind::Eof, 105),
        ];
        let (arena, result, diags) = parse_unsafe_block(tokens);

        assert_eq!(diags.len(), 0, "Expected no diagnostics");
        assert!(result.is_ok(), "Expected parse success");

        let expr_id = result.unwrap();
        let expr_node = arena.get(expr_id).unwrap();
        assert_eq!(expr_node.kind, NodeKind::ExprUnsafe);
    }

    #[test]
    fn unsafe_capabilities_accepts_dotted_path() {
        // unsafe { effects: {}, capabilities: {Mmio.read_cap}, justification: "...", block: { sfence } }
        let tokens = vec![
            tok(TokenKind::KwUnsafe, 0),
            tok(TokenKind::LBrace, 7),
            tok(TokenKind::Ident, 9), // effects
            tok(TokenKind::Colon, 16),
            tok(TokenKind::LBrace, 18),
            tok(TokenKind::RBrace, 19),
            tok(TokenKind::Comma, 20),
            tok(TokenKind::Ident, 22), // capabilities
            tok(TokenKind::Colon, 34),
            tok(TokenKind::LBrace, 36),
            tok(TokenKind::Ident, 37), // Mmio
            tok(TokenKind::Dot, 41),
            tok(TokenKind::Ident, 42), // read_cap
            tok(TokenKind::RBrace, 50),
            tok(TokenKind::Comma, 51),
            tok(TokenKind::Ident, 53), // justification
            tok(TokenKind::Colon, 66),
            tok(TokenKind::StringLit, 68),
            tok(TokenKind::Comma, 90),
            tok(TokenKind::Ident, 92), // block
            tok(TokenKind::Colon, 97),
            tok(TokenKind::LBrace, 99),
            tok(TokenKind::Ident, 101), // sfence
            tok(TokenKind::RBrace, 107),
            tok(TokenKind::RBrace, 109),
            tok(TokenKind::Eof, 110),
        ];
        let (arena, result, diags) = parse_unsafe_block(tokens);

        assert_eq!(diags.len(), 0, "Expected no diagnostics");
        assert!(result.is_ok(), "Expected parse success");

        let expr_id = result.unwrap();
        let expr_node = arena.get(expr_id).unwrap();
        assert_eq!(expr_node.kind, NodeKind::ExprUnsafe);
    }

    #[test]
    fn unsafe_capabilities_accepts_multiple_dotted() {
        // unsafe { effects: {}, capabilities: {Mmio.read_cap, Pci.config_read_cap}, justification: "...", block: { sfence } }
        let tokens = vec![
            tok(TokenKind::KwUnsafe, 0),
            tok(TokenKind::LBrace, 7),
            tok(TokenKind::Ident, 9), // effects
            tok(TokenKind::Colon, 16),
            tok(TokenKind::LBrace, 18),
            tok(TokenKind::RBrace, 19),
            tok(TokenKind::Comma, 20),
            tok(TokenKind::Ident, 22), // capabilities
            tok(TokenKind::Colon, 34),
            tok(TokenKind::LBrace, 36),
            tok(TokenKind::Ident, 37), // Mmio
            tok(TokenKind::Dot, 41),
            tok(TokenKind::Ident, 42), // read_cap
            tok(TokenKind::Comma, 50),
            tok(TokenKind::Ident, 52), // Pci
            tok(TokenKind::Dot, 55),
            tok(TokenKind::Ident, 56), // config_read_cap
            tok(TokenKind::RBrace, 70),
            tok(TokenKind::Comma, 71),
            tok(TokenKind::Ident, 73), // justification
            tok(TokenKind::Colon, 86),
            tok(TokenKind::StringLit, 88),
            tok(TokenKind::Comma, 110),
            tok(TokenKind::Ident, 112), // block
            tok(TokenKind::Colon, 117),
            tok(TokenKind::LBrace, 119),
            tok(TokenKind::Ident, 121), // sfence
            tok(TokenKind::RBrace, 127),
            tok(TokenKind::RBrace, 129),
            tok(TokenKind::Eof, 130),
        ];
        let (arena, result, diags) = parse_unsafe_block(tokens);

        assert_eq!(diags.len(), 0, "Expected no diagnostics");
        assert!(result.is_ok(), "Expected parse success");

        let expr_id = result.unwrap();
        let expr_node = arena.get(expr_id).unwrap();
        assert_eq!(expr_node.kind, NodeKind::ExprUnsafe);
    }

    #[test]
    fn unsafe_capabilities_mixed_bare_and_dotted() {
        // unsafe { effects: {}, capabilities: {raw_cap, Mmio.read_cap}, justification: "...", block: { sfence } }
        let tokens = vec![
            tok(TokenKind::KwUnsafe, 0),
            tok(TokenKind::LBrace, 7),
            tok(TokenKind::Ident, 9), // effects
            tok(TokenKind::Colon, 16),
            tok(TokenKind::LBrace, 18),
            tok(TokenKind::RBrace, 19),
            tok(TokenKind::Comma, 20),
            tok(TokenKind::Ident, 22), // capabilities
            tok(TokenKind::Colon, 34),
            tok(TokenKind::LBrace, 36),
            tok(TokenKind::Ident, 37), // raw_cap
            tok(TokenKind::Comma, 45),
            tok(TokenKind::Ident, 47), // Mmio
            tok(TokenKind::Dot, 51),
            tok(TokenKind::Ident, 52), // read_cap
            tok(TokenKind::RBrace, 60),
            tok(TokenKind::Comma, 61),
            tok(TokenKind::Ident, 63), // justification
            tok(TokenKind::Colon, 76),
            tok(TokenKind::StringLit, 78),
            tok(TokenKind::Comma, 100),
            tok(TokenKind::Ident, 102), // block
            tok(TokenKind::Colon, 107),
            tok(TokenKind::LBrace, 109),
            tok(TokenKind::Ident, 111), // sfence
            tok(TokenKind::RBrace, 117),
            tok(TokenKind::RBrace, 119),
            tok(TokenKind::Eof, 120),
        ];
        let (arena, result, diags) = parse_unsafe_block(tokens);

        assert_eq!(diags.len(), 0, "Expected no diagnostics");
        assert!(result.is_ok(), "Expected parse success");

        let expr_id = result.unwrap();
        let expr_node = arena.get(expr_id).unwrap();
        assert_eq!(expr_node.kind, NodeKind::ExprUnsafe);
    }
}
