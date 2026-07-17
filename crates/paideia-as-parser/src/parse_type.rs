//! Type parsing.
//!
//! Implements §8 Type grammar: function arrows, effect rows, capability sets,
//! linear classes, and quantified types.

use paideia_as_ast::{LinClass, NodeKind, TypeData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};


// --- Split submodules (2026-07-08 refactor) ---
mod effect_row;
mod errors;
mod type_kinds;
mod type_shape;

#[cfg(test)]
mod tests;

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a type according to §8 Type grammar.
    ///
    /// Dispatch:
    /// 1. **forall quantifier**: if `forall` keyword, consume and parse
    ///    bound variable (discarded in phase-1), then recursively parse inner type.
    /// 2. **Linearity class prefix**: if keyword or glyph marker (`linear`, `~`, etc.),
    ///    consume, recurse, and wrap in `TypeLinearClass`.
    /// 3. **LParen**: disambiguate paren, tuple, or function arrow.
    /// 4. **Ident**: base type name, optionally with type arguments.
    /// 5. **EffectOpen (`!{`)**: parse effect row.
    /// 6. **CapOpen (`@{`)**: parse capability set (phase-1: stored as `TypeEffectRow`).
    ///
    /// Returns the `NodeId` of the allocated type node.
    pub fn parse_type(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Step 1: Handle `forall` quantifier
        if self.at(TokenKind::KwForall) {
            self.bump(); // consume `forall`

            // Expect the quantified variable name
            self.expect(TokenKind::Ident)?; // discarded in phase-1; document

            // Expect `.` separator
            self.expect(TokenKind::Dot)?;

            // Recursively parse the inner type (the quantified var is not attached)
            return self.parse_type_unquantified();
        }

        // Step 2-6: Parse non-quantified type
        self.parse_type_unquantified()
    }

    /// Parse a type without a `forall` quantifier prefix.
    ///
    /// Handles:
    /// - Linearity class prefix
    /// - LParen (paren, tuple, arrow)
    /// - Ident (type name)
    /// - EffectOpen/CapOpen (effect/capability rows)
    pub(crate) fn parse_type_unquantified(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Step 1: Check for linearity class prefix
        if let Some(tok) = self.peek() {
            match tok.kind {
                TokenKind::KwOrdered
                | TokenKind::KwLinear
                | TokenKind::KwAffine
                | TokenKind::KwUnrestricted
                | TokenKind::LinearMark
                | TokenKind::AffineMark => {
                    let prefix_tok = self.bump().unwrap();
                    let class = match prefix_tok.kind {
                        TokenKind::KwOrdered => LinClass::Ordered,
                        TokenKind::KwLinear => LinClass::Linear,
                        TokenKind::KwAffine => LinClass::Affine,
                        TokenKind::KwUnrestricted => LinClass::Unrestricted,
                        TokenKind::LinearMark => LinClass::LinearMark,
                        TokenKind::AffineMark => LinClass::AffineMark,
                        _ => unreachable!(),
                    };

                    // Recursively parse the inner type
                    let inner = self.parse_type_unquantified()?;

                    // Allocate TypeLinearClass node
                    let span_start = prefix_tok.span;
                    let span_end = self
                        .arena()
                        .get(inner)
                        .map(|nd| nd.span)
                        .unwrap_or(span_start);
                    let span = Span::new(
                        span_start.file(),
                        span_start.byte_start(),
                        span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
                    );

                    return Ok(self.arena_mut().alloc_type(
                        NodeKind::TypeLinearClass,
                        span,
                        TypeData::LinearClass { class, inner },
                    ));
                }
                _ => {}
            }
        }

        // Step 2: Handle pointer type prefix `*` or reference type prefix `&`/`&mut` or closure type `|...`
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::Pipe) => self.parse_type_closure(),
            Some(TokenKind::Star) => {
                let star_tok = self.bump().unwrap();
                if !self.is_type_start(self.peek()) {
                    return self.error_malformed_ptr(star_tok.span);
                }
                let pointee = self.parse_type_unquantified()?;
                let span_end = self
                    .arena()
                    .get(pointee)
                    .map(|nd| nd.span)
                    .unwrap_or(star_tok.span);
                let span = Span::new(
                    star_tok.span.file(),
                    star_tok.span.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - star_tok.span.byte_start(),
                );
                Ok(self
                    .arena_mut()
                    .alloc_type(NodeKind::TypePtr, span, TypeData::Ptr { pointee }))
            }
            Some(TokenKind::Amp) => {
                let amp_tok = self.bump().unwrap();

                // Check for optional `mut` keyword
                let mutable = if self.at(TokenKind::KwMut) {
                    self.bump();
                    true
                } else {
                    false
                };

                // Check for lifetime (parse-clean: consume but don't elaborate)
                // A lifetime looks like: &'name Type
                // Phase 4 m5-003: If we see an Ident that looks like a lifetime (i.e., lexeme starts with ')
                // consume it and continue to parse the actual type.
                if self.at(TokenKind::Ident) {
                    if let Some(tok) = self.peek() {
                        let source = self.source();
                        let start = tok.span.byte_start() as usize;
                        let end = (tok.span.byte_start() + tok.span.byte_len()) as usize;
                        if start < source.len() && end <= source.len() {
                            let lexeme = &source[start..end];
                            if lexeme.starts_with('\'') {
                                // This is a lifetime token; consume it but don't elaborate
                                self.bump();
                            }
                        }
                    }
                }

                if !self.is_type_start(self.peek()) {
                    return self.error_malformed_ref(amp_tok.span);
                }

                let pointee = self.parse_type_unquantified()?;
                let span_end = self
                    .arena()
                    .get(pointee)
                    .map(|nd| nd.span)
                    .unwrap_or(amp_tok.span);
                let span = Span::new(
                    amp_tok.span.file(),
                    amp_tok.span.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - amp_tok.span.byte_start(),
                );
                Ok(self.arena_mut().alloc_type(
                    NodeKind::TypeRef,
                    span,
                    TypeData::Ref { pointee, mutable },
                ))
            }
            Some(TokenKind::KwRecord) => self.parse_type_record(),
            Some(TokenKind::KwEnum) => self.parse_type_enum(),
            Some(TokenKind::LParen) => self.parse_type_paren(),
            Some(TokenKind::LBracket) => self.parse_type_array(),
            Some(TokenKind::KwSelfType) => self.parse_self_qualified_path(),
            Some(TokenKind::Ident) => self.parse_type_name(),
            Some(TokenKind::EffectOpen) => self.parse_effect_row(),
            Some(TokenKind::CapOpen) => self.parse_cap_set(),
            _ => self.error_expected_type(),
        }
    }

}
