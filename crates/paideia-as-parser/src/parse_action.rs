//! Action block parsing.
//!
//! Implements §8 ActionBlock grammar: `action (!{effects})? (@{caps})? { stmts }`.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse an action block: `action !{effects}? @{caps}? { stmts }`.
    ///
    /// **Algorithm:**
    /// 1. Expect `KwAction`.
    /// 2. Optional effect set: if `EffectOpen`, parse via `parse_effect_row()`.
    /// 3. Optional capability set: if `CapOpen`, parse via `parse_cap_set()`.
    /// 4. Expect `LBrace`.
    /// 5. Loop: parse statements via `parse_stmt()` until `RBrace`.
    ///    At least one statement is required (§8 says `Stmt+`).
    /// 6. Expect `RBrace`.
    /// 7. Allocate `ExprData::ActionBlock { effects, capabilities, body }`.
    ///
    /// Returns the `NodeId` of the allocated expression on success.
    pub(crate) fn parse_action(&mut self) -> Result<NodeId, ParseError> {
        let action_tok = self.expect(TokenKind::KwAction)?;
        let span_start = action_tok.span;

        // Optional effect set: `!{...}`
        let effects = if self.at(TokenKind::EffectOpen) {
            Some(self.parse_effect_row()?)
        } else {
            None
        };

        // Optional capability set: `@{...}`
        let capabilities = if self.at(TokenKind::CapOpen) {
            Some(self.parse_cap_set()?)
        } else {
            None
        };

        // Expect opening brace
        self.expect(TokenKind::LBrace)?;

        let mut body = Vec::new();

        // Parse statements until closing brace.
        // Pass `in_action_context = true` to enable instruction statement parsing.
        loop {
            if self.at(TokenKind::RBrace) {
                break;
            }

            let stmt = self.parse_stmt(true)?;
            body.push(stmt);
        }

        // At least one statement is required per §8
        if body.is_empty() {
            let rbrace_tok = self.peek().ok_or(ParseError)?;
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 150).expect("valid P code");
            let diag = Diagnostic::error(code)
                .message("action block must contain at least one statement")
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

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprActionBlock,
            span,
            ExprData::ActionBlock {
                effects,
                capabilities,
                body,
            },
        ))
    }
}

// Tests will be in integration tests; parse_action is internal to the parser module.
