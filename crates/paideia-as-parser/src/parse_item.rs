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

use paideia_as_ast::{GenericParam, ItemData, NodeId, NodeKind};
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
    /// - `KwUnsafe` → `parse_unsafe` (existing parser, wrapped as an expression)
    /// - `Ident` with lexeme "macro" → `parse_macro_decl` (contextual keyword)
    /// - Anything else → emit P0100 and return Err
    ///
    /// Returns the `NodeId` of the allocated item on success.
    pub fn parse_item(&mut self) -> Result<NodeId, ParseError> {
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::KwModule) => self.parse_module_decl(),
            Some(TokenKind::KwSignature) => self.parse_signature_decl(),
            Some(TokenKind::KwLet) => self.parse_let_decl(),
            Some(TokenKind::KwEffect) => self.parse_effect_decl(),
            Some(TokenKind::KwCapability) => self.parse_capability_decl(),
            Some(TokenKind::KwStruct) => self.parse_struct_decl(),
            Some(TokenKind::KwEnum) => self.parse_enum_decl(),
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
                    .message("expected item (module, signature, let, effect, capability, struct, enum, macro, or unsafe)")
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
                    .message("expected item (module, signature, let, effect, capability, struct, enum, macro, or unsafe)")
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
        let mut module_count = 0;
        let file_span_start = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));

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

        // Allocate synthetic root Structure
        let root_span = self.peek().map(|t| t.span).unwrap_or(file_span_start);
        let root = self.arena_mut().alloc_item(
            NodeKind::Structure,
            root_span,
            ItemData::Structure { items, doc: None },
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
            ItemData::Structure { items, doc: None },
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

    /// Parse a top-level let declaration: `let <Ident> <GenericParams>? (: Type)? = Expr`
    fn parse_let_decl(&mut self) -> Result<NodeId, ParseError> {
        let let_tok = self.expect(TokenKind::KwLet)?;
        let span_start = let_tok.span;

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
                name: name_id,
                generic_params,
                ty,
                value,
                doc: None,
            },
        );
        Ok(item)
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

    /// Parse a struct type declaration: `struct <Ident> <GenericParams>? { ... }`
    ///
    /// For phase-1, the body is parsed as a skeleton (just match braces).
    fn parse_struct_decl(&mut self) -> Result<NodeId, ParseError> {
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
                doc: None,
            },
        );
        Ok(item)
    }

    /// Parse an enum type declaration: `enum <Ident> <GenericParams>? { ... }`
    ///
    /// For phase-1, the body is parsed as a skeleton (just match braces).
    fn parse_enum_decl(&mut self) -> Result<NodeId, ParseError> {
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
                doc: None,
            },
        );
        Ok(item)
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

            // Parse parameter name (Ident)
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
            let mut bounds = Vec::new();
            if self.eat(TokenKind::Colon) {
                loop {
                    // Parse trait name as a path
                    let trait_name = self.parse_type_name_path()?;
                    bounds.push(trait_name);

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

            params.push(GenericParam {
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
}
