//! Primary expression parsing: literals, identifiers, paths, and parenthesized
//! expressions.
//!
//! Primary expressions are the atoms of the syntax tree — the base cases that
//! other expression categories build upon. This module implements the `parse_primary`
//! method that dispatches on token kind and constructs the appropriate AST nodes.

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::{TokenKind, extract_byte_string_content, extract_string_content};

use crate::parser::{ParseError, Parser};

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
        match self.peek() {
            None => self.error_expected_expression(),

            Some(tok) => {
                let tok_kind = tok.kind;
                let span_start = tok.span;

                match tok_kind {
                    // Numeric and character literals
                    TokenKind::IntLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::FloatLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::CharLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::StringLit => {
                        self.bump();
                        let source = self.source();

                        // Extract the token's text from the source
                        let start = span_start.byte_start() as usize;
                        let end = (span_start.byte_start() + span_start.byte_len()) as usize;
                        let token_text = if start <= source.len() && end <= source.len() {
                            &source[start..end]
                        } else {
                            ""
                        };

                        // Determine if this is a raw string by checking the token text
                        let is_raw = token_text.starts_with('r')
                            || token_text.starts_with("br")
                            || token_text.starts_with("rb");

                        match extract_string_content(token_text, 0, is_raw, false) {
                            Ok(content) => Ok(self.arena_mut().alloc_expr(
                                NodeKind::ExprString,
                                span_start,
                                ExprData::StringLiteral(content),
                            )),
                            Err(_err) => {
                                // Emit diagnostic and fall back to placeholder
                                let diag = Diagnostic::error(
                                    DiagnosticCode::new(Category::E, Severity::Error, 4)
                                        .expect("valid E code"),
                                )
                                .message("invalid string literal")
                                .with_span(span_start)
                                .finish();
                                self.emit_diagnostic(diag);
                                let lit_id =
                                    self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                                Ok(self.arena_mut().alloc_expr(
                                    NodeKind::ExprLiteral,
                                    span_start,
                                    ExprData::Literal { lit: lit_id },
                                ))
                            }
                        }
                    }

                    TokenKind::ByteLit => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::ByteStringLit => {
                        self.bump();
                        let source = self.source();

                        // Extract the token's text from the source
                        let start = span_start.byte_start() as usize;
                        let end = (span_start.byte_start() + span_start.byte_len()) as usize;
                        let token_text = if start <= source.len() && end <= source.len() {
                            &source[start..end]
                        } else {
                            ""
                        };

                        // Check for 'br' or 'rb' prefix
                        let is_raw = token_text.starts_with("br") || token_text.starts_with("rb");

                        match extract_byte_string_content(token_text, 0, is_raw) {
                            Ok(content) => Ok(self.arena_mut().alloc_expr(
                                NodeKind::ExprByteString,
                                span_start,
                                ExprData::ByteStringLiteral(content),
                            )),
                            Err(_err) => {
                                // Emit diagnostic and fall back to placeholder
                                let diag = Diagnostic::error(
                                    DiagnosticCode::new(Category::E, Severity::Error, 4)
                                        .expect("valid E code"),
                                )
                                .message("invalid byte string literal")
                                .with_span(span_start)
                                .finish();
                                self.emit_diagnostic(diag);
                                let lit_id =
                                    self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                                Ok(self.arena_mut().alloc_expr(
                                    NodeKind::ExprLiteral,
                                    span_start,
                                    ExprData::Literal { lit: lit_id },
                                ))
                            }
                        }
                    }

                    // Boolean and null constants
                    // Note: We allocate synthetic Placeholder nodes for true/false/null.
                    // A future PR will add dedicated NodeKind variants (BoolLit, NullLit).
                    TokenKind::KwTrue => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::KwFalse => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

                    TokenKind::KwNull => {
                        self.bump();
                        let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprLiteral,
                            span_start,
                            ExprData::Literal { lit: lit_id },
                        ))
                    }

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

                    TokenKind::KwSelfType | TokenKind::KwSelfValue => {
                        self.bump();
                        let ident_id = self.arena_mut().alloc(NodeKind::Ident, span_start);
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprPath,
                            span_start,
                            ExprData::Path {
                                segments: vec![ident_id],
                            },
                        ))
                    }

                    // Array literals
                    TokenKind::LBracket => self.parse_array_lit(),

                    // Parenthesized expressions and tuples
                    TokenKind::LParen => self.parse_paren_expr(),

                    // Break and continue expressions
                    TokenKind::KwBreak => {
                        self.bump();
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprBreak,
                            span_start,
                            ExprData::Break,
                        ))
                    }

                    TokenKind::KwContinue => {
                        self.bump();
                        Ok(self.arena_mut().alloc_expr(
                            NodeKind::ExprContinue,
                            span_start,
                            ExprData::Continue,
                        ))
                    }

                    // Compile-time directives: @guid, @include_bytes, etc.
                    TokenKind::At => self.parse_inline_directive(),

                    // Anything else is an error
                    // (Block expressions are handled in parse_expr_bp Step 0)
                    _ => self.error_expected_expression(),
                }
            }
        }
    }

    /// Parse a path or single identifier.
    ///
    /// Path syntax: `Ident (:: Ident)*`.
    /// Also dispatches to `parse_quote_expr` if the identifier is the
    /// contextual keyword "quote" followed by `{`, or returns an ExprUninit
    /// if the identifier is the contextual keyword "uninit".
    /// Returns an ExprPath node with segments, or an ExprQuote/ExprUninit on those contexts.
    pub(crate) fn parse_path_or_ident(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let first_tok = self.expect(TokenKind::Ident)?;
        let span_start = first_tok.span;

        // Check if this is the contextual keyword "quote" followed by `{` or "uninit"
        let source = self.source();
        let start = first_tok.span.byte_start() as usize;
        let end = (first_tok.span.byte_start() + first_tok.span.byte_len()) as usize;
        let ident_lexeme = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        if ident_lexeme == "quote" && self.peek().is_some_and(|t| t.kind == TokenKind::LBrace) {
            return self.parse_quote_expr(first_tok);
        }

        if ident_lexeme == "uninit" {
            return Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprUninit,
                span_start,
                ExprData::Uninit,
            ));
        }

        // Otherwise, parse as a normal path
        let mut segments = vec![self.arena_mut().alloc(NodeKind::Ident, span_start)];
        let mut span_end = span_start;

        while self.at(TokenKind::ColonColon) {
            self.bump(); // consume `::`

            let ident_tok = self.expect(TokenKind::Ident)?;
            span_end = ident_tok.span;
            segments.push(self.arena_mut().alloc(NodeKind::Ident, ident_tok.span));
        }

        // Compute the span covering the entire path.
        let path_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_expr(NodeKind::ExprPath, path_span, ExprData::Path { segments }))
    }

    /// Parse an array literal: `[expr1, expr2, ..., exprN]` or `[expr1, expr2, ...,]`.
    ///
    /// Algorithm:
    /// 1. Expect `[`.
    /// 2. If `]` immediately follows, emit P0210 and return Err (empty array without type annotation).
    /// 3. Otherwise, parse comma-separated expressions until `]`.
    /// 4. Allow trailing comma before the closing `]`.
    /// 5. Return ExprArrayLit with the list of element node IDs.
    ///
    /// Returns an ExprArrayLit node with `ArrayLit(Vec<NodeId>)`.
    fn parse_array_lit(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lbracket_tok = self.expect(TokenKind::LBracket)?;
        let span_start = lbracket_tok.span;

        // Check for empty array literal: `[]`
        if self.at(TokenKind::RBracket) {
            // Empty array requires explicit type annotation — emit P0210
            let span = span_start;
            let diag = Diagnostic::error(p_code(210))
                .message("empty array literal requires explicit type annotation".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Parse first element
        let first_elem = self.parse_expr()?;

        // Check for repeat syntax: `[expr; count]`
        if self.at(TokenKind::Semicolon) {
            self.bump(); // consume semicolon
            let count_expr = self.parse_expr()?;

            // Expect closing bracket
            if !self.at(TokenKind::RBracket) {
                return self.error_mismatched_delimiter(span_start);
            }
            let rbracket_tok = self.expect(TokenKind::RBracket)?;
            let rbracket_span = rbracket_tok.span;

            // Compute span from `[` to `]`
            let array_span = Span::new(
                span_start.file(),
                span_start.byte_start(),
                rbracket_span.byte_start() + rbracket_span.byte_len() - span_start.byte_start(),
            );

            // Allocate an ArrayRepeat node. The elaborator will expand this during lowering
            // by evaluating the count literal and replicating the expr.
            return Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprArrayRepeat,
                array_span,
                ExprData::ArrayRepeat {
                    expr: first_elem,
                    count: count_expr,
                },
            ));
        }

        // Normal comma-separated array literal
        let mut elements = vec![first_elem];

        loop {
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma

            // Check for trailing comma before closing bracket
            if self.at(TokenKind::RBracket) {
                break;
            }

            elements.push(self.parse_expr()?);
        }

        // Expect closing bracket
        if !self.at(TokenKind::RBracket) {
            return self.error_mismatched_delimiter(span_start);
        }
        let rbracket_tok = self.expect(TokenKind::RBracket)?;
        let rbracket_span = rbracket_tok.span;

        // Compute span from `[` to `]`
        let array_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbracket_span.byte_start() + rbracket_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprArrayLit,
            array_span,
            ExprData::ArrayLit(elements),
        ))
    }

    /// Parse parenthesized expressions: `()`, `(expr)`, or `(a, b, c)`.
    ///
    /// - `()` allocates a Placeholder and wraps it in ExprLiteral.
    /// - `(expr)` returns the inner expression (parens are syntactic sugar).
    /// - `(a, b, c)` allocates a Placeholder node (tuples deferred to a later PR).
    fn parse_paren_expr(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Check for empty parens: `()`
        if self.at(TokenKind::RParen) {
            self.bump();
            let lit_id = self.arena_mut().alloc(NodeKind::Placeholder, lparen_span);
            return Ok(self.arena_mut().alloc_expr(
                NodeKind::ExprLiteral,
                lparen_span,
                ExprData::Literal { lit: lit_id },
            ));
        }

        // Parse the first expression with full infix/prefix/postfix support
        let first_expr = self.parse_expr()?;

        // Check for comma: tuple case or parenthesized single expr?
        if self.at(TokenKind::Comma) {
            // Tuple: collect remaining elements
            let mut _elements = vec![first_expr];

            while self.at(TokenKind::Comma) {
                self.bump(); // consume comma

                // Check for trailing comma before closing paren
                if self.at(TokenKind::RParen) {
                    break;
                }

                _elements.push(self.parse_expr()?);
            }

            if !self.at(TokenKind::RParen) {
                return self.error_mismatched_delimiter(lparen_span);
            }
            let rparen_tok = self.expect(TokenKind::RParen)?;
            let rparen_span = rparen_tok.span;

            // Allocate tuple as Placeholder (deferred to future PR).
            // Span covers the entire tuple, from `(` to `)`.
            let tuple_span = Span::new(
                lparen_span.file(),
                lparen_span.byte_start(),
                rparen_span.byte_start() + rparen_span.byte_len() - lparen_span.byte_start(),
            );

            return Ok(self.arena_mut().alloc(NodeKind::Placeholder, tuple_span));
        }

        // Parenthesized single expression: expect RParen and return inner expr
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        self.bump(); // consume `)`
        Ok(first_expr)
    }

    /// Parse a perform expression: `perform Effect::op(args)`.
    ///
    /// Algorithm:
    /// 1. Expect `KwPerform`.
    /// 2. Parse a path (Ident (:: Ident)*).
    /// 3. Expect `LParen`.
    /// 4. Parse comma-separated argument expressions until `RParen`.
    /// 5. Allocate ExprData::Perform { op_path, args }.
    fn parse_perform(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let perform_tok = self.expect(TokenKind::KwPerform)?;
        let span_start = perform_tok.span;

        // Parse the effect operation path (e.g., `Io::port_read`)
        let op_path = self.parse_path_or_ident()?;

        // Expect `(`
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(161))
                .message("expected `(` after effect-operation path".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Parse arguments: comma-separated expressions
        let mut args = vec![];
        if !self.at(TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume comma

                // Check for trailing comma
                if self.at(TokenKind::RParen) {
                    break;
                }
            }
        }

        // Expect `)`
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        let rparen_tok = self.expect(TokenKind::RParen)?;
        let rparen_span = rparen_tok.span;

        // Compute span from `perform` keyword through closing `)`
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprPerform,
            span,
            ExprData::Perform { op_path, args },
        ))
    }

    /// Parse a resume expression: `resume value`.
    ///
    /// Algorithm:
    /// 1. Expect `KwResume`.
    /// 2. Parse a full expression.
    /// 3. Allocate ExprData::Resume { value }.
    fn parse_resume(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let resume_tok = self.expect(TokenKind::KwResume)?;
        let span_start = resume_tok.span;

        // Parse the value expression with full infix/prefix/postfix support
        let value = self.parse_expr()?;

        let value_span = self
            .arena()
            .get(value)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        // Compute span from `resume` keyword through the value
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            value_span.byte_start() + value_span.byte_len() - span_start.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_expr(NodeKind::ExprResume, span, ExprData::Resume { value }))
    }

    /// Parse a compile-time inline directive: `@guid("...")`, `@include_bytes("...")`, etc.
    ///
    /// Algorithm:
    /// 1. Expect `@` token.
    /// 2. Peek next token; must be Ident (the directive name).
    /// 3. Dispatch based on the directive name (guid, include_bytes, etc.).
    /// 4. Each directive parser returns an ExprInlineBytes on success.
    fn parse_inline_directive(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
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

    /// Parse a GUID literal: `@guid("XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX")`.
    ///
    /// Algorithm:
    /// 1. Expect `(`.
    /// 2. Expect a string literal (the GUID string).
    /// 3. Extract the string content.
    /// 4. Parse the GUID string into 16 bytes using parse_guid helper.
    /// 5. On error, emit P0278 diagnostic.
    /// 6. On success, allocate ExprInlineBytes node.
    fn parse_guid_literal(&mut self, at_span: Span) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Expect `(`
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(278))
                .message("expected `(` after @guid".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Expect a string literal
        if !self.at(TokenKind::StringLit) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(278))
                .message("expected string literal in @guid(...)".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let source = self.source();
        let start = str_tok.span.byte_start() as usize;
        let end = (str_tok.span.byte_start() + str_tok.span.byte_len()) as usize;
        let token_text = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Extract the string content (handle raw string prefixes if any)
        let is_raw = token_text.starts_with('r');
        let guid_string = match extract_string_content(token_text, 0, is_raw, false) {
            Ok(content) => content,
            Err(_) => {
                let diag = Diagnostic::error(p_code(278))
                    .message("invalid string literal in @guid".to_string())
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Parse the GUID string into 16 bytes
        let guid_bytes = match parse_guid(&guid_string) {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                let diag = Diagnostic::error(p_code(278))
                    .message(format!("malformed GUID literal: {}", e.message()))
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Expect `)`
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        let rparen_tok = self.expect(TokenKind::RParen)?;
        let rparen_span = rparen_tok.span;

        // Compute span from `@` through closing `)`
        let span = Span::new(
            at_span.file(),
            at_span.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - at_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprInlineBytes,
            span,
            ExprData::InlineBytes(guid_bytes),
        ))
    }

    /// Emit a P0100 ("expected expression") diagnostic and return `Err(ParseError)`.
    fn error_expected_expression(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            // At EOF: use a zero-width span at byte 0
            Span::new(self.file(), 0, 0)
        };

        let diag = Diagnostic::error(p_code(100))
            .message("expected expression".to_string())
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

/// Error type for GUID parsing.
#[derive(Debug, Clone, Copy)]
enum GuidError {
    /// GUID string is not exactly 36 characters.
    WrongLength,
    /// Dashes are in wrong positions (must be at 8, 13, 18, 23).
    MalformedDashes,
    /// Non-hexadecimal character encountered outside dash positions.
    NonHex,
}

impl GuidError {
    fn message(self) -> &'static str {
        match self {
            GuidError::WrongLength => "GUID must be exactly 36 characters (format: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX)",
            GuidError::MalformedDashes => "dashes must be at positions 8, 13, 18, 23",
            GuidError::NonHex => "all characters except dashes must be hexadecimal (0-9, a-f, A-F)",
        }
    }
}

/// Parse a GUID string in the format "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
/// into 16 bytes in UEFI mixed-endian byte order.
///
/// UEFI mixed-endian format:
/// - Data1 (0-3): u32 little-endian
/// - Data2 (4-5): u16 little-endian
/// - Data3 (6-7): u16 little-endian
/// - Data4 (8-15): 8 bytes in big-endian (raw order)
///
/// Example: "12345678-1234-1234-1234-123456789abc" produces
/// [0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x12, 0x34, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
fn parse_guid(text: &str) -> Result<[u8; 16], GuidError> {
    // Length must be exactly 36
    if text.len() != 36 {
        return Err(GuidError::WrongLength);
    }

    // Verify dashes are at positions 8, 13, 18, 23
    for (i, c) in text.chars().enumerate() {
        let expect_dash = matches!(i, 8 | 13 | 18 | 23);
        if expect_dash && c != '-' {
            return Err(GuidError::MalformedDashes);
        }
        if !expect_dash && !c.is_ascii_hexdigit() {
            return Err(GuidError::NonHex);
        }
    }

    // Parse subfields
    let data1 = u32::from_str_radix(&text[0..8], 16).map_err(|_| GuidError::NonHex)?;
    let data2 = u16::from_str_radix(&text[9..13], 16).map_err(|_| GuidError::NonHex)?;
    let data3 = u16::from_str_radix(&text[14..18], 16).map_err(|_| GuidError::NonHex)?;

    // UEFI mixed-endian layout
    let mut out = [0u8; 16];

    // Data1 as little-endian u32
    out[0..4].copy_from_slice(&data1.to_le_bytes());

    // Data2 as little-endian u16
    out[4..6].copy_from_slice(&data2.to_le_bytes());

    // Data3 as little-endian u16
    out[6..8].copy_from_slice(&data3.to_le_bytes());

    // Data4: first part "xxxx" at positions 19-23 (2 bytes)
    for i in 0..2 {
        out[8 + i] = u8::from_str_radix(&text[19 + i * 2..19 + i * 2 + 2], 16)
            .map_err(|_| GuidError::NonHex)?;
    }

    // Data4: second part "xxxxxxxxxxxx" at positions 24-36 (6 bytes)
    for i in 0..6 {
        out[10 + i] = u8::from_str_radix(&text[24 + i * 2..24 + i * 2 + 2], 16)
            .map_err(|_| GuidError::NonHex)?;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    #[test]
    fn parses_int_literal() {
        let tokens = vec![tok(TokenKind::IntLit, 0, 2), tok(TokenKind::Eof, 2, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        // Verify it's an ExprLiteral
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_string_literal() {
        let source = "\"hello\"";
        let tokens = vec![tok(TokenKind::StringLit, 0, 7), tok(TokenKind::Eof, 7, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprString);

        // Verify the string content was parsed
        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::StringLiteral(s) => assert_eq!(s, "hello"),
                _ => panic!("expected StringLiteral"),
            }
        }
    }

    #[test]
    fn parses_char_literal() {
        let tokens = vec![tok(TokenKind::CharLit, 0, 3), tok(TokenKind::Eof, 3, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_bool_true() {
        let tokens = vec![tok(TokenKind::KwTrue, 0, 4), tok(TokenKind::Eof, 4, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_bool_false() {
        let tokens = vec![tok(TokenKind::KwFalse, 0, 5), tok(TokenKind::Eof, 5, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_simple_identifier() {
        let tokens = vec![tok(TokenKind::Ident, 0, 4), tok(TokenKind::Eof, 4, 0)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPath);
    }

    #[test]
    fn parses_path_of_three_segments() {
        let tokens = vec![
            tok(TokenKind::Ident, 0, 2),      // "a"
            tok(TokenKind::ColonColon, 2, 2), // "::"
            tok(TokenKind::Ident, 4, 2),      // "b"
            tok(TokenKind::ColonColon, 6, 2), // "::"
            tok(TokenKind::Ident, 8, 2),      // "c"
            tok(TokenKind::Eof, 10, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPath);
    }

    #[test]
    fn parses_parenthesized_expression() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::IntLit, 1, 2),
            tok(TokenKind::RParen, 3, 1),
            tok(TokenKind::Eof, 4, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_unit_literal() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::RParen, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn parses_tuple_three_elements() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            tok(TokenKind::Comma, 4, 1),
            tok(TokenKind::IntLit, 5, 1),
            tok(TokenKind::RParen, 6, 1),
            tok(TokenKind::Eof, 7, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::Placeholder);
    }

    #[test]
    fn mismatched_paren_emits_p0101() {
        let tokens = vec![
            tok(TokenKind::LParen, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            // Missing RParen
            tok(TokenKind::Eof, 4, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_err());
        assert_eq!(sink.diagnostics().len(), 1);
        let diag = &sink.diagnostics()[0];
        assert_eq!(diag.code().number(), 101);
    }

    #[test]
    fn parses_empty_block_rejected() {
        // Empty blocks are not allowed; must have a tail expression.
        // Per #156 requirement: emits P0157 and returns Err.
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::RBrace, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        // Note: blocks are now parsed in parse_expr_bp Step 0, not in parse_primary
        let result = parser.parse_expr();
        assert!(result.is_err(), "empty block should parse error");

        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 157),
            "expected P0157 diagnostic (empty block)"
        );
    }

    #[test]
    fn parses_perform_basic() {
        // perform Io::port_read(0x60)
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 9),
            tok(TokenKind::LParen, 21, 1),
            tok(TokenKind::IntLit, 22, 3),
            tok(TokenKind::RParen, 25, 1),
            tok(TokenKind::Eof, 26, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPerform);
    }

    #[test]
    fn parses_perform_zero_args() {
        // perform Io::flush()
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 5),
            tok(TokenKind::LParen, 17, 1),
            tok(TokenKind::RParen, 18, 1),
            tok(TokenKind::Eof, 19, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPerform);
        if let Some(ExprData::Perform { args, .. }) = arena.expr_data(expr_id) {
            assert_eq!(args.len(), 0);
        } else {
            panic!("expected Perform variant");
        }
    }

    #[test]
    fn parses_perform_multi_args() {
        // perform Io::port_write(0x64, 0xED)
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 10),
            tok(TokenKind::LParen, 22, 1),
            tok(TokenKind::IntLit, 23, 3),
            tok(TokenKind::Comma, 26, 1),
            tok(TokenKind::IntLit, 28, 3),
            tok(TokenKind::RParen, 31, 1),
            tok(TokenKind::Eof, 32, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        if let Some(ExprData::Perform { args, .. }) = arena.expr_data(expr_id) {
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected Perform variant");
        }
    }

    #[test]
    fn parses_perform_path_three_segments() {
        // perform Mod::Io::read(addr)
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 3),
            tok(TokenKind::ColonColon, 11, 2),
            tok(TokenKind::Ident, 13, 2),
            tok(TokenKind::ColonColon, 15, 2),
            tok(TokenKind::Ident, 17, 4),
            tok(TokenKind::LParen, 21, 1),
            tok(TokenKind::Ident, 22, 4),
            tok(TokenKind::RParen, 26, 1),
            tok(TokenKind::Eof, 27, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        if let Some(ExprData::Perform { op_path, .. }) = arena.expr_data(expr_id) {
            if let Some(ExprData::Path { segments }) = arena.expr_data(*op_path) {
                assert_eq!(segments.len(), 3);
            } else {
                panic!("expected Path for op_path");
            }
        } else {
            panic!("expected Perform variant");
        }
    }

    #[test]
    fn perform_missing_paren_emits_p0161() {
        // perform Io::flush ... missing (
        let tokens = vec![
            tok(TokenKind::KwPerform, 0, 7),
            tok(TokenKind::Ident, 8, 2),
            tok(TokenKind::ColonColon, 10, 2),
            tok(TokenKind::Ident, 12, 5),
            tok(TokenKind::Eof, 17, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_err());
        assert_eq!(sink.diagnostics().len(), 1);
        let diag = &sink.diagnostics()[0];
        assert_eq!(diag.code().number(), 161);
    }

    #[test]
    fn parses_resume_value() {
        // resume v
        let tokens = vec![
            tok(TokenKind::KwResume, 0, 6),
            tok(TokenKind::Ident, 7, 1),
            tok(TokenKind::Eof, 8, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprResume);
        if let Some(ExprData::Resume { value }) = arena.expr_data(expr_id) {
            let value_node = arena.get(*value).unwrap();
            assert_eq!(value_node.kind, NodeKind::ExprPath);
        } else {
            panic!("expected Resume variant");
        }
    }

    #[test]
    fn parses_resume_unit() {
        // resume ()
        let tokens = vec![
            tok(TokenKind::KwResume, 0, 6),
            tok(TokenKind::LParen, 7, 1),
            tok(TokenKind::RParen, 8, 1),
            tok(TokenKind::Eof, 9, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprResume);
    }

    #[test]
    fn parses_array_lit_single_element() {
        // [1]
        let tokens = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::RBracket, 2, 1),
            tok(TokenKind::Eof, 3, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprArrayLit);
        if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
            assert_eq!(elements.len(), 1);
        } else {
            panic!("expected ArrayLit variant");
        }
    }

    #[test]
    fn parses_array_lit_three_elements() {
        // [1, 2, 3]
        let tokens = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            tok(TokenKind::Comma, 4, 1),
            tok(TokenKind::IntLit, 5, 1),
            tok(TokenKind::RBracket, 6, 1),
            tok(TokenKind::Eof, 7, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprArrayLit);
        if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("expected ArrayLit variant");
        }
    }

    #[test]
    fn parses_array_lit_trailing_comma() {
        // [1, 2, 3,]
        let tokens = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            tok(TokenKind::Comma, 4, 1),
            tok(TokenKind::IntLit, 5, 1),
            tok(TokenKind::Comma, 6, 1),
            tok(TokenKind::RBracket, 7, 1),
            tok(TokenKind::Eof, 8, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprArrayLit);
        if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("expected ArrayLit variant");
        }
    }

    #[test]
    fn parses_array_lit_with_byte_literals() {
        // [0xCF, 0x9A, 0x00, 0x00, 0xFF]
        let tokens = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::IntLit, 1, 4), // 0xCF
            tok(TokenKind::Comma, 5, 1),
            tok(TokenKind::IntLit, 6, 4), // 0x9A
            tok(TokenKind::Comma, 10, 1),
            tok(TokenKind::IntLit, 11, 5), // 0x00
            tok(TokenKind::Comma, 16, 1),
            tok(TokenKind::IntLit, 17, 5), // 0x00
            tok(TokenKind::Comma, 22, 1),
            tok(TokenKind::IntLit, 23, 4), // 0xFF
            tok(TokenKind::RBracket, 27, 1),
            tok(TokenKind::Eof, 28, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprArrayLit);
        if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
            assert_eq!(elements.len(), 5);
        } else {
            panic!("expected ArrayLit variant");
        }
    }

    #[test]
    fn empty_array_lit_emits_p0210() {
        // []
        let tokens = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::RBracket, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_err(), "empty array literal should fail");
        assert_eq!(sink.diagnostics().len(), 1);
        let diag = &sink.diagnostics()[0];
        assert_eq!(diag.code().number(), 210);
    }

    #[test]
    fn array_lit_missing_close_emits_p0101() {
        // [1, 2 (EOF) - missing ]
        let tokens = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::IntLit, 1, 1),
            tok(TokenKind::Comma, 2, 1),
            tok(TokenKind::IntLit, 3, 1),
            tok(TokenKind::Eof, 4, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_primary();
        assert!(result.is_err(), "missing closing bracket should fail");
        assert!(!sink.diagnostics().is_empty());
        let diag = &sink.diagnostics()[sink.diagnostics().len() - 1];
        assert_eq!(diag.code().number(), 101);
    }

    /// Phase 6 m5-001: Test — `uninit` contextual keyword parses as ExprUninit.
    #[test]
    fn parses_uninit_expr() {
        let source = "uninit";
        let tokens = vec![
            tok(TokenKind::Ident, 0, 6), // "uninit"
            tok(TokenKind::Eof, 6, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_primary();
        assert!(
            result.is_ok(),
            "uninit should parse as a primary expression"
        );
        let expr_id = result.unwrap();

        // Verify it's an ExprUninit node
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprUninit);

        // Verify the ExprData is Uninit
        if let Some(ExprData::Uninit) = arena.expr_data(expr_id) {
            // Success
        } else {
            panic!("expected Uninit variant");
        }
    }

    #[test]
    fn parse_guid_valid_lowercase() {
        let guid = "12345678-1234-1234-1234-123456789abc";
        let result = parse_guid(guid);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert_eq!(
            bytes,
            [0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x12, 0x34, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
        );
    }

    #[test]
    fn parse_guid_uppercase() {
        let guid = "ABCDEF01-2345-6789-ABCD-EF0123456789";
        let result = parse_guid(guid);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert_eq!(
            bytes,
            [0x01, 0xef, 0xcd, 0xab, 0x45, 0x23, 0x89, 0x67, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89]
        );
    }

    #[test]
    fn parse_guid_mixed_case() {
        let guid = "AaBbCcDd-EeFf-0011-2233-445566778899";
        let result = parse_guid(guid);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        // Data1: AaBbCcDd → dd, cc, bb, aa (little-endian)
        // Data2: EeFf → ff, ee (little-endian)
        // Data3: 0011 → 11, 00 (little-endian)
        // Data4: 2233, 445566778899 → 22, 33, 44, 55, 66, 77, 88, 99
        assert_eq!(
            bytes,
            [0xdd, 0xcc, 0xbb, 0xaa, 0xff, 0xee, 0x11, 0x00, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99]
        );
    }

    #[test]
    fn parse_guid_all_zeros() {
        let guid = "00000000-0000-0000-0000-000000000000";
        let result = parse_guid(guid);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert_eq!(bytes, [0u8; 16]);
    }

    #[test]
    fn parse_guid_all_fs() {
        let guid = "ffffffff-ffff-ffff-ffff-ffffffffffff";
        let result = parse_guid(guid);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert_eq!(bytes, [0xffu8; 16]);
    }

    #[test]
    fn parse_guid_dashes_in_wrong_places() {
        // GUID with dash at position 7 instead of 8
        // A dash at a position where hex is expected is treated as NonHex, not MalformedDashes
        let guid = "1234567-12345-1234-1234-123456789abc";
        let result = parse_guid(guid);
        assert!(matches!(result, Err(GuidError::NonHex)));
    }

    #[test]
    fn parse_guid_wrong_length() {
        let guid = "12345678-1234-1234-1234-123456789abc-extra";
        let result = parse_guid(guid);
        assert!(matches!(result, Err(GuidError::WrongLength)));
    }

    #[test]
    fn parse_guid_too_short() {
        let guid = "12345678-1234-1234-1234-123456789ab";
        let result = parse_guid(guid);
        assert!(matches!(result, Err(GuidError::WrongLength)));
    }

    #[test]
    fn parse_guid_non_hex() {
        let guid = "gggggggg-1234-1234-1234-123456789abc";
        let result = parse_guid(guid);
        assert!(matches!(result, Err(GuidError::NonHex)));
    }

    #[test]
    fn parse_guid_empty() {
        let guid = "";
        let result = parse_guid(guid);
        assert!(matches!(result, Err(GuidError::WrongLength)));
    }
}
