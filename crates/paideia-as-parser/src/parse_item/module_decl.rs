//! Module / structure / functor / signature parsing.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_module_decl(&mut self) -> Result<NodeId, ParseError> {
        let module_tok = self.expect(TokenKind::KwModule)?;
        let span_start = module_tok.span;

        // Parse module name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional signature ascription
        let sig = if self.eat(TokenKind::Colon) {
            let sig_tok = self.expect(TokenKind::Ident)?;
            Some(self.arena_mut().alloc(NodeKind::Ident, sig_tok.span))
        } else {
            None
        };

        // Expect `=`
        self.expect(TokenKind::Assign)?;

        // Parse module body (Structure or Functor)
        let body = self.parse_module_body()?;

        // Compute span
        let body_span = self.arena().get(body).map(|n| n.span).unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            body_span.byte_start() + body_span.byte_len() - span_start.byte_start(),
        );

        // Allocate module item
        let item = self.arena_mut().alloc_item(
            NodeKind::Module,
            span,
            ItemData::Module {
                name: name_id,
                sig,
                body,
                inner_attrs: vec![],
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse a module body: either `structure { items }` or `functor (params) -> structure { items }`.
    pub(super) fn parse_module_body(&mut self) -> Result<NodeId, ParseError> {
        if self.at(TokenKind::KwFunctor) {
            self.parse_functor()
        } else if self.at(TokenKind::KwStructure) {
            self.parse_structure()
        } else {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 100).expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message("expected `structure` or `functor` for module body")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            Err(ParseError)
        }
    }

    /// Parse a structure: `structure { ItemDecl* }`
    pub(super) fn parse_structure(&mut self) -> Result<NodeId, ParseError> {
        let struct_tok = self.expect(TokenKind::KwStructure)?;
        let span_start = struct_tok.span;

        self.expect(TokenKind::LBrace)?;

        // Parse scope-head inner attributes (#![...])
        let mut inner_attrs = vec![];
        while self.at(TokenKind::Hash) && self.peek_bang_bracket() {
            match self.parse_inner_attribute() {
                Ok(attr) => inner_attrs.push(attr),
                Err(_) => {
                    // Skip the malformed attribute and continue
                    self.recover_to_one_of(&[
                        TokenKind::Hash,
                        TokenKind::KwModule,
                        TokenKind::KwSignature,
                        TokenKind::KwLet,
                        TokenKind::KwEffect,
                        TokenKind::KwCapability,
                        TokenKind::KwStruct,
                        TokenKind::KwEnum,
                        TokenKind::KwUnsafe,
                        TokenKind::RBrace,
                        TokenKind::Eof,
                    ]);
                }
            }
        }

        let mut items = vec![];
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            match self.parse_item() {
                Ok(item_id) => items.push(item_id),
                Err(_) => {
                    self.recover_to_one_of(&[
                        TokenKind::KwModule,
                        TokenKind::KwSignature,
                        TokenKind::KwLet,
                        TokenKind::KwEffect,
                        TokenKind::KwCapability,
                        TokenKind::KwStruct,
                        TokenKind::KwEnum,
                        TokenKind::KwUnsafe,
                        TokenKind::RBrace,
                        TokenKind::Eof,
                    ]);
                }
            }
        }

        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
        );

        let node_id = self.arena_mut().alloc_item(
            NodeKind::Structure,
            span,
            ItemData::Structure {
                items,
                inner_attrs,
                doc: None,
            },
        );
        Ok(node_id)
    }

    /// Parse a functor: `functor (Param: Sig)+ -> structure { ItemDecl* }`
    pub(super) fn parse_functor(&mut self) -> Result<NodeId, ParseError> {
        let functor_tok = self.expect(TokenKind::KwFunctor)?;
        let span_start = functor_tok.span;

        // Parse parameters: (Ident: Ident)+
        let mut params = vec![];
        loop {
            self.expect(TokenKind::LParen)?;

            let param_name_tok = self.expect(TokenKind::Ident)?;
            let param_name_id = self.arena_mut().alloc(NodeKind::Ident, param_name_tok.span);

            self.expect(TokenKind::Colon)?;

            let param_sig_tok = self.expect(TokenKind::Ident)?;
            let param_sig_id = self.arena_mut().alloc(NodeKind::Ident, param_sig_tok.span);

            self.expect(TokenKind::RParen)?;

            let param = self.arena_mut().alloc_item(
                NodeKind::FunctorParam,
                param_name_tok.span,
                ItemData::FunctorParam {
                    name: param_name_id,
                    sig: param_sig_id,
                },
            );
            params.push(param);

            // Check for next parameter or arrow
            if !self.at(TokenKind::LParen) {
                break;
            }
        }

        // Expect `->` and then `structure`
        self.expect(TokenKind::Arrow)?;

        let body = self.parse_structure()?;

        // Compute span
        let body_span = self.arena().get(body).map(|n| n.span).unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            body_span.byte_start() + body_span.byte_len() - span_start.byte_start(),
        );

        let node_id = self.arena_mut().alloc_item(
            NodeKind::Functor,
            span,
            ItemData::Functor {
                params,
                body,
                doc: None,
            },
        );
        Ok(node_id)
    }

    /// Parse a signature declaration: `signature <Ident> = { ... }`
    ///
    /// For phase-1, the body is parsed as a list of items (placeholder).
    pub(super) fn parse_signature_decl(&mut self) -> Result<NodeId, ParseError> {
        let sig_tok = self.expect(TokenKind::KwSignature)?;
        let span_start = sig_tok.span;

        // Parse signature name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Expect `=`
        self.expect(TokenKind::Assign)?;

        // Parse signature body as a structure (placeholder for phase-1)
        let body = self.parse_structure()?;

        // Compute span
        let body_span = self.arena().get(body).map(|n| n.span).unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            body_span.byte_start() + body_span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Signature,
            span,
            ItemData::Signature {
                name: name_id,
                body,
                doc: None,
            },
        );
        Ok(item)
    }

}
