//! Primary expression parsing: literals, identifiers, paths, and parenthesized
//! expressions.
//!
//! Primary expressions are the atoms of the syntax tree — the base cases that
//! other expression categories build upon. This module implements the `parse_primary`
//! method that dispatches on token kind and constructs the appropriate AST nodes.
//!
//! # Structure (post-#1407 refactor)
//!
//! Originally this file held ~720 LOC of parsing logic plus ~1560 LOC of
//! tests. It was split into sibling sub-files under `parse_primary/` so
//! `mod.rs` reduces to the dispatch table plus its two error helpers.
//! All public paths (`Parser::parse_primary`, `Parser::parse_path_or_ident`)
//! are preserved verbatim; every helper the dispatcher calls lives in one
//! of the sibling files below.
//!
//! - `literal.rs`   — placeholder / string / byte-string / self / break / continue
//! - `path.rs`      — `parse_path_or_ident` (pub(crate); called from `modules.rs`,
//!                    `parse_handler.rs`, `parse_type/type_kinds.rs`)
//! - `collection.rs` — `[...]` array literals and `(...)` paren / tuple / unit
//! - `effect.rs`    — `perform` and `resume`
//! - `directive.rs` — `@Ident(...)` inline directive dispatch
//! - `embed.rs`     — per-directive parsers plus the file-read helper and GUID
//!                    codec (pre-existing sibling from the 2026-08-11 split)
//! - `tests.rs`     — the entire test suite

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser, reserved_keyword_hint};

mod collection;
mod directive;
mod effect;
mod embed;
mod literal;
mod path;

#[cfg(test)]
mod tests;

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a primary expression (atom).
    ///
    /// Dispatches on the current token kind:
    /// - **Literals** (IntLit, FloatLit, CharLit, StringLit, ByteLit, ByteStringLit):
    ///   allocate a Placeholder node for the literal, wrap in ExprLiteral.
    /// - **Boolean keywords** (KwTrue, KwFalse, KwNull):
    ///   allocate synthetic Placeholder nodes, wrap in ExprLiteral.
    /// - **Identifiers**: parse as a path with segments separated by `::`.
    /// - **KwSelfType / KwSelfValue**: treat as a single-segment path.
    /// - **LBracket**: parse as array literal `[expr, expr, ...]`. Empty array requires
    ///   explicit type annotation and emits P0210.
    /// - **LParen**: disambiguate between `()` (unit), `(expr)` (parenthesized),
    ///   and `(a, b, c)` (tuple; currently stubbed as Placeholder).
    /// - **Otherwise**: emit P0100 "expected expression" and return Err.
    ///
    /// Note: Block expressions, lambdas, and control-flow constructs are
    /// dispatched in `parse_expr_bp` Step 0, before primary parsing.
    ///
    /// On parse failure, returns `Err(ParseError)` after emitting a diagnostic.
    /// The caller is responsible for calling [`Parser::recover_to_one_of`] to
    /// synchronize if needed.
    ///
    /// Returns the `NodeId` of the allocated expression on success.
    pub fn parse_primary(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let (tok_kind, span_start) = match self.peek() {
            None => return self.error_expected_expression(),
            Some(tok) => (tok.kind, tok.span),
        };

        match tok_kind {
            // Numeric and character literals + boolean/null constants: all
            // consume the current token and wrap a Placeholder in ExprLiteral.
            // A future PR will replace the synthetic Placeholder with
            // dedicated NodeKind variants (BoolLit, NullLit, ...).
            TokenKind::IntLit
            | TokenKind::FloatLit
            | TokenKind::CharLit
            | TokenKind::ByteLit
            | TokenKind::KwTrue
            | TokenKind::KwFalse
            | TokenKind::KwNull => self.parse_placeholder_literal(span_start),

            TokenKind::StringLit => self.parse_string_lit_expr(span_start),
            TokenKind::ByteStringLit => self.parse_byte_string_lit_expr(span_start),

            // Effect operations
            TokenKind::KwPerform => self.parse_perform(),
            TokenKind::KwResume => self.parse_resume(),

            // Antiquotation (only if followed by `(`)
            TokenKind::AffineMark
                if self.peek_at(1).is_some_and(|t| t.kind == TokenKind::LParen) =>
            {
                self.parse_antiquote_expr()
            }

            // Identifiers and paths (including contextual keyword "handle" and "quote")
            TokenKind::Ident if self.peek_ident_text() == Some("handle") => {
                self.parse_handler_value()
            }
            TokenKind::Ident => self.parse_path_or_ident(),

            TokenKind::KwSelfType | TokenKind::KwSelfValue => self.parse_self_expr(span_start),

            // Array literals
            TokenKind::LBracket => self.parse_array_lit(),

            // Parenthesized expressions and tuples
            TokenKind::LParen => self.parse_paren_expr(),

            // Break and continue expressions
            TokenKind::KwBreak => self.parse_break_expr(span_start),
            TokenKind::KwContinue => self.parse_continue_expr(span_start),

            // Compile-time directives: @guid, @include_bytes, etc.
            TokenKind::At => self.parse_inline_directive(),

            // Anything else is an error
            // (Block expressions are handled in parse_expr_bp Step 0)
            _ => self.error_expected_expression(),
        }
    }

    /// Emit a P0100 ("expected expression") diagnostic and return `Err(ParseError)`.
    ///
    /// #1327: when the offending token is a reserved keyword — the common
    /// case for the mount.pdx-style regression `[rip + record]`, where
    /// `record` was made reserved by #637 — extend the message with the
    /// same actionable "rename it" hint that #1263 established for
    /// `expect(Ident)`. The bare "expected expression" caret pointed at a
    /// keyword told the user nothing about *why* their symbol reference
    /// failed to parse.
    fn error_expected_expression(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let (span, msg) = if let Some(tok) = self.peek() {
            let msg = reserved_keyword_hint(tok.kind, "expression")
                .unwrap_or_else(|| "expected expression".to_string());
            (tok.span, msg)
        } else {
            // At EOF: use a zero-width span at byte 0
            (Span::new(self.file(), 0, 0), "expected expression".to_string())
        };

        let diag = Diagnostic::error(p_code(100))
            .message(msg)
            .with_span(span)
            .finish();
        self.emit_diagnostic(diag);

        Err(ParseError)
    }

    /// Emit a P0101 ("mismatched delimiter") diagnostic and return `Err(ParseError)`.
    ///
    /// Called when an opening paren/brace has no matching closing paren/brace.
    fn error_mismatched_delimiter(
        &mut self,
        _opening_span: Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            Span::new(self.file(), 0, 0)
        };

        let diag = Diagnostic::error(p_code(101))
            .message("mismatched delimiter: expected `)`".to_string())
            .with_span(span)
            .finish();
        self.emit_diagnostic(diag);

        Err(ParseError)
    }
}

/// Construct a P-category diagnostic code at the given number, returning
/// the `DiagnosticCode`.
fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
}
