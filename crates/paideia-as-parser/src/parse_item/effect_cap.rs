//! Effect and capability declaration parsing.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_effect_decl(&mut self) -> Result<NodeId, ParseError> {
        let effect_tok = self.expect(TokenKind::KwEffect)?;
        let span_start = effect_tok.span;

        // Parse effect name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Expect `{`
        self.expect(TokenKind::LBrace)?;

        // Parse operation signatures
        let mut ops = vec![];
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            // Phase-1: parse any Ident as the "op" keyword contextually
            // Skip the "op" keyword if present
            let _op_or_name_tok = if self.at(TokenKind::Ident) {
                self.bump().expect("at(Ident) implies peek() is Some")
            } else {
                return Err(ParseError);
            };

            // Now parse the operation name (another Ident)
            let op_name_tok = self.expect(TokenKind::Ident)?;
            let op_name_id = self.arena_mut().alloc(NodeKind::Ident, op_name_tok.span);

            // Expect `:`
            self.expect(TokenKind::Colon)?;

            // Parse type
            let ty = self.parse_type()?;

            // Optional effect set: `!{ ... }`
            let effect_set = if self.at(TokenKind::EffectOpen) {
                self.bump();
                // Phase-1: skip contents until closing `}`
                let mut depth = 1;
                while !self.at_eof() && depth > 0 {
                    if self.at(TokenKind::LBrace) {
                        depth += 1;
                    } else if self.at(TokenKind::RBrace) {
                        depth -= 1;
                    }
                    self.bump();
                }
                // For phase-1, allocate a placeholder; later PRs will parse this properly
                Some(
                    self.arena_mut()
                        .alloc(NodeKind::Placeholder, op_name_tok.span),
                )
            } else {
                None
            };

            let op_sig = self.arena_mut().alloc_item(
                NodeKind::OpSig,
                op_name_tok.span,
                ItemData::OpSig {
                    name: op_name_id,
                    ty,
                    effect_set,
                },
            );
            ops.push(op_sig);
        }

        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Effect,
            span,
            ItemData::Effect {
                name: name_id,
                ops,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse a capability declaration: `capability <Ident> { ... }`
    ///
    /// For phase-1, the body is parsed as a skeleton (just match braces).
    pub(super) fn parse_capability_decl(&mut self) -> Result<NodeId, ParseError> {
        let cap_tok = self.expect(TokenKind::KwCapability)?;
        let span_start = cap_tok.span;

        // Parse capability name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Expect `{` and skip to matching `}`
        self.expect(TokenKind::LBrace)?;
        let mut depth = 1;
        while !self.at_eof() && depth > 0 {
            if self.at(TokenKind::LBrace) {
                depth += 1;
            } else if self.at(TokenKind::RBrace) {
                depth -= 1;
            }
            self.bump();
        }

        let rbrace_span = self.peek().map(|t| t.span).unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_span.byte_start() + rbrace_span.byte_len() - span_start.byte_start(),
        );

        // Allocate placeholder body for phase-1
        let body = self.arena_mut().alloc(NodeKind::Placeholder, span);

        let item = self.arena_mut().alloc_item(
            NodeKind::Capability,
            span,
            ItemData::Capability {
                name: name_id,
                body,
                doc: None,
            },
        );
        Ok(item)
    }

}
