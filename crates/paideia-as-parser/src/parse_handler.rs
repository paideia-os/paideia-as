//! With-handler expression parsing.
//!
//! Implements §8 WithHandlerExpr grammar: `with handler-expr handle name block [finally => body]`.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a with-handler expression: `with handler-expr handle name block [finally => body]`.
    ///
    /// **Algorithm:**
    /// 1. Expect `KwWith`.
    /// 2. Parse handler expression via `parse_expr()`.
    /// 3. Expect `KwHandle`.
    /// 4. Expect `Ident` (the bound name).
    /// 5. Parse block body and optional finally clause via `parse_handler_body()`.
    /// 6. Allocate `ExprData::WithHandler { handler, bind, block, finally }`.
    ///
    /// Returns the `NodeId` of the allocated expression on success.
    pub(crate) fn parse_with_handler(&mut self) -> Result<NodeId, ParseError> {
        let with_tok = self.expect(TokenKind::KwWith)?;
        let span_start = with_tok.span;

        // Parse the handler expression
        let handler = self.parse_expr()?;

        // Expect `handle`
        self.expect(TokenKind::KwHandle)?;

        // Expect binding identifier
        let bind_tok = self.expect(TokenKind::Ident)?;
        let bind = self.arena_mut().alloc(NodeKind::Ident, bind_tok.span);

        // Parse the block body and optional finally clause
        let (block, finally, final_span) = self.parse_handler_body()?;

        let span = paideia_as_diagnostics::Span::new(
            span_start.file(),
            span_start.byte_start(),
            final_span.byte_start() + final_span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprWithHandler,
            span,
            ExprData::WithHandler {
                handler,
                bind,
                block,
                finally,
            },
        ))
    }

    /// Parse a handler block body: `{ stmts [finally => expr] }`.
    ///
    /// Returns `(block_node_id, finally_expr_opt, end_span)` where:
    /// - `block_node_id`: the ExprBlock containing stmts (and possibly the finally tail).
    /// - `finally_expr_opt`: `Some(expr)` if `finally => expr` was present, else `None`.
    /// - `end_span`: the span of the closing `}` or final expression.
    ///
    /// **Algorithm:**
    /// 1. Expect `{`.
    /// 2. Parse statements/expressions as normal until we see `finally` or `}`.
    /// 3. If `finally` is seen:
    ///    - Expect `=>`.
    ///    - Parse one final expression.
    ///    - Expect `}`.
    ///    - Emit P0162 if any token follows before `}`.
    /// 4. Return (block, Some(finally_expr), span_of_rbrace).
    fn parse_handler_body(&mut self) -> Result<(NodeId, Option<NodeId>, Span), ParseError> {
        let lbrace_tok = self.expect(TokenKind::LBrace)?;
        let lbrace_span = lbrace_tok.span;

        let mut stmts = Vec::new();
        let mut tail = None;

        loop {
            // Check for closing brace or finally
            if self.at(TokenKind::RBrace) {
                break;
            }

            if self.at(TokenKind::KwFinally) {
                // Stop parsing regular statements; handle finally below
                break;
            }

            // Check if this is a let or return statement
            if self.at(TokenKind::KwLet) || self.at(TokenKind::KwReturn) {
                let stmt = self.parse_stmt(false)?;
                stmts.push(stmt);
            } else {
                // Parse one expression
                let expr = self.parse_expr()?;

                // Check for semicolon
                if self.at(TokenKind::Semicolon) {
                    self.bump(); // consume `;`
                    let expr_span = self
                        .arena()
                        .get(expr)
                        .map(|nd| nd.span)
                        .unwrap_or(lbrace_span);
                    let stmt = self.arena_mut().alloc_stmt(
                        NodeKind::StmtExpr,
                        expr_span,
                        paideia_as_ast::StmtData::Expr { expr },
                    );
                    stmts.push(stmt);
                } else {
                    // No semicolon: this is the tail expression (unless finally follows)
                    if self.at(TokenKind::KwFinally) {
                        tail = Some(expr);
                        break;
                    }
                    tail = Some(expr);
                }
            }
        }

        // Now check for finally clause
        let mut finally_expr: Option<NodeId> = None;
        if self.at(TokenKind::KwFinally) {
            self.bump(); // consume `finally`

            // Expect `=>`
            self.expect(TokenKind::FatArrow)?;

            // Parse the finally expression
            finally_expr = Some(self.parse_expr()?);

            // After finally, we must see RBrace
            if !self.at(TokenKind::RBrace) {
                use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
                let span = if let Some(tok) = self.peek() {
                    tok.span
                } else {
                    Span::new(self.file(), 0, 0)
                };
                let code = DiagnosticCode::new(Category::P, Severity::Error, 162)
                    .expect("valid P0162 code");
                self.emit_diagnostic(
                    Diagnostic::error(code)
                        .message("`finally` must be the last clause of a handler block".to_string())
                        .with_span(span)
                        .finish(),
                );
                return Err(ParseError);
            }
        }

        // Expect closing brace
        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let rbrace_span = rbrace_tok.span;

        let block_span = Span::new(
            lbrace_span.file(),
            lbrace_span.byte_start(),
            rbrace_span.byte_start() + rbrace_span.byte_len() - lbrace_span.byte_start(),
        );

        // Validate block: must not be empty, and must end with an expression (or have finally)
        if stmts.is_empty() && tail.is_none() && finally_expr.is_none() {
            use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 157).expect("valid P0157 code");
            self.emit_diagnostic(
                Diagnostic::error(code)
                    .message("empty block expression is not allowed")
                    .with_span(block_span)
                    .finish(),
            );
            return Err(ParseError);
        }

        if !stmts.is_empty() && tail.is_none() && finally_expr.is_none() {
            use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 158).expect("valid P0158 code");
            self.emit_diagnostic(
                Diagnostic::error(code)
                    .message(
                        "block expression must have a final expression or finally clause; trailing `;` is not allowed"
                    )
                    .with_span(block_span)
                    .finish(),
            );
            return Err(ParseError);
        }

        let block = self.arena_mut().alloc_expr(
            NodeKind::ExprBlock,
            block_span,
            ExprData::Block { stmts, tail },
        );

        Ok((block, finally_expr, rbrace_span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn tok(kind: paideia_as_lexer::TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    #[test]
    fn parses_with_handler_finally() {
        // with h handle Io { x; finally => cleanup() }
        // Simplified as: with h handle e { i; finally => i }
        let tokens = vec![
            tok(paideia_as_lexer::TokenKind::KwWith, 0, 4),
            tok(paideia_as_lexer::TokenKind::Ident, 5, 1), // h
            tok(paideia_as_lexer::TokenKind::KwHandle, 7, 6),
            tok(paideia_as_lexer::TokenKind::Ident, 14, 1), // e
            tok(paideia_as_lexer::TokenKind::LBrace, 16, 1),
            tok(paideia_as_lexer::TokenKind::Ident, 17, 1), // i
            tok(paideia_as_lexer::TokenKind::Semicolon, 18, 1),
            tok(paideia_as_lexer::TokenKind::KwFinally, 20, 7),
            tok(paideia_as_lexer::TokenKind::FatArrow, 28, 2),
            tok(paideia_as_lexer::TokenKind::Ident, 31, 1), // i
            tok(paideia_as_lexer::TokenKind::RBrace, 32, 1),
            tok(paideia_as_lexer::TokenKind::Eof, 33, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_with_handler();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprWithHandler);

        if let Some(paideia_as_ast::ExprData::WithHandler { finally, .. }) =
            arena.expr_data(expr_id)
        {
            assert!(finally.is_some(), "finally clause should be present");
        } else {
            panic!("expected WithHandler variant");
        }
    }

    #[test]
    fn finally_must_be_last_emits_p0162() {
        // with h handle e { finally => i; x }  <- something after finally
        let tokens = vec![
            tok(paideia_as_lexer::TokenKind::KwWith, 0, 4),
            tok(paideia_as_lexer::TokenKind::Ident, 5, 1),
            tok(paideia_as_lexer::TokenKind::KwHandle, 7, 6),
            tok(paideia_as_lexer::TokenKind::Ident, 14, 1),
            tok(paideia_as_lexer::TokenKind::LBrace, 16, 1),
            tok(paideia_as_lexer::TokenKind::KwFinally, 17, 7),
            tok(paideia_as_lexer::TokenKind::FatArrow, 25, 2),
            tok(paideia_as_lexer::TokenKind::Ident, 28, 1),
            tok(paideia_as_lexer::TokenKind::Semicolon, 29, 1),
            tok(paideia_as_lexer::TokenKind::Ident, 31, 1),
            tok(paideia_as_lexer::TokenKind::Eof, 32, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_with_handler();
        assert!(result.is_err());
        assert!(
            sink.diagnostics().iter().any(|d| d.code().number() == 162),
            "expected P0162 diagnostic"
        );
    }

    #[test]
    fn with_handler_without_finally_unchanged() {
        // with h handle e { i }  <- no finally
        let tokens = vec![
            tok(paideia_as_lexer::TokenKind::KwWith, 0, 4),
            tok(paideia_as_lexer::TokenKind::Ident, 5, 1),
            tok(paideia_as_lexer::TokenKind::KwHandle, 7, 6),
            tok(paideia_as_lexer::TokenKind::Ident, 14, 1),
            tok(paideia_as_lexer::TokenKind::LBrace, 16, 1),
            tok(paideia_as_lexer::TokenKind::Ident, 17, 1),
            tok(paideia_as_lexer::TokenKind::RBrace, 18, 1),
            tok(paideia_as_lexer::TokenKind::Eof, 19, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_with_handler();
        assert!(result.is_ok());
        let expr_id = result.unwrap();
        if let Some(paideia_as_ast::ExprData::WithHandler { finally, .. }) =
            arena.expr_data(expr_id)
        {
            assert!(finally.is_none(), "finally clause should be absent");
        } else {
            panic!("expected WithHandler variant");
        }
    }

    #[test]
    fn nested_perform_in_with() {
        // with h handle e { let s = perform Io::port_read(0x64); s }
        let tokens = vec![
            tok(paideia_as_lexer::TokenKind::KwWith, 0, 4),
            tok(paideia_as_lexer::TokenKind::Ident, 5, 1),
            tok(paideia_as_lexer::TokenKind::KwHandle, 7, 6),
            tok(paideia_as_lexer::TokenKind::Ident, 14, 1),
            tok(paideia_as_lexer::TokenKind::LBrace, 16, 1),
            tok(paideia_as_lexer::TokenKind::KwLet, 17, 3),
            tok(paideia_as_lexer::TokenKind::Ident, 21, 1), // s
            tok(paideia_as_lexer::TokenKind::Assign, 23, 1),
            tok(paideia_as_lexer::TokenKind::KwPerform, 25, 7),
            tok(paideia_as_lexer::TokenKind::Ident, 33, 2), // Io
            tok(paideia_as_lexer::TokenKind::ColonColon, 35, 2),
            tok(paideia_as_lexer::TokenKind::Ident, 37, 9), // port_read
            tok(paideia_as_lexer::TokenKind::LParen, 46, 1),
            tok(paideia_as_lexer::TokenKind::IntLit, 47, 3), // 0x64
            tok(paideia_as_lexer::TokenKind::RParen, 50, 1),
            tok(paideia_as_lexer::TokenKind::Semicolon, 51, 1),
            tok(paideia_as_lexer::TokenKind::Ident, 53, 1), // s
            tok(paideia_as_lexer::TokenKind::RBrace, 54, 1),
            tok(paideia_as_lexer::TokenKind::Eof, 55, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

        let result = parser.parse_with_handler();
        assert!(result.is_ok(), "nested perform should parse");
        let expr_id = result.unwrap();
        let node = arena.get(expr_id).unwrap();
        assert_eq!(node.kind, NodeKind::ExprWithHandler);
    }
}
