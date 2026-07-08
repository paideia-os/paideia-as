//! Trait and impl declaration parsing.
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_trait_decl(&mut self) -> Result<NodeId, ParseError> {
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
    pub(super) fn parse_trait_method(&mut self) -> Result<paideia_as_ast::TraitMethod, ParseError> {
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
    pub(super) fn parse_trait_associated_type(&mut self) -> Result<NodeId, ParseError> {
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
    pub(super) fn parse_impl_decl(&mut self) -> Result<NodeId, ParseError> {
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

}
