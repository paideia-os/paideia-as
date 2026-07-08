//! Type-parser error emitters and the `is_type_start` predicate.
//! Split out of `parse_type.rs` (2026-07-08).

use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn is_type_start(&self, opt_tok: Option<&paideia_as_lexer::Token>) -> bool {
        if let Some(tok) = opt_tok {
            matches!(
                tok.kind,
                TokenKind::Ident
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::EffectOpen
                    | TokenKind::CapOpen
                    | TokenKind::Star
                    | TokenKind::Amp
                    | TokenKind::KwRecord
                    | TokenKind::KwEnum
                    | TokenKind::KwSelfType
                    | TokenKind::KwOrdered
                    | TokenKind::KwLinear
                    | TokenKind::KwAffine
                    | TokenKind::KwUnrestricted
                    | TokenKind::LinearMark
                    | TokenKind::AffineMark
            )
        } else {
            false
        }
    }

    /// Emit a P0100 ("expected type") diagnostic and return Err.
    pub(super) fn error_expected_type(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            Span::new(self.file(), 0, 0)
        };
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                100,
            )
            .unwrap(),
        )
        .message("expected type".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Emit a P0195 ("malformed pointer type") diagnostic and return Err.
    pub(super) fn error_malformed_ptr(
        &mut self,
        span: paideia_as_diagnostics::Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                195,
            )
            .unwrap(),
        )
        .message("expected type after '*'".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Emit a P0196 ("malformed reference type") diagnostic and return Err.
    pub(super) fn error_malformed_ref(
        &mut self,
        span: paideia_as_diagnostics::Span,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                196,
            )
            .unwrap(),
        )
        .message("expected type after '&'".to_string())
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Parse a record type: `record { field1: Type1, field2: Type2, ... }`.
    ///
    /// Consumes `record` keyword, expects LBrace, parses field declarations
    /// (Ident : Type pairs separated by commas, trailing comma allowed),
    /// and closes with RBrace.
    pub(super) fn error_malformed_record(
        &mut self,
        span: paideia_as_diagnostics::Span,
        reason: &str,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                197,
            )
            .unwrap(),
        )
        .message(format!("malformed record type: {}", reason))
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Parse an enum type: `enum { Variant1, Variant2(T1, T2), Variant3 { f1: T1 }, ... }`.
    ///
    /// Consumes `enum` keyword, expects LBrace, parses variants (unit, tuple, or record payload),
    /// separated by Comma (trailing OK), closes RBrace.
    pub(crate) fn error_malformed_enum(
        &mut self,
        span: paideia_as_diagnostics::Span,
        reason: &str,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                198,
            )
            .unwrap(),
        )
        .message(format!("malformed enum type: {}", reason))
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Parse a fixed-size array type: `[T; N]`.
    ///
    /// Syntax: `LBracket Type Semicolon Expr RBracket`.
    /// The length is parsed as a primary expression (any primary expression is valid
    /// syntactically; semantic constraints to constant values are enforced at
    /// type elaboration, not at parse time).
    ///
    /// Returns a TypeArray node with element type and length expression.
    ///
    /// Errors:
    /// - P0199: malformed array type (missing length, missing `;`, etc.)
    pub(super) fn error_malformed_array(
        &mut self,
        span: paideia_as_diagnostics::Span,
        reason: &str,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let diag = paideia_as_diagnostics::Diagnostic::error(
            paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::P,
                paideia_as_diagnostics::Severity::Error,
                199,
            )
            .unwrap(),
        )
        .message(format!("malformed array type: {}", reason))
        .with_span(span)
        .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

}
