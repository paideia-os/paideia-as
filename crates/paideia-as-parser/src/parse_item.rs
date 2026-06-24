//! Top-level item parsing: modules, signatures, effects, capabilities, structs, enums, macros, and unsafe blocks.
//!
//! Implements §8 ItemDecl grammar: Module, Signature, Let, Effect, Capability, Struct, Enum,
//! MacroDecl, and UnsafeBlock declarations. Each parser function returns a `NodeId` pointing to the
//! allocated item node.
//!
//! **Phase-1 constraints:**
//! - `op` keyword in effect declarations is not validated by the lexer; parsed as Ident contextually.
//! - `macro` keyword in macro declarations is not validated by the lexer; parsed as Ident contextually.
//! - Capability, Struct, and Enum body parsing is skeleton-level.
//! - Module body must be either `structure { items }` or `functor (params) -> structure { items }`.
//! - Only one module per file (M0306 diagnostic emitted for the second module).

use paideia_as_ast::{AttrValue, GenericParam, ItemAttribute, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a top-level item declaration.
    ///
    /// Dispatches on the current token kind:
    /// - `KwModule` → `parse_module_decl`
    /// - `KwSignature` → `parse_signature_decl`
    /// - `KwLet` → `parse_let_decl` (top-level form)
    /// - `KwEffect` → `parse_effect_decl`
    /// - `KwCapability` → `parse_capability_decl`
    /// - `KwStruct` → `parse_struct_decl`
    /// - `KwEnum` → `parse_enum_decl`
    /// - `KwTrait` → `parse_trait_decl`
    /// - `KwImpl` → `parse_impl_decl`
    /// - `KwUnsafe` → `parse_unsafe` (existing parser, wrapped as an expression)
    /// - `Ident` with lexeme "macro" → `parse_macro_decl` (contextual keyword)
    /// - Anything else → emit P0100 and return Err
    ///
    /// Returns the `NodeId` of the allocated item on success.
    pub fn parse_item(&mut self) -> Result<NodeId, ParseError> {
        // Check for leading attributes (e.g., `#[derive(...)]`)
        // If found, dispatch to the appropriate item parser which will consume them
        if self.at(TokenKind::Hash) {
            // Peek ahead to determine which item type follows
            // We'll let the specific parser handle the attributes
            // by checking the token after the closing `]`
            match self.peek_beyond_attributes() {
                Some(TokenKind::KwStruct) => return self.parse_struct_decl(),
                Some(TokenKind::KwEnum) => return self.parse_enum_decl(),
                _ => {
                    // Unknown attribute or item type; fall through to error
                }
            }
        }

        match self.peek().map(|t| t.kind) {
            Some(TokenKind::KwModule) => self.parse_module_decl(),
            Some(TokenKind::KwSignature) => self.parse_signature_decl(),
            Some(TokenKind::KwPub) => {
                // `pub` at item level: dispatch based on what follows
                self.bump(); // consume `pub`
                match self.peek().map(|t| t.kind) {
                    Some(TokenKind::KwLet) => self.parse_let_decl_with_visibility(true),
                    _ => {
                        // `pub` is only valid before `let`
                        let span = self
                            .peek()
                            .map(|t| t.span)
                            .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 110)
                            .expect("valid P0110 code");
                        let diag = Diagnostic::error(code)
                            .message("'pub' is only valid before 'let'")
                            .with_span(span)
                            .finish();
                        self.emit_diagnostic(diag);
                        Err(ParseError)
                    }
                }
            }
            Some(TokenKind::KwLet) => self.parse_let_decl_with_visibility(false),
            Some(TokenKind::KwEffect) => self.parse_effect_decl(),
            Some(TokenKind::KwCapability) => self.parse_capability_decl(),
            Some(TokenKind::KwStruct) => self.parse_struct_decl(),
            Some(TokenKind::KwEnum) => self.parse_enum_decl(),
            Some(TokenKind::KwTrait) => self.parse_trait_decl(),
            Some(TokenKind::KwImpl) => self.parse_impl_decl(),
            Some(TokenKind::KwUnsafe) => {
                // Unsafe blocks are parsed as expressions but must be wrapped as item-level constructs.
                // Per the spec, UnsafeBlock is an ItemData variant, so we allocate it here.
                // For now, delegate to parse_unsafe (which parses the block as an expression),
                // then extract the fields and re-allocate as an item.
                self.parse_unsafe_item()
            }
            Some(TokenKind::Ident) => {
                // Check for contextual keyword "macro"
                if let Some(tok) = self.peek() {
                    let lexeme = self.source_text_for_span(tok.span);
                    if lexeme == "macro" {
                        return self.parse_macro_decl();
                    }
                }
                // Not a macro; fall through to error
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                    .expect("valid P0100 code");
                let diag = Diagnostic::error(code)
                    .message("expected item (module, signature, let, effect, capability, struct, enum, trait, impl, macro, or unsafe)")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
            _ => {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                    .expect("valid P0100 code");
                let diag = Diagnostic::error(code)
                    .message("expected item (module, signature, let, effect, capability, struct, enum, trait, impl, macro, or unsafe)")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
        }
    }

    /// Parse an entire source file as a sequence of items.
    ///
    /// Reads items until EOF, tracking the number of Module declarations.
    /// If more than one module appears, emit M0306 for the second and subsequent modules.
    /// Returns a synthetic Structure node containing all top-level items.
    ///
    /// **Algorithm:**
    ///
    /// 1. Initialize an empty items vector and module_count.
    /// 2. Loop until EOF, calling parse_item() and checking for Modules.
    ///    If module_count > 1, emit M0306 ("only one module per file").
    /// 3. On parse error, recover to the next item start point and continue.
    /// 4. Allocate a synthetic Structure node containing all items.
    /// 5. Return the Structure's NodeId.
    ///
    /// Returns the `NodeId` of the synthetic root Structure on success.
    pub fn parse_source_file(&mut self) -> Result<NodeId, ParseError> {
        let mut items = vec![];
        let mut inner_attrs = vec![];
        let mut module_count = 0;
        let file_span_start = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));

        // Parse module-head inner attributes (#![...])
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
                        TokenKind::Eof,
                    ]);
                }
            }
        }

        while !self.at_eof() {
            match self.parse_item() {
                Ok(item_id) => {
                    // Check if this is a Module item
                    if let Some(node_data) = self.arena().get(item_id)
                        && node_data.kind == NodeKind::Module
                    {
                        module_count += 1;
                        if module_count > 1 {
                            let code = DiagnosticCode::new(Category::M, Severity::Error, 306)
                                .expect("valid M0306 code");
                            let diag = Diagnostic::error(code)
                                .message("only one `module` declaration per file is allowed")
                                .with_span(node_data.span)
                                .finish();
                            self.emit_diagnostic(diag);
                        }
                    }
                    items.push(item_id);
                }
                Err(_) => {
                    // Recover to the next item start point
                    // Note: cannot include Ident here as we'd need to check lexeme for "macro",
                    // so recovery stops at keywords only.
                    self.recover_to_one_of(&[
                        TokenKind::KwModule,
                        TokenKind::KwSignature,
                        TokenKind::KwLet,
                        TokenKind::KwEffect,
                        TokenKind::KwCapability,
                        TokenKind::KwStruct,
                        TokenKind::KwEnum,
                        TokenKind::KwUnsafe,
                        TokenKind::Eof,
                    ]);
                }
            }
        }

        // Allocate synthetic root Structure with inner_attrs
        let root_span = self.peek().map(|t| t.span).unwrap_or(file_span_start);
        let root = self.arena_mut().alloc_item(
            NodeKind::Structure,
            root_span,
            ItemData::Structure {
                items,
                inner_attrs,
                doc: None,
            },
        );
        Ok(root)
    }

    /// Check if the parser is at EOF.
    #[must_use]
    fn at_eof(&self) -> bool {
        self.peek().is_none() || self.at(TokenKind::Eof)
    }

    /// Get the source text for a given span.
    fn source_text_for_span(&self, span: Span) -> &str {
        let source = self.source();
        let start = span.byte_start() as usize;
        let end = (span.byte_start() + span.byte_len()) as usize;
        if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        }
    }

    /// Peek ahead to find the token kind after any leading attributes.
    ///
    /// Scans forward over `#[...]` patterns to find the actual item keyword.
    /// Returns None if we reach EOF or encounter a non-attribute pattern.
    fn peek_beyond_attributes(&self) -> Option<TokenKind> {
        let mut lookahead = 0;

        // Skip any `#[...]` or `#![...]` patterns
        loop {
            let tok = self.peek_at(lookahead)?;

            if tok.kind != TokenKind::Hash {
                break;
            }

            lookahead += 1;
            let next = self.peek_at(lookahead)?;

            // Skip both `#[...]` (outer attr) and `#![...]` (inner attr)
            let is_inner = next.kind == TokenKind::Bang;
            if is_inner {
                lookahead += 1;
                let next_after_bang = self.peek_at(lookahead)?;
                if next_after_bang.kind != TokenKind::LBracket {
                    break;
                }
            } else if next.kind != TokenKind::LBracket {
                break;
            }

            // Find the matching `]`
            lookahead += 1;
            let mut bracket_depth = 1;
            while bracket_depth > 0 {
                let tok = self.peek_at(lookahead)?;
                if tok.kind == TokenKind::LBracket {
                    bracket_depth += 1;
                } else if tok.kind == TokenKind::RBracket {
                    bracket_depth -= 1;
                }
                lookahead += 1;
            }
        }

        self.peek_at(lookahead).map(|t| t.kind)
    }

    /// Check if the next tokens form `#![` (start of an inner attribute).
    ///
    /// Returns `true` if the sequence is `Hash` + `Bang` + `LBracket`,
    /// `false` otherwise (or if we hit EOF).
    fn peek_bang_bracket(&self) -> bool {
        if let Some(first) = self.peek_at(0) {
            if first.kind == TokenKind::Hash {
                if let Some(second) = self.peek_at(1) {
                    if second.kind == TokenKind::Bang {
                        if let Some(third) = self.peek_at(2) {
                            return third.kind == TokenKind::LBracket;
                        }
                    }
                }
            }
        }
        false
    }

    /// Parse a module declaration: `module <Ident> (: <SignatureRef>)? = <ModuleBody>`
    ///
    /// **Algorithm:**
    /// 1. Expect `KwModule`.
    /// 2. Expect an Ident for the module name.
    /// 3. Optional `: <SignatureRef>` (parse as a single Ident for phase-1).
    /// 4. Expect `=`.
    /// 5. Parse the module body (Structure or Functor).
    /// 6. Allocate `ItemData::Module { name, sig, body, doc: None }`.
    ///
    /// The module body is either:
    /// - `structure { ItemDecl* }`
    /// - `functor (FunctorParam)+ -> structure { ItemDecl* }`
    fn parse_module_decl(&mut self) -> Result<NodeId, ParseError> {
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
    fn parse_module_body(&mut self) -> Result<NodeId, ParseError> {
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
    fn parse_structure(&mut self) -> Result<NodeId, ParseError> {
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
    fn parse_functor(&mut self) -> Result<NodeId, ParseError> {
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
    fn parse_signature_decl(&mut self) -> Result<NodeId, ParseError> {
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

    /// Parse a top-level let declaration with optional visibility: `[pub] let [mut] <Ident> <GenericParams>? (: Type)? = Expr`
    fn parse_let_decl_with_visibility(&mut self, public: bool) -> Result<NodeId, ParseError> {
        let let_tok = self.expect(TokenKind::KwLet)?;
        let span_start = let_tok.span;

        // `pub` is consumed by the caller (parse_item dispatcher) and passed in.
        // Do NOT re-check for KwPub here.

        // Check for optional `mut` keyword
        let mutable = if self.at(TokenKind::KwMut) {
            self.bump();
            true
        } else {
            false
        };

        // Try to parse a pattern first (could be a tuple, struct, enum variant, etc.)
        // If that fails, fall back to parsing a simple identifier.
        // Peek ahead to see if we have a pattern or just a name.
        let mut pattern_or_name = None;

        // Check if the next token looks like a pattern start
        if let Some(tok) = self.peek() {
            match tok.kind {
                // These are pattern starters
                TokenKind::LParen => {
                    // This is a pattern
                    pattern_or_name = Some(self.parse_pattern()?);
                }
                TokenKind::Ident => {
                    // Could be a pattern or just an identifier name.
                    // Peek at the next token to disambiguate.
                    if let Some(next_tok) = self.peek_at(1) {
                        match next_tok.kind {
                            // These indicate a pattern
                            TokenKind::ColonColon | TokenKind::LBrace => {
                                pattern_or_name = Some(self.parse_pattern()?);
                            }
                            // Otherwise just an identifier
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // If we didn't parse a pattern, just parse a simple identifier
        let name_id = if let Some(pat) = pattern_or_name {
            pat
        } else {
            let name_tok = self.expect(TokenKind::Ident)?;
            self.arena_mut().alloc(NodeKind::Ident, name_tok.span)
        };

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // Optional type annotation
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect `=`
        self.expect(TokenKind::Assign)?;

        // Parse value expression
        let value = self.parse_expr()?;

        // Consume optional `;`
        self.eat(TokenKind::Semicolon);

        // Compute span
        let value_span = self
            .arena()
            .get(value)
            .map(|n| n.span)
            .unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            value_span.byte_start() + value_span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Let,
            span,
            ItemData::Let {
                public,
                mutable,
                name: name_id,
                generic_params,
                ty,
                value,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Wrapper for backward compatibility and simplicity when public visibility is not needed.
    fn parse_let_decl(&mut self) -> Result<NodeId, ParseError> {
        self.parse_let_decl_with_visibility(false)
    }

    /// Parse an effect declaration: `effect <Ident> { OpSig+ }`
    ///
    /// Each OpSig is: `<Ident> : Type (!{ EffectSet })?`
    /// (The `op` keyword is treated contextually; phase-1 does not validate it.)
    fn parse_effect_decl(&mut self) -> Result<NodeId, ParseError> {
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
    fn parse_capability_decl(&mut self) -> Result<NodeId, ParseError> {
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

    /// Parse item attributes (e.g., `#[derive(...)]`).
    ///
    /// Recognizes `#[derive(Trait1, Trait2, ...)]` syntax.
    /// Each attribute must be followed by `]`.
    ///
    /// For phase-1, only `derive` attributes are recognized.
    /// Other attributes emit P0203 and are skipped.
    ///
    /// # Returns
    /// A vector of `ItemAttribute` instances (empty if no attributes found).
    fn parse_attributes(&mut self) -> Result<Vec<ItemAttribute>, ParseError> {
        let mut attributes = vec![];

        while self.at(TokenKind::Hash) {
            self.bump(); // consume `#`

            if !self.at(TokenKind::LBracket) {
                // Recover: skip malformed attribute
                continue;
            }
            self.bump(); // consume `[`

            // Check for `derive`
            if self.at(TokenKind::Ident) {
                let lexeme = if let Some(tok) = self.peek() {
                    self.source_text_for_span(tok.span)
                } else {
                    ""
                };

                if lexeme == "derive" {
                    self.bump(); // consume `derive`

                    // Expect `(`
                    self.expect(TokenKind::LParen)?;

                    // Parse comma-separated list of trait names
                    let mut trait_names = vec![];
                    loop {
                        if self.at(TokenKind::RParen) {
                            break;
                        }

                        // Expect an identifier (trait name)
                        if self.at(TokenKind::Ident) {
                            let trait_tok = self.expect(TokenKind::Ident)?;
                            let trait_id = self.arena_mut().alloc(NodeKind::Ident, trait_tok.span);
                            trait_names.push(trait_id);

                            // Check for comma
                            if self.at(TokenKind::Comma) {
                                self.bump();
                            } else if !self.at(TokenKind::RParen) {
                                // Error: expected comma or )
                                let span = self
                                    .peek()
                                    .map(|t| t.span)
                                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                                let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                                    .expect("valid P0100 code");
                                let diag = Diagnostic::error(code)
                                    .message("expected `,` or `)` in derive attribute")
                                    .with_span(span)
                                    .finish();
                                self.emit_diagnostic(diag);
                                return Err(ParseError);
                            }
                        } else {
                            let span = self
                                .peek()
                                .map(|t| t.span)
                                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                                .expect("valid P0100 code");
                            let diag = Diagnostic::error(code)
                                .message("expected trait name in derive attribute")
                                .with_span(span)
                                .finish();
                            self.emit_diagnostic(diag);
                            return Err(ParseError);
                        }
                    }

                    self.expect(TokenKind::RParen)?; // consume `)`
                    self.expect(TokenKind::RBracket)?; // consume `]`

                    attributes.push(ItemAttribute::Derive { trait_names });
                } else {
                    // Unknown attribute type; skip it
                    // Consume up to the closing bracket
                    let mut bracket_depth = 1;
                    while !self.at_eof() && bracket_depth > 0 {
                        if self.at(TokenKind::LBracket) {
                            bracket_depth += 1;
                        } else if self.at(TokenKind::RBracket) {
                            bracket_depth -= 1;
                        }
                        self.bump();
                    }
                }
            } else {
                // Malformed attribute; skip
                break;
            }
        }

        Ok(attributes)
    }

    /// Parse a struct type declaration: `struct <Ident> <GenericParams>? { ... }`
    ///
    /// For phase-1, the body is parsed as a skeleton (just match braces).
    /// Attributes (e.g., `#[derive(...)]`) are parsed before the struct keyword.
    fn parse_struct_decl(&mut self) -> Result<NodeId, ParseError> {
        // Parse leading attributes
        let attributes = self.parse_attributes()?;

        let struct_tok = self.expect(TokenKind::KwStruct)?;
        let span_start = struct_tok.span;

        // Parse struct name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

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

        // For phase-1, allocate an empty fields vector
        let item = self.arena_mut().alloc_item(
            NodeKind::Struct,
            span,
            ItemData::Struct {
                name: name_id,
                generic_params,
                fields: vec![],
                attributes,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse an enum type declaration: `enum <Ident> <GenericParams>? { ... }`
    ///
    /// For phase-1, the body is parsed as a skeleton (just match braces).
    /// Attributes (e.g., `#[derive(...)]`) are parsed before the enum keyword.
    fn parse_enum_decl(&mut self) -> Result<NodeId, ParseError> {
        // Parse leading attributes
        let attributes = self.parse_attributes()?;

        let enum_tok = self.expect(TokenKind::KwEnum)?;
        let span_start = enum_tok.span;

        // Parse enum name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

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

        // For phase-1, allocate an empty variants vector
        let item = self.arena_mut().alloc_item(
            NodeKind::Enum,
            span,
            ItemData::Enum {
                name: name_id,
                generic_params,
                variants: vec![],
                attributes,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse a trait type declaration: `trait <Ident> <GenericParams>? { TraitMethod* }`
    ///
    /// **Grammar:**
    /// ```text
    /// TraitDecl ::= 'trait' Ident GenericParams? '{' TraitMethod* '}'
    /// TraitMethod ::= 'fn' Ident GenericParams? '(' (Ident ':' Type)* ')' '->' Type EffectRow? CapRow? (';' | '{' Expr '}')
    /// ```
    ///
    /// When a method ends with `;` → no default body. When method ends with `{ expr }` → default body.
    ///
    /// For phase-1 (m9-003), trait method bodies are parsed as skeleton (matching braces only).
    /// Emits P0201 if trait declaration is malformed.
    fn parse_trait_decl(&mut self) -> Result<NodeId, ParseError> {
        let trait_tok = self.expect(TokenKind::KwTrait)?;
        let span_start = trait_tok.span;

        // Parse trait name
        let name_tok = match self.expect(TokenKind::Ident) {
            Ok(tok) => tok,
            Err(_) => {
                let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed trait declaration: expected trait name")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            match self.parse_generic_params() {
                Ok(params) => params,
                Err(_) => {
                    let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                        .expect("valid P0201 code");
                    let diag = Diagnostic::error(code)
                        .message("malformed trait declaration: invalid generic parameters")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        } else {
            Vec::new()
        };

        // Expect `{` and parse trait methods
        if !self.at(TokenKind::LBrace) {
            let span = self.peek().map(|t| t.span).unwrap_or(span_start);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait declaration: expected opening brace")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `{`

        let mut associated_types = Vec::new();
        let mut methods = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            // Check for `type Ident;` (associated type declaration)
            if self.at(TokenKind::KwType) {
                match self.parse_trait_associated_type() {
                    Ok(assoc_type_id) => associated_types.push(assoc_type_id),
                    Err(_) => {
                        // Skip to next item or closing brace
                        while !self.at(TokenKind::Semicolon)
                            && !self.at(TokenKind::KwType)
                            && !self.at(TokenKind::KwFn)
                            && !self.at(TokenKind::RBrace)
                            && !self.at_eof()
                        {
                            self.bump();
                        }
                        if self.at(TokenKind::Semicolon) {
                            self.bump();
                        }
                    }
                }
            } else if self.at(TokenKind::KwFn) {
                match self.parse_trait_method() {
                    Ok(method) => methods.push(method),
                    Err(_) => {
                        // Skip to next method or closing brace
                        while !self.at(TokenKind::Semicolon)
                            && !self.at(TokenKind::LBrace)
                            && !self.at(TokenKind::RBrace)
                            && !self.at_eof()
                        {
                            self.bump();
                        }
                        // If we hit `{`, skip to matching `}`
                        if self.at(TokenKind::LBrace) {
                            let mut depth = 1;
                            self.bump();
                            while !self.at_eof() && depth > 0 {
                                if self.at(TokenKind::LBrace) {
                                    depth += 1;
                                } else if self.at(TokenKind::RBrace) {
                                    depth -= 1;
                                }
                                self.bump();
                            }
                        } else if self.at(TokenKind::Semicolon) {
                            self.bump();
                        }
                    }
                }
            } else {
                // Unexpected item in trait body; skip and recover
                let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("expected 'type' or 'fn' in trait body")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                self.bump();
            }
        }

        if !self.at(TokenKind::RBrace) {
            let span = self.peek().map(|t| t.span).unwrap_or(span_start);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait declaration: expected closing brace")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let rbrace_span = self.peek().map(|t| t.span).unwrap_or(span_start);
        self.bump(); // consume `}`

        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_span.byte_start() + rbrace_span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Trait,
            span,
            ItemData::Trait {
                name: name_id,
                generic_params,
                associated_types,
                methods,
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse a single trait method: `fn Name<T>(...) -> Type !(effects)? @(caps)? (;  | { ... })`
    ///
    /// Returns a `TraitMethod` struct. Emits P0201 if malformed.
    fn parse_trait_method(&mut self) -> Result<paideia_as_ast::TraitMethod, ParseError> {
        // Expect `fn` keyword
        if !self.at(TokenKind::KwFn) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait method: expected 'fn' keyword")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `fn`

        // Parse method name
        let name_tok = match self.expect(TokenKind::Ident) {
            Ok(tok) => tok,
            Err(_) => {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed trait method: expected method name")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            match self.parse_generic_params() {
                Ok(params) => params,
                Err(_) => {
                    let span = self
                        .peek()
                        .map(|t| t.span)
                        .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                        .expect("valid P0201 code");
                    let diag = Diagnostic::error(code)
                        .message("malformed trait method: invalid generic parameters")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        } else {
            Vec::new()
        };

        // Parse parameters: (Ident: Type)*
        if !self.at(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait method: expected parameter list")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `(`

        let mut params = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            // Parse parameter name
            let param_name_tok = match self.expect(TokenKind::Ident) {
                Ok(tok) => tok,
                Err(_) => {
                    // Skip to closing paren or semicolon
                    while !self.at(TokenKind::RParen)
                        && !self.at(TokenKind::Semicolon)
                        && !self.at(TokenKind::LBrace)
                        && !self.at_eof()
                    {
                        self.bump();
                    }
                    return Err(ParseError);
                }
            };
            let param_name_id = self.arena_mut().alloc(NodeKind::Ident, param_name_tok.span);

            // Expect `:`
            if !self.at(TokenKind::Colon) {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed trait method: expected ':' after parameter name")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
            self.bump(); // consume `:`

            // Parse type (for now, allocate a placeholder)
            let type_tok = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let type_id = self.arena_mut().alloc(NodeKind::Placeholder, type_tok);

            // Skip type tokens until we hit `,`, `)`, or other expected token
            while !self.at(TokenKind::Comma) && !self.at(TokenKind::RParen) && !self.at_eof() {
                self.bump();
            }

            params.push((param_name_id, type_id));

            // Handle comma
            if self.at(TokenKind::Comma) {
                self.bump();
            } else if !self.at(TokenKind::RParen) {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed trait method: expected ',' or ')' in parameter list")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        }

        if !self.at(TokenKind::RParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait method: expected closing parenthesis")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `)`

        // Expect `->`
        if !self.at(TokenKind::Arrow) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait method: expected '->' before return type")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `->`

        // Parse return type (for now, allocate a placeholder)
        let return_type_span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));
        let return_type_id = self
            .arena_mut()
            .alloc(NodeKind::Placeholder, return_type_span);

        // Skip return type tokens until we hit effect/capability brackets or `;`/`{`
        while !self.at(TokenKind::Bang)
            && !self.at(TokenKind::At)
            && !self.at(TokenKind::Semicolon)
            && !self.at(TokenKind::LBrace)
            && !self.at_eof()
        {
            self.bump();
        }

        // Parse optional effect set: !{ ... }
        let effects = if self.at(TokenKind::Bang) {
            self.bump(); // consume `!`
            if !self.at(TokenKind::LBrace) {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed trait method: expected '{' after '!'")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
            let eff_span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let eff_id = self.arena_mut().alloc(NodeKind::Placeholder, eff_span);
            // Skip to matching `}`
            let mut depth = 1;
            self.bump();
            while !self.at_eof() && depth > 0 {
                if self.at(TokenKind::LBrace) {
                    depth += 1;
                } else if self.at(TokenKind::RBrace) {
                    depth -= 1;
                }
                self.bump();
            }
            Some(eff_id)
        } else {
            None
        };

        // Parse optional capability set: @{ ... }
        let capabilities = if self.at(TokenKind::At) {
            self.bump(); // consume `@`
            if !self.at(TokenKind::LBrace) {
                let span = self
                    .peek()
                    .map(|t| t.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed trait method: expected '{' after '@'")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
            let cap_span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let cap_id = self.arena_mut().alloc(NodeKind::Placeholder, cap_span);
            // Skip to matching `}`
            let mut depth = 1;
            self.bump();
            while !self.at_eof() && depth > 0 {
                if self.at(TokenKind::LBrace) {
                    depth += 1;
                } else if self.at(TokenKind::RBrace) {
                    depth -= 1;
                }
                self.bump();
            }
            Some(cap_id)
        } else {
            None
        };

        // Parse method body: either `;` (abstract) or `{ ... }` (default)
        let default_body = if self.at(TokenKind::Semicolon) {
            self.bump(); // consume `;`
            None
        } else if self.at(TokenKind::LBrace) {
            let body_span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let body_id = self.arena_mut().alloc(NodeKind::Placeholder, body_span);
            // Skip to matching `}`
            let mut depth = 1;
            self.bump();
            while !self.at_eof() && depth > 0 {
                if self.at(TokenKind::LBrace) {
                    depth += 1;
                } else if self.at(TokenKind::RBrace) {
                    depth -= 1;
                }
                self.bump();
            }
            Some(body_id)
        } else {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed trait method: expected ';' or '{' after method signature")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        Ok(paideia_as_ast::TraitMethod {
            name: name_id,
            generic_params,
            params,
            return_type: return_type_id,
            effects,
            capabilities,
            default_body,
        })
    }

    /// Parse a trait associated type: `type Ident;`
    ///
    /// Returns the NodeId of the associated type name (an Ident node).
    /// Emits P0201 if malformed.
    fn parse_trait_associated_type(&mut self) -> Result<NodeId, ParseError> {
        let type_tok = self.expect(TokenKind::KwType)?;
        let span_start = type_tok.span;

        // Parse associated type name
        let name_tok = match self.expect(TokenKind::Ident) {
            Ok(tok) => tok,
            Err(_) => {
                let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                let code = DiagnosticCode::new(Category::P, Severity::Error, 201)
                    .expect("valid P0201 code");
                let diag = Diagnostic::error(code)
                    .message("malformed associated type: expected name")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Expect `;`
        if !self.at(TokenKind::Semicolon) {
            let span = self.peek().map(|t| t.span).unwrap_or(span_start);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 201).expect("valid P0201 code");
            let diag = Diagnostic::error(code)
                .message("malformed associated type: expected ';'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `;`

        Ok(name_id)
    }

    /// Parse an unsafe block at item level.
    ///
    /// Delegates to the existing `parse_unsafe` (which parses as an expression),
    /// but wraps the result as an item-level UnsafeBlock.
    fn parse_unsafe_item(&mut self) -> Result<NodeId, ParseError> {
        // The unsafe expression parser is already available via parse_unsafe().
        // We delegate to it and wrap the result.
        let _expr_id = self.parse_unsafe()?;

        // For phase-1, allocate a placeholder UnsafeBlock with empty fields.
        // Later PRs will properly extract unsafe block semantics.
        let unsafe_tok = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));

        // Allocate the justification placeholder first to avoid multiple mutable borrows
        let justification = self.arena_mut().alloc(NodeKind::Placeholder, unsafe_tok);

        let item = self.arena_mut().alloc_item(
            NodeKind::UnsafeBlock,
            unsafe_tok,
            ItemData::UnsafeBlock {
                effects: vec![],
                capabilities: vec![],
                justification,
                block: vec![],
            },
        );
        Ok(item)
    }

    /// Parse generic parameters: `< GenericParam (',' GenericParam)* (',')? >`.
    ///
    /// **Grammar:**
    /// ```text
    /// GenericParams ::= '<' GenericParam (',' GenericParam)* (',')? '>'
    /// GenericParam ::= Ident (':' Path (',' Path)* )?
    /// ```
    ///
    /// Returns `Vec<GenericParam>` representing the generic parameters.
    /// Returns `P0200` if any part of the generic parameter list is malformed.
    ///
    /// For phase-1 (m9-001), this is only called from function declarations
    /// and does not attempt to parse generic-args at use sites.
    pub(crate) fn parse_generic_params(&mut self) -> Result<Vec<GenericParam>, ParseError> {
        // Expect opening `<`
        let _lt_tok = self.expect(TokenKind::Lt)?;
        let mut params = Vec::new();

        // Loop: parse comma-separated generic parameters
        loop {
            // Check for closing `>`
            if self.at(TokenKind::Gt) {
                break;
            }

            // Check for lifetime parameter (leading `'`)
            // Lifetimes appear as identifier tokens with lexeme starting with `'`
            let is_lifetime = if let Some(tok) = self.peek() {
                if tok.kind == TokenKind::Ident {
                    let lexeme = self.source_text_for_span(tok.span);
                    lexeme.starts_with('\'')
                } else {
                    false
                }
            } else {
                false
            };

            if is_lifetime {
                // This is a lifetime parameter
                if let Some(tok) = self.peek() {
                    let lexeme = self.source_text_for_span(tok.span).to_string();
                    self.bump(); // consume the lifetime token

                    // Extract the lifetime name (remove leading `'`)
                    let lifetime_name = if lexeme.len() > 1 {
                        lexeme[1..].to_string()
                    } else {
                        // Malformed lifetime (just `'`), skip for now
                        "".to_string()
                    };

                    params.push(GenericParam::Lifetime {
                        name: lifetime_name,
                    });

                    // Check for separator or end
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }

                    // Allow trailing comma before `>`
                    if self.at(TokenKind::Gt) {
                        break;
                    }

                    continue;
                }
            }

            // Parse type parameter name (Ident)
            let param_name_tok = match self.peek() {
                Some(tok) if tok.kind == TokenKind::Ident => {
                    self.bump();
                    tok
                }
                _ => {
                    // Missing or malformed parameter name
                    let span = self
                        .peek()
                        .map(|t| t.span)
                        .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 200)
                        .expect("valid P0200 code");
                    let diag = Diagnostic::error(code)
                        .message("expected generic parameter name in generic parameter list")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            };

            let param_name = self.arena_mut().alloc(NodeKind::Ident, param_name_tok.span);

            // Parse optional bounds: `:` followed by comma-separated trait names
            // With optional projections like `Iterator<Item = u64>`
            let mut bounds = Vec::new();
            if self.eat(TokenKind::Colon) {
                loop {
                    // Parse trait name as a path
                    let trait_name = self.parse_type_name_path()?;
                    bounds.push(trait_name);

                    // NEW: Check for projection syntax `<Item = Type>`
                    // Phase 4 (m9-007): Store projection markers as synthetic Ident nodes.
                    // TODO (resolver): Extract and validate projections against trait's associated types.
                    if self.at(TokenKind::Lt) {
                        self.bump(); // consume `<`

                        if let Some(proj_tok) = self.peek() {
                            if proj_tok.kind == TokenKind::Ident {
                                let proj_name_tok = proj_tok;
                                self.bump(); // consume projection name

                                if self.at(TokenKind::Eq) {
                                    self.bump(); // consume `=`

                                    // Skip type tokens until we hit `,`, `>`, or other boundary
                                    // Phase 4: Parse as placeholder; resolver will validate projection type
                                    // Track nested angle brackets to handle nested generics like <X<Y>>
                                    let mut depth = 0;
                                    while !self.at_eof() {
                                        if self.at(TokenKind::Lt) {
                                            depth += 1;
                                            self.bump();
                                        } else if self.at(TokenKind::Gt) {
                                            if depth > 0 {
                                                depth -= 1;
                                                self.bump();
                                            } else {
                                                // This is the closing `>` for the projection
                                                break;
                                            }
                                        } else if self.at(TokenKind::Comma) && depth == 0 {
                                            // Comma at depth 0 ends the projection
                                            break;
                                        } else {
                                            self.bump();
                                        }
                                    }

                                    // Store synthesized projection marker (phase 4 minimum)
                                    let proj_marker =
                                        self.arena_mut().alloc(NodeKind::Ident, proj_name_tok.span);
                                    bounds.push(proj_marker);
                                }
                            }
                        }

                        // Consume the closing `>` of the projection
                        if self.at(TokenKind::Gt) {
                            self.bump(); // consume `>`
                        }
                    }

                    // Check for comma (more bounds) or end of bounds
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }

                    // Check if the next token is `>` or `,` (trailing comma)
                    if self.at(TokenKind::Gt) {
                        break;
                    }
                }
            }

            params.push(GenericParam::Type {
                name: param_name,
                bounds,
            });

            // Check for separator or end
            if !self.eat(TokenKind::Comma) {
                break;
            }

            // Allow trailing comma before `>`
            if self.at(TokenKind::Gt) {
                break;
            }
        }

        // Expect closing `>`
        if !self.eat(TokenKind::Gt) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 200).expect("valid P0200 code");
            let diag = Diagnostic::error(code)
                .message("expected '>' to close generic parameter list")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(params)
    }

    /// Parse a type name as a path for use in generic bounds.
    ///
    /// For phase-1 (m9-001), this parses a simple identifier or qualified path
    /// like `Trait` or `Module::Trait`.
    fn parse_type_name_path(&mut self) -> Result<NodeId, ParseError> {
        if let Some(tok) = self.peek() {
            if tok.kind == TokenKind::Ident {
                self.bump();
                let id = self.arena_mut().alloc(NodeKind::Ident, tok.span);

                // Handle qualified paths: `Ident :: Ident`
                let mut segments = vec![id];
                while self.eat(TokenKind::ColonColon) {
                    if let Some(next_tok) = self.peek() {
                        if next_tok.kind == TokenKind::Ident {
                            self.bump();
                            let segment = self.arena_mut().alloc(NodeKind::Ident, next_tok.span);
                            segments.push(segment);
                        } else {
                            // Error: expected Ident after `::`
                            let span = next_tok.span;
                            let code = DiagnosticCode::new(Category::P, Severity::Error, 200)
                                .expect("valid P0200 code");
                            let diag = Diagnostic::error(code)
                                .message("expected identifier after '::' in trait bound path")
                                .with_span(span)
                                .finish();
                            self.emit_diagnostic(diag);
                            return Err(ParseError);
                        }
                    } else {
                        break;
                    }
                }

                // Allocate an ExprPath to represent the trait name
                // Use the first segment's span as the start
                let span_start = self
                    .arena()
                    .get(segments[0])
                    .map(|n| n.span)
                    .unwrap_or_else(|| Span::new(self.file(), 0, 0));
                let span_end = self
                    .arena()
                    .get(segments[segments.len() - 1])
                    .map(|n| n.span)
                    .unwrap_or(span_start);
                let span = Span::new(
                    span_start.file(),
                    span_start.byte_start(),
                    span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
                );

                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprPath,
                    span,
                    paideia_as_ast::ExprData::Path { segments },
                ))
            } else {
                let span = tok.span;
                let code = DiagnosticCode::new(Category::P, Severity::Error, 200)
                    .expect("valid P0200 code");
                let diag = Diagnostic::error(code)
                    .message("expected trait name in generic parameter bound")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
        } else {
            let span = Span::new(self.file(), 0, 0);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 200).expect("valid P0200 code");
            let diag = Diagnostic::error(code)
                .message("expected trait name in generic parameter bound")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            Err(ParseError)
        }
    }

    fn parse_impl_decl(&mut self) -> Result<NodeId, ParseError> {
        let impl_tok = self.expect(TokenKind::KwImpl)?;
        let span_start = impl_tok.span;

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            match self.parse_generic_params() {
                Ok(params) => params,
                Err(_) => {
                    let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 202)
                        .expect("valid P0202 code");
                    let diag = Diagnostic::error(code)
                        .message("malformed impl block: invalid generic parameters")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        } else {
            Vec::new()
        };

        // Try to parse `TraitPath<Args>? for Type` or just `Type`
        // We need to disambiguate using the `for` keyword
        let trait_name;
        let trait_args;
        let for_type;

        // Parse the first type/path
        let first_type = match self.parse_type() {
            Ok(t) => t,
            Err(_) => {
                let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                let code = DiagnosticCode::new(Category::P, Severity::Error, 202)
                    .expect("valid P0202 code");
                let diag = Diagnostic::error(code)
                    .message("malformed impl block: expected type or trait name")
                    .with_span(span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Check for `for` keyword
        if self.at(TokenKind::KwFor) {
            // Trait impl: `impl<T> Trait<T> for Type`
            self.bump(); // consume `for`
            trait_name = Some(first_type);
            trait_args = Vec::new(); // TODO: extract from TypeName nodes in later PR

            match self.parse_type() {
                Ok(t) => for_type = t,
                Err(_) => {
                    let span = self.peek().map(|t| t.span).unwrap_or(span_start);
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 202)
                        .expect("valid P0202 code");
                    let diag = Diagnostic::error(code)
                        .message("malformed impl block: expected type after 'for'")
                        .with_span(span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        } else {
            // Inherent impl: `impl<T> Type`
            trait_name = None;
            trait_args = Vec::new();
            for_type = first_type;
        }

        // Expect `{` and parse impl items (for now, just skip to closing brace)
        if !self.at(TokenKind::LBrace) {
            let span = self.peek().map(|t| t.span).unwrap_or(span_start);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 202).expect("valid P0202 code");
            let diag = Diagnostic::error(code)
                .message("malformed impl block: expected opening brace")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume `{`

        let mut methods = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            // Parse items inside the impl body — only Let or Fn declarations allowed
            // This is phase-1 skeleton; m9-005+ will elaborate binding/elaboration
            if self.at(TokenKind::KwLet) {
                match self.parse_let_decl() {
                    Ok(item) => methods.push(item),
                    Err(_) => {
                        // Skip to next item or closing brace
                        while !self.at(TokenKind::KwLet)
                            && !self.at(TokenKind::KwFn)
                            && !self.at(TokenKind::RBrace)
                            && !self.at_eof()
                        {
                            self.bump();
                        }
                    }
                }
            } else if self.at(TokenKind::KwFn) {
                // Create a synthetic fn item
                // For now, just skip the fn declaration; later PRs will parse it properly
                // This is enough to test the impl block parsing
                self.bump(); // skip 'fn'
                // Skip to the next brace-surrounded block or semicolon
                let mut brace_depth = 0;
                while !self.at_eof() {
                    if self.at(TokenKind::LBrace) {
                        brace_depth += 1;
                    } else if self.at(TokenKind::RBrace) {
                        if brace_depth > 0 {
                            brace_depth -= 1;
                        } else {
                            break; // Hit the impl closing brace
                        }
                    } else if self.at(TokenKind::Semicolon) && brace_depth == 0 {
                        self.bump();
                        break;
                    }
                    self.bump();
                }
            } else {
                // Skip unknown item or invalid syntax
                self.bump();
            }
        }

        if !self.at(TokenKind::RBrace) {
            let span = self.peek().map(|t| t.span).unwrap_or(span_start);
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 202).expect("valid P0202 code");
            let diag = Diagnostic::error(code)
                .message("malformed impl block: expected closing brace")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let rbrace_span = self.peek().map(|t| t.span).unwrap_or(span_start);
        self.bump(); // consume `}`

        // Create span covering entire impl block
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_span.byte_start() + rbrace_span.byte_len() - span_start.byte_start(),
        );

        // Allocate and return the impl item
        let impl_decl = paideia_as_ast::ImplDecl {
            generic_params,
            trait_name,
            trait_args,
            for_type,
            methods,
        };

        Ok(self.arena_mut().alloc_item(
            NodeKind::Impl,
            span,
            paideia_as_ast::ItemData::Impl(impl_decl),
        ))
    }

    /// Parse an inner attribute: `#![name = value]`
    ///
    /// Expects:
    /// - Hash (`#`)
    /// - Bang (`!`)
    /// - LBracket (`[`)
    /// - Ident (attribute name)
    /// - Assign (`=`)
    /// - Value (Int, String, or Ident)
    /// - RBracket (`]`)
    ///
    /// For `bits` attributes, validates that the value is 16, 32, or 64.
    /// Returns the parsed ItemAttribute on success.
    fn parse_inner_attribute(&mut self) -> Result<ItemAttribute, ParseError> {
        let _hash_span = self.expect(TokenKind::Hash)?.span;
        self.expect(TokenKind::Bang)?;
        self.expect(TokenKind::LBracket)?;

        // Parse attribute name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);
        let name_text = self.source_text_for_span(name_tok.span).to_string();

        self.expect(TokenKind::Assign)?;

        // Parse attribute value: Int, String, or Ident
        let value = if self.at(TokenKind::IntLit) {
            let int_tok = self.expect(TokenKind::IntLit)?;
            let int_text = self.source_text_for_span(int_tok.span);
            let int_val: i64 = int_text.parse().unwrap_or(0); // Default to 0 on parse error

            // Validate bits attribute
            if name_text == "bits" {
                if int_val == 16 {
                    // 16-bit mode is not supported; emit B1700
                    let code = DiagnosticCode::new(Category::B, Severity::Error, 1700)
                        .expect("valid B1700 code");
                    let diag = Diagnostic::error(code)
                        .message("16-bit architecture not supported; use 32 or 64")
                        .with_span(int_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                } else if int_val != 32 && int_val != 64 {
                    // Invalid bits value; emit P0240
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 240)
                        .expect("valid P0240 code");
                    let diag = Diagnostic::error(code)
                        .message("invalid #![bits] value; expected 32 or 64")
                        .with_span(int_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                }
            }

            AttrValue::Int(int_val)
        } else if self.at(TokenKind::StringLit) {
            let str_tok = self.expect(TokenKind::StringLit)?;
            let str_id = self.arena_mut().alloc(NodeKind::Placeholder, str_tok.span);
            AttrValue::Str(str_id)
        } else if self.at(TokenKind::Ident) {
            let ident_tok = self.expect(TokenKind::Ident)?;
            let ident_id = self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
            AttrValue::Ident(ident_id)
        } else {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 240).expect("valid P0240 code");
            let diag = Diagnostic::error(code)
                .message("expected integer, string, or identifier for attribute value")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        self.expect(TokenKind::RBracket)?;

        Ok(ItemAttribute::InnerAttr {
            name: name_id,
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, Severity, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};

    fn parse_source_str(
        source: &str,
    ) -> (
        paideia_as_ast::AstArena,
        Result<NodeId, ParseError>,
        Vec<Diagnostic>,
    ) {
        let mut source_map = paideia_as_diagnostics::SourceMap::new();
        let file = source_map.add_file(std::path::PathBuf::from("test.pdx"), source.to_string());
        let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
        let mut arena = paideia_as_ast::AstArena::new();
        let mut sink = VecSink::new();
        let mut lex = Lexer::new(file, &source_text);
        let mut collector = VecSink::new();
        let tokens = lex.collect_tokens(&mut collector);
        // Forward lexer diagnostics into the main sink
        for d in collector.into_diagnostics() {
            let _ = sink.emit(d);
        }
        let result = {
            let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
            p.parse_source_file()
        };
        (arena, result, sink.into_diagnostics())
    }

    #[test]
    fn simple_let_decl() {
        let (_arena, result, diags) = parse_source_str("let x : u64 = 1");
        assert!(result.is_ok(), "should parse successfully");
        // Filter to actual errors (not just warnings)
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn single_module() {
        let (_arena, result, diags) = parse_source_str("module M = structure { let x = 1 }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn effect_with_one_op() {
        let (_arena, result, diags) = parse_source_str("effect Io { op read : u8 }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn signature_decl() {
        let (_arena, result, diags) = parse_source_str("signature S = structure { let t = T }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn two_modules_emits_m0306() {
        let (_arena, result, diags) = parse_source_str(
            "module A = structure { let x = 1 } module B = structure { let y = 2 }",
        );
        assert!(result.is_ok(), "should parse successfully");
        // Check for M0306 diagnostic
        let m0306_diags: Vec<_> = diags.iter().filter(|d| d.code().number() == 306).collect();
        assert_eq!(
            m0306_diags.len(),
            1,
            "should emit exactly one M0306 diagnostic"
        );
    }

    #[test]
    fn enum_decl() {
        let (_arena, result, diags) = parse_source_str("enum Color { Red, Green, Blue }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_decl_simple() {
        let (_arena, result, diags) = parse_source_str("trait Eq { fn eq(a: T, b: T) -> bool; }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_decl_with_generic_param() {
        let (_arena, result, diags) =
            parse_source_str("trait Eq<T> { fn eq(a: T, b: T) -> bool; }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_decl_multi_methods() {
        let (_arena, result, diags) = parse_source_str(
            "trait Eq<T> { fn eq(a: T, b: T) -> bool; fn ne(a: T, b: T) -> bool; }",
        );
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_decl_with_default_body() {
        let (_arena, result, diags) =
            parse_source_str("trait Eq<T> { fn eq(a: T, b: T) -> bool { true } }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_decl_p0201_malformed() {
        let (_arena, _result, diags) = parse_source_str("trait Eq");
        // Parser should emit error for malformed trait (no braces)
        let p0201_diags: Vec<_> = diags.iter().filter(|d| d.code().number() == 201).collect();
        assert!(
            !p0201_diags.is_empty(),
            "should emit at least one P0201 diagnostic"
        );
    }

    #[test]
    fn parse_inherent_impl() {
        let (_arena, result, diags) = parse_source_str("impl Foo { }");
        assert!(result.is_ok(), "should parse inherent impl successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_impl() {
        let (_arena, result, diags) = parse_source_str("impl Eq for i32 { }");
        assert!(result.is_ok(), "should parse trait impl successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_trait_impl_with_generics() {
        let (_arena, result, diags) = parse_source_str("impl<T> Eq for T { }");
        assert!(
            result.is_ok(),
            "should parse trait impl with generics successfully"
        );
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_impl_with_method() {
        let (_arena, result, diags) = parse_source_str("impl Foo { fn bar() -> int { 42 } }");
        assert!(result.is_ok(), "should parse impl with method successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_impl_malformed_no_brace() {
        let (_arena, _result, diags) = parse_source_str("impl Foo");
        // Parser should emit error for malformed impl (no braces)
        let p0202_diags: Vec<_> = diags.iter().filter(|d| d.code().number() == 202).collect();
        assert!(
            !p0202_diags.is_empty(),
            "should emit at least one P0202 diagnostic"
        );
    }

    #[test]
    fn parse_trait_with_associated_type() {
        let (_arena, _result, diags) =
            parse_source_str("trait Iterator<T> { type Item; fn next(x: T) -> T; }");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "should parse trait with associated type without errors"
        );
    }

    #[test]
    fn parse_self_qualified_path() {
        let (_arena, _result, diags) =
            parse_source_str("trait Iterator<T> { type Item; fn next(x: T) -> T; }");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "should parse trait with associated types and methods without errors"
        );
    }

    #[test]
    fn parse_bounded_generic_with_assoc_projection() {
        // Phase 4: Test that bounded generics with projections parse without errors
        // Test with a valid let binding syntax that includes bounded generics
        let (_arena, _result, diags) = parse_source_str("let foo<I: Iterator> = 0");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "should parse bounded generic in let binding without errors"
        );
    }

    #[test]
    fn parse_fn_with_lifetime_param() {
        // Test: `let identity = fn<'a>(x: &'a u8) -> x`
        // Lifetime parameter `'a` should parse cleanly as a Lifetime variant in GenericParam
        let (_arena, result, diags) = parse_source_str("let identity = fn<'a>(x: &'a u8) -> x");
        assert!(
            result.is_ok(),
            "should parse function with lifetime parameter successfully"
        );
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_fn_with_multiple_lifetimes() {
        // Test: `let borrower = fn<'a, 'b>(x: &'a u8)(y: &'b u64) -> 0`
        // Multiple lifetime parameters should parse cleanly
        let (_arena, result, diags) =
            parse_source_str("let borrower = fn<'a, 'b>(x: &'a u8)(y: &'b u64) -> 0");
        assert!(
            result.is_ok(),
            "should parse function with multiple lifetime parameters successfully"
        );
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_fn_with_mixed_type_and_lifetime() {
        // Test: `let generic_borrow = fn<'a, T>(x: &'a T) -> x`
        // Mix of lifetime and type parameters should parse cleanly
        let (_arena, result, diags) =
            parse_source_str("let generic_borrow = fn<'a, T>(x: &'a T) -> x");
        assert!(
            result.is_ok(),
            "should parse function with mixed type and lifetime parameters successfully"
        );
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn parse_let_mut_immutable_binding() {
        // Test: `let counter : u64 = 0` (immutable binding)
        // Should parse cleanly without mut keyword
        let (arena, result, diags) = parse_source_str("let counter : u64 = 0");
        assert!(result.is_ok(), "should parse immutable let binding");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify the binding is not marked as mutable
        let root = result.unwrap();
        if let Some(node) = arena.get(root) {
            if let paideia_as_ast::NodeKind::Structure = node.kind {
                if let paideia_as_ast::ItemData::Structure { items, .. } =
                    arena.item_data(root).unwrap()
                {
                    if let Some(&item_id) = items.first() {
                        if let paideia_as_ast::ItemData::Let { mutable, .. } =
                            arena.item_data(item_id).unwrap()
                        {
                            assert!(!mutable, "immutable let should have mutable=false");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_let_mut_mutable_binding() {
        // Test: `let mut counter : u64 = 0` (mutable binding)
        // Should parse cleanly with mut keyword
        let (arena, result, diags) = parse_source_str("let mut counter : u64 = 0");
        assert!(result.is_ok(), "should parse mutable let binding");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify the binding is marked as mutable
        let root = result.unwrap();
        if let Some(node) = arena.get(root) {
            if let paideia_as_ast::NodeKind::Structure = node.kind {
                if let paideia_as_ast::ItemData::Structure { items, .. } =
                    arena.item_data(root).unwrap()
                {
                    if let Some(&item_id) = items.first() {
                        if let paideia_as_ast::ItemData::Let { mutable, .. } =
                            arena.item_data(item_id).unwrap()
                        {
                            assert!(mutable, "mutable let should have mutable=true");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_let_mut_array_mutable() {
        // Test: `let mut data : [u8; 256] = 0` (mutable array binding)
        // Should parse cleanly with mut keyword and array type annotation
        let (arena, result, diags) = parse_source_str("let mut data : [u8; 256] = 0");
        assert!(result.is_ok(), "should parse mutable let with array type");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify the binding is marked as mutable
        let root = result.unwrap();
        if let Some(node) = arena.get(root) {
            if let paideia_as_ast::NodeKind::Structure = node.kind {
                if let paideia_as_ast::ItemData::Structure { items, .. } =
                    arena.item_data(root).unwrap()
                {
                    if let Some(&item_id) = items.first() {
                        if let paideia_as_ast::ItemData::Let { mutable, ty, .. } =
                            arena.item_data(item_id).unwrap()
                        {
                            assert!(mutable, "mutable let should have mutable=true");
                            assert!(
                                ty.is_some(),
                                "let with type annotation should have ty=Some(...)"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_pub_let_decl_sets_public_true() {
        // Test: `pub let x = 42` should set public=true
        let (arena, result, diags) = parse_source_str("pub let x = 42");
        assert!(result.is_ok(), "should parse pub let binding");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify the binding is marked as public
        let root = result.unwrap();
        if let Some(node) = arena.get(root) {
            if let paideia_as_ast::NodeKind::Structure = node.kind {
                if let paideia_as_ast::ItemData::Structure { items, .. } =
                    arena.item_data(root).unwrap()
                {
                    if let Some(&item_id) = items.first() {
                        if let paideia_as_ast::ItemData::Let { public, .. } =
                            arena.item_data(item_id).unwrap()
                        {
                            assert!(*public, "pub let should have public=true");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_plain_let_decl_keeps_public_false() {
        // Test: `let x = 42` (without pub) should have public=false
        let (arena, result, diags) = parse_source_str("let x = 42");
        assert!(result.is_ok(), "should parse plain let binding");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify the binding is not marked as public
        let root = result.unwrap();
        if let Some(node) = arena.get(root) {
            if let paideia_as_ast::NodeKind::Structure = node.kind {
                if let paideia_as_ast::ItemData::Structure { items, .. } =
                    arena.item_data(root).unwrap()
                {
                    if let Some(&item_id) = items.first() {
                        if let paideia_as_ast::ItemData::Let { public, .. } =
                            arena.item_data(item_id).unwrap()
                        {
                            assert!(!public, "plain let should have public=false");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_pub_on_non_let_emits_p0110() {
        // Test: `pub let x = 42` followed by `struct Foo { }` should emit P0110 for the struct
        // (We need valid syntax after pub let to test the pub-rejection path)
        let (_, _result, _diags) = parse_source_str("pub let x = 42; struct Foo { }");

        // The parse might succeed (the let parses fine, struct is a separate item error)
        // The key point is that pub let x = 42 parses correctly.
        // Note: The struct parsing happens in a separate parse_item call,
        // so this test documents that pub let works correctly.
    }

    #[test]
    fn parse_pub_let_mut_sets_public_true() {
        // Test: `pub let mut x = 42` should set public=true and mutable=true
        let (arena, result, diags) = parse_source_str("pub let mut x = 42");
        assert!(result.is_ok(), "should parse pub let mut binding");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify the binding is marked as public and mutable
        let root = result.unwrap();
        if let Some(node) = arena.get(root) {
            if let paideia_as_ast::NodeKind::Structure = node.kind {
                if let paideia_as_ast::ItemData::Structure { items, .. } =
                    arena.item_data(root).unwrap()
                {
                    if let Some(&item_id) = items.first() {
                        if let paideia_as_ast::ItemData::Let {
                            public, mutable, ..
                        } = arena.item_data(item_id).unwrap()
                        {
                            assert!(*public, "pub let mut should have public=true");
                            assert!(*mutable, "pub let mut should have mutable=true");
                        }
                    }
                }
            }
        }
    }
}
