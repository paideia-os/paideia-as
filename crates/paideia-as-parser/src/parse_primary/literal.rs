//! Simple literal parsing helpers (numeric, boolean, null, string, byte-string,
//! `self` / `Self`, `break`, `continue`).
//!
//! Extracted from `parse_primary/mod.rs` (issue #1407 God-file split).
//! Each helper is `pub(super)` — invoked only by `parse_primary`'s dispatch
//! table. Semantics and diagnostic codes are preserved verbatim.

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::{extract_byte_string_content, extract_string_content};

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Consume the current token and allocate a `Placeholder` + `ExprLiteral`
    /// wrapper anchored at `span`.
    ///
    /// This is the shared body of the numeric-, character-, byte-,
    /// `true` / `false` / `null` arms in `parse_primary`. A future PR will
    /// swap the synthetic Placeholder for dedicated NodeKind variants
    /// (BoolLit, NullLit, etc.); until then the parser wraps them all
    /// through this single helper so the arm bodies stay one-liners.
    pub(super) fn parse_placeholder_literal(
        &mut self,
        span: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.bump();
        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span);
        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLiteral,
            span,
            ExprData::Literal { lit: lit_id },
        ))
    }

    /// Consume a `StringLit` token and return an `ExprString` node whose
    /// payload is the decoded UTF-8 byte content (escapes resolved, quotes
    /// stripped, `r` / `br` / `rb` prefixes honored). On decode failure
    /// emit `E0004` and fall back to a placeholder-wrapped `ExprLiteral`.
    pub(super) fn parse_string_lit_expr(
        &mut self,
        span_start: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.bump();
        let source = self.source();

        // Extract the token's text from the source
        let start = span_start.byte_start() as usize;
        let end = (span_start.byte_start() + span_start.byte_len()) as usize;
        let token_text = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Determine if this is a raw string by checking the token text
        let is_raw = token_text.starts_with('r')
            || token_text.starts_with("br")
            || token_text.starts_with("rb");

        match extract_string_content(token_text, 0, is_raw, false) {
            Ok(bytes) => Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprString,
                span_start,
                ExprData::StringLiteral(bytes),
            )),
            Err(_err) => {
                // Emit diagnostic and fall back to placeholder
                let diag = Diagnostic::error(
                    DiagnosticCode::new(Category::E, Severity::Error, 4)
                        .expect("valid E code"),
                )
                .message("invalid string literal")
                .with_span(span_start)
                .finish();
                self.emit_diagnostic(diag);
                let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprLiteral,
                    span_start,
                    ExprData::Literal { lit: lit_id },
                ))
            }
        }
    }

    /// Consume a `ByteStringLit` token and return an `ExprByteString` node
    /// whose payload is the decoded bytes. Same failure recovery as
    /// `parse_string_lit_expr` (`E0004` + placeholder fallback).
    pub(super) fn parse_byte_string_lit_expr(
        &mut self,
        span_start: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.bump();
        let source = self.source();

        // Extract the token's text from the source
        let start = span_start.byte_start() as usize;
        let end = (span_start.byte_start() + span_start.byte_len()) as usize;
        let token_text = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Check for 'br' or 'rb' prefix
        let is_raw = token_text.starts_with("br") || token_text.starts_with("rb");

        match extract_byte_string_content(token_text, 0, is_raw) {
            Ok(content) => Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprByteString,
                span_start,
                ExprData::ByteStringLiteral(content),
            )),
            Err(_err) => {
                // Emit diagnostic and fall back to placeholder
                let diag = Diagnostic::error(
                    DiagnosticCode::new(Category::E, Severity::Error, 4)
                        .expect("valid E code"),
                )
                .message("invalid byte string literal")
                .with_span(span_start)
                .finish();
                self.emit_diagnostic(diag);
                let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprLiteral,
                    span_start,
                    ExprData::Literal { lit: lit_id },
                ))
            }
        }
    }

    /// Consume `KwSelfType` / `KwSelfValue` and wrap it as a single-segment
    /// `ExprPath`.
    pub(super) fn parse_self_expr(
        &mut self,
        span_start: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.bump();
        let ident_id = self.arena_mut().alloc(NodeKind::Ident, span_start);
        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprPath,
            span_start,
            ExprData::Path {
                segments: vec![ident_id],
            },
        ))
    }

    /// Consume `KwBreak` and allocate an `ExprBreak` node.
    pub(super) fn parse_break_expr(
        &mut self,
        span_start: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.bump();
        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprBreak,
            span_start,
            ExprData::Break,
        ))
    }

    /// Consume `KwContinue` and allocate an `ExprContinue` node.
    pub(super) fn parse_continue_expr(
        &mut self,
        span_start: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.bump();
        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprContinue,
            span_start,
            ExprData::Continue,
        ))
    }
}

