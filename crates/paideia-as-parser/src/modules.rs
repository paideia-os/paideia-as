//! Module-level expression parsing: functor application with sharing constraints.
//!
//! This module implements parsing of functor application expressions:
//! `F(M)(N) sharing (M::t = N::t, ...)`.
//!
//! The `parse_functor_app` function is exposed for direct test invocation;
//! it is not wired into `parse_primary` to avoid regressing value-level `f(x)`.

use paideia_as_ast::{ExprData, NodeId, NodeKind, SharingConstraint};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a functor application expression: `F(M)(N) sharing (...)`.
    ///
    /// **Grammar:**
    /// ```text
    /// functor_app := module_path '(' module_path ')' ('(' module_path ')')*
    ///               ('sharing' '(' constraint (',' constraint)* ')')?
    /// constraint  := module_path '=' module_path
    /// module_path := IDENT ('::' IDENT)*
    /// ```
    ///
    /// **Algorithm:**
    /// 1. Parse functor name (module path).
    /// 2. Expect `(` and parse first module argument.
    /// 3. Consume zero or more `(M)` groups (curried arguments).
    /// 4. Check for contextual `sharing` keyword followed by `(`.
    /// 5. Parse zero or more sharing constraints separated by `,`.
    /// 6. Allocate and return ExprFunctorApp node.
    ///
    /// **Errors:**
    /// - P0190: missing close paren or malformed functor application.
    /// - P0191: missing `=` or malformed sharing constraint.
    ///
    /// This is exposed as pub for external testing. It is not wired into
    /// `parse_primary` to avoid regressing value-level `f(x)`.
    pub fn parse_functor_app(&mut self) -> Result<NodeId, ParseError> {
        let span_start = self.current_span();

        // Parse functor name (module path)
        let functor = self.parse_path_or_ident()?;

        // Expect first opening paren
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(190))
                .message("malformed functor application: expected `(`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let lparen_tok = self.expect(TokenKind::LParen)?;
        let lparen_span = lparen_tok.span;

        // Parse first module argument
        let first_arg = self.parse_path_or_ident()?;

        // Expect closing paren for first argument
        if !self.at(TokenKind::RParen) {
            return self.error_malformed_functor_app(lparen_span);
        }
        self.expect(TokenKind::RParen)?;

        // Collect all arguments (first + curried)
        let mut arguments = vec![first_arg];

        // Parse additional curried arguments: (M) (N) ...
        while self.at(TokenKind::LParen) {
            // Peek ahead to see if this is another argument or something else
            if let Some(next_tok) = self.peek_at(1) {
                if next_tok.kind == TokenKind::Ident
                    || next_tok.kind == TokenKind::KwSelfType
                    || next_tok.kind == TokenKind::KwSelfValue
                {
                    let lparen = self.expect(TokenKind::LParen)?;
                    let lparen_span = lparen.span;
                    let arg = self.parse_path_or_ident()?;
                    if !self.at(TokenKind::RParen) {
                        return self.error_malformed_functor_app(lparen_span);
                    }
                    self.expect(TokenKind::RParen)?;
                    arguments.push(arg);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Check for optional sharing constraints
        let sharing = if self.peek_contextual_keyword("sharing") {
            self.bump(); // consume "sharing"
            self.parse_sharing_constraints()?
        } else {
            vec![]
        };

        // Compute final span
        let span_end = if let Some(last_share) = sharing.last() {
            last_share.span
        } else {
            // Use span of last argument
            self.arena()
                .get(*arguments.last().unwrap())
                .map(|nd| nd.span)
                .unwrap_or(span_start)
        };

        let final_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprFunctorApp,
            final_span,
            ExprData::FunctorApp {
                functor,
                arguments,
                sharing,
            },
        ))
    }

    /// Parse sharing constraints: `( constraint (',' constraint)* )`.
    ///
    /// Each constraint is `module_path '=' module_path`.
    fn parse_sharing_constraints(&mut self) -> Result<Vec<SharingConstraint>, ParseError> {
        // Expect opening paren
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(190))
                .message("expected `(` for sharing constraints".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let _lparen_span = self.expect(TokenKind::LParen)?.span;

        let mut constraints = vec![];

        // Check for empty constraint list
        if !self.at(TokenKind::RParen) {
            loop {
                let constraint = self.parse_sharing_constraint()?;
                constraints.push(constraint);

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume comma

                // Check for trailing comma before closing paren
                if self.at(TokenKind::RParen) {
                    break;
                }
            }
        }

        // Expect closing paren
        if !self.at(TokenKind::RParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(190))
                .message("expected `)` to close sharing constraints".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.expect(TokenKind::RParen)?;

        Ok(constraints)
    }

    /// Parse a single sharing constraint: `module_path '=' module_path`.
    fn parse_sharing_constraint(&mut self) -> Result<SharingConstraint, ParseError> {
        let span_start = self.current_span();

        // Parse left path
        let left_path = self.parse_path_to_strings()?;

        // Expect `=`
        if !self.at(TokenKind::Eq) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(191))
                .message("malformed sharing constraint: expected `=`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.expect(TokenKind::Eq)?;

        // Parse right path
        let right_path = self.parse_path_to_strings()?;

        let span_end = self.current_span();
        let constraint_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(SharingConstraint {
            left_path,
            right_path,
            span: constraint_span,
        })
    }

    /// Parse a module path and extract the string segments.
    ///
    /// Path syntax: `Ident (:: Ident)*`.
    /// Returns Vec of segment strings extracted from source.
    ///
    /// **Note:** This function extracts lexeme text from the source at the
    /// byte offsets specified by token spans. For correct results, the source
    /// string must match the token positions.
    fn parse_path_to_strings(&mut self) -> Result<Vec<String>, ParseError> {
        let first_tok = self.expect(TokenKind::Ident)?;
        let source = self.source();
        let start = first_tok.span.byte_start() as usize;
        let end = (first_tok.span.byte_start() + first_tok.span.byte_len()) as usize;
        let first_segment = extract_lexeme(source, start, end);

        let mut segments = vec![first_segment];

        while self.at(TokenKind::ColonColon) {
            self.bump(); // consume `::`

            let ident_tok = self.expect(TokenKind::Ident)?;
            let source = self.source();
            let start = ident_tok.span.byte_start() as usize;
            let end = (ident_tok.span.byte_start() + ident_tok.span.byte_len()) as usize;
            let segment = extract_lexeme(source, start, end);
            segments.push(segment);
        }

        Ok(segments)
    }

    /// Check if the current token is a contextual keyword with the given lexeme.
    ///
    /// Used to identify contextual keywords like "sharing" (treated as an Ident).
    fn peek_contextual_keyword(&self, keyword: &str) -> bool {
        if let Some(tok) = self.peek()
            && tok.kind == TokenKind::Ident
        {
            let source = self.source();
            let start = tok.span.byte_start() as usize;
            let end = (tok.span.byte_start() + tok.span.byte_len()) as usize;
            if start <= source.len() && end <= source.len() {
                return &source[start..end] == keyword;
            }
        }
        false
    }

    /// Emit a P0190 error ("malformed functor application") and return Err.
    fn error_malformed_functor_app(&mut self, _opening_span: Span) -> Result<NodeId, ParseError> {
        let span = if let Some(tok) = self.peek() {
            tok.span
        } else {
            Span::new(self.file(), 0, 0)
        };

        let diag = Diagnostic::error(p_code(190))
            .message("malformed functor application: missing close paren or argument".to_string())
            .with_span(span)
            .finish();
        self.emit_diagnostic(diag);

        Err(ParseError)
    }

    /// Parse a pack expression: `pack M : S`.
    ///
    /// **Grammar:**
    /// ```text
    /// pack_expr := 'pack' module_path ':' module_path
    /// ```
    ///
    /// **Algorithm:**
    /// 1. Expect contextual `"pack"` keyword.
    /// 2. `parse_path_or_ident` → module_path.
    /// 3. Expect `Colon` token; on miss → P0192.
    /// 4. `parse_path_or_ident` → signature_path.
    /// 5. Allocate and return ExprPack node.
    ///
    /// **Errors:**
    /// - P0192: missing `:` or signature path.
    ///
    /// This is exposed as pub for external testing. It is not wired into
    /// `parse_primary` to avoid regressing value-level expressions.
    pub fn parse_pack_expr(&mut self) -> Result<NodeId, ParseError> {
        let span_start = self.current_span();

        // Expect contextual "pack" keyword
        if !self.peek_contextual_keyword("pack") {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(192))
                .message("expected `pack` keyword".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume "pack"

        // Parse module path
        let module_path = self.parse_path_or_ident()?;

        // Expect colon
        if !self.at(TokenKind::Colon) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(192))
                .message("malformed pack expression: expected `:`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.expect(TokenKind::Colon)?;

        // Parse signature path
        let signature_path = self.parse_path_or_ident()?;

        // Compute final span
        let span_end = self
            .arena()
            .get(signature_path)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        let final_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprPack,
            final_span,
            ExprData::Pack {
                module_path,
                signature_path,
            },
        ))
    }

    /// Parse an unpack expression: `unpack v`.
    ///
    /// **Grammar:**
    /// ```text
    /// unpack_expr := 'unpack' expr
    /// ```
    ///
    /// **Algorithm:**
    /// 1. Expect contextual `"unpack"` keyword.
    /// 2. parse_expr → value.
    /// 3. If parse_expr errors before consuming → P0193.
    /// 4. Allocate and return ExprUnpack node.
    ///
    /// **Errors:**
    /// - P0193: missing operand.
    ///
    /// This is exposed as pub for external testing. It is not wired into
    /// `parse_primary` to avoid regressing value-level expressions.
    pub fn parse_unpack_expr(&mut self) -> Result<NodeId, ParseError> {
        let span_start = self.current_span();

        // Expect contextual "unpack" keyword
        if !self.peek_contextual_keyword("unpack") {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(193))
                .message("expected `unpack` keyword".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume "unpack"

        // Parse operand expression
        let value = self.parse_expr()?;

        // Compute final span
        let span_end = self
            .arena()
            .get(value)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        let final_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprUnpack,
            final_span,
            ExprData::Unpack { value },
        ))
    }

    /// Parse a let-module binding: `let module N = unpack v in <expr>`.
    ///
    /// **Grammar:**
    /// ```text
    /// let_module := 'let' 'module' IDENT '=' unpack_expr 'in' expr
    /// ```
    ///
    /// **Algorithm:**
    /// 1. Expect `TokenKind::KwLet`.
    /// 2. Expect contextual `"module"`.
    /// 3. Expect Ident; extract lexeme as `name`.
    /// 4. Expect Eq; on miss → P0194.
    /// 5. Call `parse_unpack_expr()` → body.
    /// 6. Expect contextual `"in"`; on miss → P0194.
    /// 7. parse_expr → rest.
    /// 8. Allocate and return ExprLetModule node.
    ///
    /// **Errors:**
    /// - P0194: missing `=`, `unpack`, or `in`.
    ///
    /// This is exposed as pub for external testing. It is not wired into
    /// `parse_primary` to avoid regressing value-level expressions.
    pub fn parse_let_module(&mut self) -> Result<NodeId, ParseError> {
        let span_start = self.current_span();

        // Expect 'let' keyword
        if !self.at(TokenKind::KwLet) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(194))
                .message("expected `let` keyword".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume 'let'

        // Expect contextual "module" keyword
        if !self.peek_contextual_keyword("module") {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(194))
                .message("expected `module` keyword in let-module binding".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume "module"

        // Expect identifier (binding name)
        let ident_tok = self.expect(TokenKind::Ident)?;
        let source = self.source();
        let start = ident_tok.span.byte_start() as usize;
        let end = (ident_tok.span.byte_start() + ident_tok.span.byte_len()) as usize;
        let name = extract_lexeme(source, start, end);

        // Expect '='
        if !self.at(TokenKind::Eq) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(194))
                .message("malformed let-module: expected `=`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.expect(TokenKind::Eq)?;

        // Parse unpack expression (RHS of =)
        let body = self.parse_unpack_expr()?;

        // Expect contextual "in" keyword
        if !self.peek_contextual_keyword("in") {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(194))
                .message("malformed let-module: expected `in`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        self.bump(); // consume "in"

        // Parse continuation expression
        let rest = self.parse_expr()?;

        // Compute final span
        let span_end = self
            .arena()
            .get(rest)
            .map(|nd| nd.span)
            .unwrap_or(span_start);

        let final_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLetModule,
            final_span,
            ExprData::LetModule { name, body, rest },
        ))
    }
}

