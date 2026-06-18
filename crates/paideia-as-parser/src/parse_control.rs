//! Control flow expression parsing (if, loop, block).
//!
//! Implements §8 grammar for:
//! - `IfExpr`: `if cond { ... } else { ... }` with optional else-if chaining.
//! - `LoopExpr`: `loop { ... }`, `while cond { ... }`, `for pat in iter { ... }`.
//! - `BlockExpr`: `{ stmts; expr? }` with optional tail expression.

use paideia_as_ast::{ExprData, LoopKind, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse an if expression.
    ///
    /// Form: `if cond { then-block } else { else-block }` or just `if cond { then-block }`.
    /// The else-block can itself be another if (else-if chain).
    /// Returns a `NodeKind::ExprIf`.
    pub(crate) fn parse_if(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let if_tok = self.expect(TokenKind::KwIf)?;
        let if_span = if_tok.span;

        // Parse condition
        let cond = self.parse_expr()?;

        // Parse then-block
        let then_block = self.parse_block()?;

        // Optional else clause
        let else_block = if self.at(TokenKind::KwElse) {
            self.bump(); // consume `else`

            // Check for else-if or else block
            if self.at(TokenKind::KwIf) {
                // Else-if: recursively parse as another if expression
                Some(self.parse_if()?)
            } else {
                // Else block
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        // Compute span
        let last_span = else_block
            .and_then(|id| self.arena().get(id).map(|nd| nd.span))
            .or_else(|| self.arena().get(then_block).map(|nd| nd.span))
            .unwrap_or(if_span);
        let if_span_full = Span::new(
            if_span.file(),
            if_span.byte_start(),
            last_span.byte_start() + last_span.byte_len() - if_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprIf,
            if_span_full,
            ExprData::If {
                cond,
                then_block,
                else_block,
            },
        ))
    }

    /// Parse a loop expression.
    ///
    /// Dispatches on KwLoop, KwWhile, or KwFor.
    /// - `loop { body }`: `LoopKind::Loop`, header=None.
    /// - `while cond { body }`: `LoopKind::While`, header=Some(cond).
    /// - `for pat in iter { body }`: `LoopKind::For`, header=Some(iter); pattern is currently not stored (TODO).
    ///
    /// Returns a `NodeKind::ExprLoop`.
    pub(crate) fn parse_loop_form(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        if let Some(tok) = self.peek() {
            match tok.kind {
                TokenKind::KwLoop => self.parse_loop_infinite(),
                TokenKind::KwWhile => self.parse_loop_while(),
                TokenKind::KwFor => self.parse_loop_for(),
                _ => Err(ParseError), // Should not be called for non-loop tokens
            }
        } else {
            Err(ParseError)
        }
    }

    /// Parse infinite loop: `loop { body }`.
    fn parse_loop_infinite(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let loop_tok = self.expect(TokenKind::KwLoop)?;
        let loop_span = loop_tok.span;

        let body = self.parse_block()?;

        let body_span = self
            .arena()
            .get(body)
            .map(|nd| nd.span)
            .unwrap_or(loop_span);
        let loop_span_full = Span::new(
            loop_span.file(),
            loop_span.byte_start(),
            body_span.byte_start() + body_span.byte_len() - loop_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLoop,
            loop_span_full,
            ExprData::Loop {
                kind: LoopKind::Loop,
                header: None,
                body,
            },
        ))
    }

    /// Parse conditional loop: `while cond { body }`.
    fn parse_loop_while(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let while_tok = self.expect(TokenKind::KwWhile)?;
        let while_span = while_tok.span;

        let cond = self.parse_expr()?;
        let body = self.parse_block()?;

        let body_span = self
            .arena()
            .get(body)
            .map(|nd| nd.span)
            .unwrap_or(while_span);
        let loop_span_full = Span::new(
            while_span.file(),
            while_span.byte_start(),
            body_span.byte_start() + body_span.byte_len() - while_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLoop,
            loop_span_full,
            ExprData::Loop {
                kind: LoopKind::While,
                header: Some(cond),
                body,
            },
        ))
    }

    /// Parse iterative loop: `for pat in iter { body }`.
    ///
    /// **TODO (phase-2)**: Currently only the iterator expression is parsed and stored.
    /// The pattern is parsed but not attached to the loop node. A future PR will
    /// extend the Loop variant to carry the pattern separately or wrap it in header.
    fn parse_loop_for(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let for_tok = self.expect(TokenKind::KwFor)?;
        let for_span = for_tok.span;

        // Parse pattern (currently discarded in phase-1)
        let _pattern = self.parse_for_pattern()?;

        // Expect `in`
        self.expect(TokenKind::KwIn)?;

        // Parse iterator expression
        let iter_expr = self.parse_expr()?;

        // Parse body
        let body = self.parse_block()?;

        let body_span = self.arena().get(body).map(|nd| nd.span).unwrap_or(for_span);
        let loop_span_full = Span::new(
            for_span.file(),
            for_span.byte_start(),
            body_span.byte_start() + body_span.byte_len() - for_span.byte_start(),
        );

        // For now, store the iterator as the header. The pattern is parsed but
        // discarded. A future PR will properly attach the pattern.
        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprLoop,
            loop_span_full,
            ExprData::Loop {
                kind: LoopKind::For,
                header: Some(iter_expr),
                body,
            },
        ))
    }

    /// Parse a pattern for `for` loops (simplified for phase-1).
    fn parse_for_pattern(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        if let Some(tok) = self.peek() {
            if tok.kind == TokenKind::Ident {
                let ident_tok = self.bump().unwrap();
                let ident_id = self.arena_mut().alloc(NodeKind::Ident, ident_tok.span);
                Ok(self.arena_mut().alloc_pattern(
                    NodeKind::PatIdent,
                    ident_tok.span,
                    paideia_as_ast::PatternData::Ident {
                        name: ident_id,
                        mutable: false,
                    },
                ))
            } else {
                Err(ParseError)
            }
        } else {
            Err(ParseError)
        }
    }

    /// Parse a block expression.
    ///
    /// Form: `{ stmt1; stmt2; expr? }`.
    /// Statements are expressions followed by `;`.
    /// The last expression (if not followed by `;`) is the tail expression.
    /// Returns a `NodeKind::ExprBlock`.
    pub(crate) fn parse_block(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lbrace_tok = self.expect(TokenKind::LBrace)?;
        let lbrace_span = lbrace_tok.span;

        let mut stmts = Vec::new();
        let mut tail = None;

        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Parse one expression
            let expr = self.parse_expr()?;

            // Check for semicolon
            if self.at(TokenKind::Semicolon) {
                self.bump(); // consume `;`
                // Get the span before mutably borrowing arena
                let expr_span = self
                    .arena()
                    .get(expr)
                    .map(|nd| nd.span)
                    .unwrap_or(lbrace_span);
                // Wrap as a statement
                let stmt = self.arena_mut().alloc_stmt(
                    NodeKind::StmtExpr,
                    expr_span,
                    paideia_as_ast::StmtData::Expr { expr },
                );
                stmts.push(stmt);
            } else {
                // No semicolon: this is the tail expression
                tail = Some(expr);
                // Next iteration should see RBrace
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

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprBlock,
            block_span,
            ExprData::Block { stmts, tail },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::{Token, TokenKind};

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    fn parse(
        tokens: Vec<Token>,
    ) -> (
        AstArena,
        paideia_as_ast::NodeId,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr().expect("parse failed")
        };
        let diags = sink.diagnostics().to_vec();
        (arena, root, diags)
    }

    #[test]
    fn if_simple() {
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),  // if
            tok(TokenKind::Ident, 3, 4), // cond
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1), // 1
            tok(TokenKind::RBrace, 11, 1),
            tok(TokenKind::Eof, 12, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::If { else_block, .. } = expr_data {
                assert!(else_block.is_none());
            } else {
                panic!("expected ExprIf");
            }
        }
    }

    #[test]
    fn if_else() {
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),  // if
            tok(TokenKind::Ident, 3, 1), // a
            tok(TokenKind::LBrace, 5, 1),
            tok(TokenKind::IntLit, 7, 1), // 1
            tok(TokenKind::RBrace, 8, 1),
            tok(TokenKind::KwElse, 10, 4), // else
            tok(TokenKind::LBrace, 15, 1),
            tok(TokenKind::IntLit, 17, 1), // 2
            tok(TokenKind::RBrace, 18, 1),
            tok(TokenKind::Eof, 19, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::If { else_block, .. } = expr_data {
                assert!(else_block.is_some());
            } else {
                panic!("expected ExprIf");
            }
        }
    }

    #[test]
    fn if_else_if_chain() {
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2), // if a
            tok(TokenKind::Ident, 3, 1),
            tok(TokenKind::LBrace, 5, 1),
            tok(TokenKind::IntLit, 7, 1), // 1
            tok(TokenKind::RBrace, 8, 1),
            tok(TokenKind::KwElse, 10, 4), // else if b
            tok(TokenKind::KwIf, 15, 2),
            tok(TokenKind::Ident, 18, 1),
            tok(TokenKind::LBrace, 20, 1),
            tok(TokenKind::IntLit, 22, 1), // 2
            tok(TokenKind::RBrace, 23, 1),
            tok(TokenKind::KwElse, 25, 4), // else
            tok(TokenKind::LBrace, 30, 1),
            tok(TokenKind::IntLit, 32, 1), // 3
            tok(TokenKind::RBrace, 33, 1),
            tok(TokenKind::Eof, 34, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);

        // Check that else_block is itself an If node
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::If {
                else_block: Some(else_id),
                ..
            } = expr_data
            {
                let else_node = arena.get(*else_id).unwrap();
                assert_eq!(else_node.kind, NodeKind::ExprIf);
            } else {
                panic!("expected ExprIf with else block");
            }
        }
    }

    #[test]
    fn loop_form() {
        let tokens = vec![
            tok(TokenKind::KwLoop, 0, 4), // loop
            tok(TokenKind::LBrace, 5, 1),
            tok(TokenKind::IntLit, 7, 1), // 1
            tok(TokenKind::RBrace, 8, 1),
            tok(TokenKind::Eof, 9, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLoop);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Loop { kind, header, .. } = expr_data {
                assert_eq!(*kind, LoopKind::Loop);
                assert!(header.is_none());
            } else {
                panic!("expected ExprLoop");
            }
        }
    }

    #[test]
    fn while_form() {
        let tokens = vec![
            tok(TokenKind::KwWhile, 0, 5), // while
            tok(TokenKind::Ident, 6, 4),   // cond
            tok(TokenKind::LBrace, 11, 1),
            tok(TokenKind::IntLit, 13, 1), // 1
            tok(TokenKind::RBrace, 14, 1),
            tok(TokenKind::Eof, 15, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLoop);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Loop { kind, header, .. } = expr_data {
                assert_eq!(*kind, LoopKind::While);
                assert!(header.is_some());
            } else {
                panic!("expected ExprLoop");
            }
        }
    }

    #[test]
    fn block_with_statements_and_tail() {
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::Ident, 2, 1), // a
            tok(TokenKind::Semicolon, 3, 1),
            tok(TokenKind::Ident, 5, 1), // b
            tok(TokenKind::RBrace, 6, 1),
            tok(TokenKind::Eof, 7, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Block { stmts, tail } = expr_data {
                assert_eq!(stmts.len(), 1, "one statement expected");
                assert!(tail.is_some(), "tail expression expected");
            } else {
                panic!("expected ExprBlock");
            }
        }
    }

    #[test]
    fn empty_block() {
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::RBrace, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Block { stmts, tail } = expr_data {
                assert_eq!(stmts.len(), 0);
                assert!(tail.is_none());
            } else {
                panic!("expected ExprBlock");
            }
        }
    }
}
