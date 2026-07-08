//! Effect-row and capability-set parsing (`!{ e1, e2, ... }` and `!!{ c1, c2, ... }`).
//! Split out of `parse_type.rs` (2026-07-08).

use paideia_as_ast::{NodeKind, TypeData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(crate) fn parse_effect_row(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let effect_open_tok = self.expect(TokenKind::EffectOpen)?;
        let span_start = effect_open_tok.span;

        // Check for empty effect set
        if self.at(TokenKind::RBrace) {
            let rbrace_tok = self.expect(TokenKind::RBrace)?;
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeEffectRow,
                span,
                TypeData::EffectRow {
                    items: vec![],
                    rest: None,
                },
            ));
        }

        let mut items = Vec::new();

        // Parse comma-separated effect identifiers
        loop {
            if self.at(TokenKind::Ident) {
                let ident_tok = self.bump().unwrap();
                let ident_id = self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
                items.push(ident_id);

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume `,`
            } else {
                break;
            }
        }

        let mut rest = None;

        // Check for pipe tail
        if self.at(TokenKind::Pipe) {
            self.bump(); // consume `|`

            if let Some(tok) = self.peek()
                && tok.kind == TokenKind::Ident
            {
                let rest_tok = self.bump().unwrap();
                let rest_id = self.arena_mut().alloc(NodeKind::Ident, rest_tok.span);
                rest = Some(rest_id);
            }
        }

        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span_end = rbrace_tok.span;

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeEffectRow,
            span,
            TypeData::EffectRow { items, rest },
        ))
    }

    /// Parse a capability set: `@{ cap1, cap2, ... }`.
    ///
    /// Phase-1 representation: each dotted path `Mmio.read_cap` is accumulated
    /// as a sequence of Ident nodes and stored in `TypeData::EffectRow` with
    /// `rest: None` (reusing the effect row variant). A dedicated TypeData
    /// variant for capability sets can be added in a later phase if needed.
    pub(crate) fn parse_cap_set(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let cap_open_tok = self.expect(TokenKind::CapOpen)?;
        let span_start = cap_open_tok.span;

        // Check for empty capability set
        if self.at(TokenKind::RBrace) {
            let rbrace_tok = self.expect(TokenKind::RBrace)?;
            let span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
            );
            return Ok(self.arena_mut().alloc_type(
                NodeKind::TypeEffectRow,
                span,
                TypeData::EffectRow {
                    items: vec![],
                    rest: None,
                },
            ));
        }

        let mut items = Vec::new();

        // Parse comma-separated capability identifiers (with optional dot-separated segments)
        loop {
            if self.at(TokenKind::Ident) {
                let ident_tok = self.bump().unwrap();

                // For phase-1, accumulate a dotted path as separate Ident nodes.
                // E.g., `Mmio.read_cap` becomes two nodes: Mmio, read_cap.
                items.push(self.arena_mut().alloc(NodeKind::Ident, ident_tok.span));

                // Check for dot-separated path continuation
                while self.at(TokenKind::Dot) {
                    self.bump(); // consume `.`

                    if let Some(next_tok) = self.peek() {
                        if next_tok.kind == TokenKind::Ident {
                            let next_ident_tok = self.bump().unwrap();
                            items
                                .push(self.arena_mut().alloc(NodeKind::Ident, next_ident_tok.span));
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume `,`
            } else {
                break;
            }
        }

        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span_end = rbrace_tok.span;

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        // Phase-1: reuse TypeEffectRow for capability sets.
        Ok(self.arena_mut().alloc_type(
            NodeKind::TypeEffectRow,
            span,
            TypeData::EffectRow { items, rest: None },
        ))
    }

}