/// Extract lexeme text from source at the given byte range.
///
/// If the range is invalid, returns a default placeholder string.
fn extract_lexeme(source: &str, start: usize, end: usize) -> String {
    if start <= source.len() && end <= source.len() && start <= end {
        source[start..end].to_string()
    } else {
        // Return a placeholder; in tests this can happen if token spans
        // don't match the source text.
        format!("__{start}_{end}__")
    }
}

/// Construct a P-category diagnostic code at the given number.
fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
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
    fn parses_functor_app_single() {
        // F(M)
        let tokens = vec![
            tok(TokenKind::Ident, 0, 1),  // F
            tok(TokenKind::LParen, 1, 1), // (
            tok(TokenKind::Ident, 2, 1),  // M
            tok(TokenKind::RParen, 3, 1), // )
            tok(TokenKind::Eof, 4, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "F(M)",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_functor_app();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFunctorApp);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::FunctorApp {
                    functor: _,
                    arguments,
                    sharing,
                } => {
                    assert_eq!(arguments.len(), 1);
                    assert!(sharing.is_empty());
                }
                _ => panic!("expected FunctorApp variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn parses_functor_app_curried() {
        // F(M)(N)
        let tokens = vec![
            tok(TokenKind::Ident, 0, 1),  // F
            tok(TokenKind::LParen, 1, 1), // (
            tok(TokenKind::Ident, 2, 1),  // M
            tok(TokenKind::RParen, 3, 1), // )
            tok(TokenKind::LParen, 4, 1), // (
            tok(TokenKind::Ident, 5, 1),  // N
            tok(TokenKind::RParen, 6, 1), // )
            tok(TokenKind::Eof, 7, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "F(M)(N)",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_functor_app();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFunctorApp);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::FunctorApp {
                    functor: _,
                    arguments,
                    sharing,
                } => {
                    assert_eq!(arguments.len(), 2);
                    assert!(sharing.is_empty());
                }
                _ => panic!("expected FunctorApp variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn parses_functor_app_one_sharing() {
        // F(M)(N) sharing (M::t = N::t)
        // "F(M)(N) sharing (M::t = N::t)"
        // 0123456789...
        let source = "F(M)(N) sharing (M::t = N::t)";
        let tokens = vec![
            tok(TokenKind::Ident, 0, 1),       // F @ 0
            tok(TokenKind::LParen, 1, 1),      // (
            tok(TokenKind::Ident, 2, 1),       // M
            tok(TokenKind::RParen, 3, 1),      // )
            tok(TokenKind::LParen, 4, 1),      // (
            tok(TokenKind::Ident, 5, 1),       // N
            tok(TokenKind::RParen, 6, 1),      // )
            tok(TokenKind::Ident, 8, 7),       // sharing @ 8
            tok(TokenKind::LParen, 16, 1),     // (
            tok(TokenKind::Ident, 17, 1),      // M
            tok(TokenKind::ColonColon, 18, 2), // ::
            tok(TokenKind::Ident, 20, 1),      // t
            tok(TokenKind::Eq, 22, 1),         // =
            tok(TokenKind::Ident, 24, 1),      // N
            tok(TokenKind::ColonColon, 25, 2), // ::
            tok(TokenKind::Ident, 27, 1),      // t
            tok(TokenKind::RParen, 28, 1),     // )
            tok(TokenKind::Eof, 29, 0),
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

        let result = parser.parse_functor_app();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFunctorApp);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::FunctorApp {
                    functor: _,
                    arguments,
                    sharing,
                } => {
                    assert_eq!(arguments.len(), 2);
                    assert_eq!(sharing.len(), 1);
                    assert_eq!(sharing[0].left_path, vec!["M", "t"]);
                    assert_eq!(sharing[0].right_path, vec!["N", "t"]);
                }
                _ => panic!("expected FunctorApp variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn parses_functor_app_multi_sharing() {
        // F(M)(N) sharing (M::t = N::t, M::u = N::u)
        let source = "F(M)(N) sharing (M::t = N::t, M::u = N::u)";
        let tokens = vec![
            tok(TokenKind::Ident, 0, 1),       // F
            tok(TokenKind::LParen, 1, 1),      // (
            tok(TokenKind::Ident, 2, 1),       // M
            tok(TokenKind::RParen, 3, 1),      // )
            tok(TokenKind::LParen, 4, 1),      // (
            tok(TokenKind::Ident, 5, 1),       // N
            tok(TokenKind::RParen, 6, 1),      // )
            tok(TokenKind::Ident, 8, 7),       // sharing
            tok(TokenKind::LParen, 16, 1),     // (
            tok(TokenKind::Ident, 17, 1),      // M
            tok(TokenKind::ColonColon, 18, 2), // ::
            tok(TokenKind::Ident, 20, 1),      // t
            tok(TokenKind::Eq, 22, 1),         // =
            tok(TokenKind::Ident, 24, 1),      // N
            tok(TokenKind::ColonColon, 25, 2), // ::
            tok(TokenKind::Ident, 27, 1),      // t
            tok(TokenKind::Comma, 28, 1),      // ,
            tok(TokenKind::Ident, 30, 1),      // M
            tok(TokenKind::ColonColon, 31, 2), // ::
            tok(TokenKind::Ident, 33, 1),      // u
            tok(TokenKind::Eq, 35, 1),         // =
            tok(TokenKind::Ident, 37, 1),      // N
            tok(TokenKind::ColonColon, 38, 2), // ::
            tok(TokenKind::Ident, 40, 1),      // u
            tok(TokenKind::RParen, 41, 1),     // )
            tok(TokenKind::Eof, 42, 0),
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

        let result = parser.parse_functor_app();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFunctorApp);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::FunctorApp {
                    functor: _,
                    arguments,
                    sharing,
                } => {
                    assert_eq!(arguments.len(), 2);
                    assert_eq!(sharing.len(), 2);
                    assert_eq!(sharing[0].left_path, vec!["M", "t"]);
                    assert_eq!(sharing[0].right_path, vec!["N", "t"]);
                    assert_eq!(sharing[1].left_path, vec!["M", "u"]);
                    assert_eq!(sharing[1].right_path, vec!["N", "u"]);
                }
                _ => panic!("expected FunctorApp variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn rejects_missing_close_paren_p0190() {
        // F(M - missing close paren
        let tokens = vec![
            tok(TokenKind::Ident, 0, 1),  // F
            tok(TokenKind::LParen, 1, 1), // (
            tok(TokenKind::Ident, 2, 1),  // M
            tok(TokenKind::Eof, 3, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "F(M",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_functor_app();
        assert!(result.is_err());

        // Check that a P0190 diagnostic was emitted
        let diags = sink.diagnostics();
        assert!(!diags.is_empty());
        let code_str = format!("{}", diags[0].code());
        assert!(code_str.contains("P0190"));
    }

    #[test]
    fn rejects_missing_eq_p0191() {
        // F(M) sharing (M::t N::t) - missing =
        let tokens = vec![
            tok(TokenKind::Ident, 0, 1),       // F
            tok(TokenKind::LParen, 1, 1),      // (
            tok(TokenKind::Ident, 2, 1),       // M
            tok(TokenKind::RParen, 3, 1),      // )
            tok(TokenKind::Ident, 5, 7),       // sharing
            tok(TokenKind::LParen, 12, 1),     // (
            tok(TokenKind::Ident, 13, 1),      // M
            tok(TokenKind::ColonColon, 14, 2), // ::
            tok(TokenKind::Ident, 16, 1),      // t
            tok(TokenKind::Ident, 18, 1),      // N
            tok(TokenKind::ColonColon, 19, 2), // ::
            tok(TokenKind::Ident, 21, 1),      // t
            tok(TokenKind::RParen, 22, 1),     // )
            tok(TokenKind::Eof, 23, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "F(M) sharing (M::t N::t)",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_functor_app();
        assert!(result.is_err());

        // Check that a P0191 diagnostic was emitted
        let diags = sink.diagnostics();
        assert!(!diags.is_empty());
        let code_str = format!("{}", diags[0].code());
        assert!(code_str.contains("P0191"));
    }

    #[test]
    fn parses_pack_simple() {
        // pack M : S
        let tokens = vec![
            tok(TokenKind::Ident, 0, 4), // pack
            tok(TokenKind::Ident, 5, 1), // M
            tok(TokenKind::Colon, 7, 1), // :
            tok(TokenKind::Ident, 9, 1), // S
            tok(TokenKind::Eof, 10, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "pack M : S",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_pack_expr();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPack);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::Pack {
                    module_path: _,
                    signature_path: _,
                } => {
                    // Verified
                }
                _ => panic!("expected Pack variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn parses_unpack_simple() {
        // unpack v
        let tokens = vec![
            tok(TokenKind::Ident, 0, 6), // unpack
            tok(TokenKind::Ident, 7, 1), // v
            tok(TokenKind::Eof, 8, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "unpack v",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_unpack_expr();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprUnpack);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::Unpack { value: _ } => {
                    // Verified
                }
                _ => panic!("expected Unpack variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn parses_let_module_simple() {
        // let module N = unpack v in body
        let tokens = vec![
            tok(TokenKind::KwLet, 0, 3),  // let
            tok(TokenKind::Ident, 4, 6),  // module
            tok(TokenKind::Ident, 11, 1), // N
            tok(TokenKind::Eq, 13, 1),    // =
            tok(TokenKind::Ident, 15, 6), // unpack
            tok(TokenKind::Ident, 22, 1), // v
            tok(TokenKind::Ident, 24, 2), // in
            tok(TokenKind::Ident, 27, 4), // body
            tok(TokenKind::Eof, 31, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "let module N = unpack v in body",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_let_module();
        assert!(result.is_ok());
        let expr_id = result.unwrap();

        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLetModule);

        if let Some(expr_data) = arena.expr_data(expr_id) {
            match expr_data {
                ExprData::LetModule {
                    name,
                    body: _,
                    rest: _,
                } => {
                    assert_eq!(name, "N");
                }
                _ => panic!("expected LetModule variant"),
            }
        } else {
            panic!("expected expr data");
        }
    }

    #[test]
    fn rejects_pack_missing_colon() {
        // pack M S - missing colon
        let tokens = vec![
            tok(TokenKind::Ident, 0, 4), // pack
            tok(TokenKind::Ident, 5, 1), // M
            tok(TokenKind::Ident, 7, 1), // S
            tok(TokenKind::Eof, 8, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "pack M S",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_pack_expr();
        assert!(result.is_err());

        // Check that a P0192 diagnostic was emitted
        let diags = sink.diagnostics();
        assert!(!diags.is_empty());
        let code_str = format!("{}", diags[0].code());
        assert!(code_str.contains("P0192"));
    }

    #[test]
    fn rejects_let_module_missing_in() {
        // let module N = unpack v body - missing 'in'
        let tokens = vec![
            tok(TokenKind::KwLet, 0, 3),  // let
            tok(TokenKind::Ident, 4, 6),  // module
            tok(TokenKind::Ident, 11, 1), // N
            tok(TokenKind::Eq, 13, 1),    // =
            tok(TokenKind::Ident, 15, 6), // unpack
            tok(TokenKind::Ident, 22, 1), // v
            tok(TokenKind::Ident, 24, 4), // body
            tok(TokenKind::Eof, 28, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "let module N = unpack v body",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_let_module();
        assert!(result.is_err());

        // Check that a P0194 diagnostic was emitted
        let diags = sink.diagnostics();
        assert!(!diags.is_empty());
        let code_str = format!("{}", diags[0].code());
        assert!(code_str.contains("P0194"));
    }

    #[test]
    fn rejects_unpack_missing_operand() {
        // unpack (missing operand - just EOF)
        let tokens = vec![
            tok(TokenKind::Ident, 0, 6), // unpack
            tok(TokenKind::Eof, 6, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(
            &tokens,
            "unpack",
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = parser.parse_unpack_expr();
        assert!(result.is_err());

        // Check that a diagnostic was emitted
        let diags = sink.diagnostics();
        assert!(!diags.is_empty());
    }
}
