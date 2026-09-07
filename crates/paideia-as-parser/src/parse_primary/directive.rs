//! Compile-time inline-directive dispatcher: `@guid(...)`, `@include_bytes(...)`,
//! `@include_str(...)`, `@include_bytes_as_str(...)`.
//!
//! The per-directive parsing (validation, file resolution, size limits,
//! UTF-8 handling) lives in `embed.rs`. This file owns just the shared
//! `@Ident` prefix parse and the name -> parser dispatch — the code that
//! used to be `parse_inline_directive` in `mod.rs`.
//!
//! Extracted from `parse_primary/mod.rs` (issue #1407 God-file split).

use paideia_as_diagnostics::Diagnostic;
use paideia_as_lexer::TokenKind;

use super::p_code;
use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a compile-time inline directive: `@guid("...")`, `@include_bytes("...")`, etc.
    ///
    /// Algorithm:
    /// 1. Expect `@` token.
    /// 2. Peek next token; must be Ident (the directive name).
    /// 3. Dispatch based on the directive name (guid, include_bytes, etc.).
    /// 4. Each directive parser returns an ExprInlineBytes on success.
    pub(super) fn parse_inline_directive(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let at_tok = self.expect(TokenKind::At)?;
        let at_span = at_tok.span;

        // Next token must be an identifier (directive name)
        let next_tok = if let Some(tok) = self.peek() {
            if tok.kind == TokenKind::Ident {
                tok
            } else {
                let diag = Diagnostic::error(p_code(100))
                    .message("expected directive name after @".to_string())
                    .with_span(tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        } else {
            let diag = Diagnostic::error(p_code(100))
                .message("expected directive name after @".to_string())
                .with_span(at_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        // Extract the directive name from source
        let source = self.source();
        let start = next_tok.span.byte_start() as usize;
        let end = (next_tok.span.byte_start() + next_tok.span.byte_len()) as usize;
        let directive_name = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Dispatch based on directive name
        match directive_name {
            "guid" => {
                self.bump(); // consume directive name
                self.parse_guid_literal(at_span)
            }
            "include_bytes" => {
                self.bump(); // consume directive name
                self.parse_include_bytes_literal(at_span)
            }
            "include_str" => {
                self.bump(); // consume directive name
                self.parse_include_str_literal(at_span, true)
            }
            "include_bytes_as_str" => {
                self.bump(); // consume directive name
                self.parse_include_str_literal(at_span, false)
            }
            _ => {
                let diag = Diagnostic::error(p_code(100))
                    .message(format!("unknown directive @{}", directive_name))
                    .with_span(next_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
        }
    }
}

