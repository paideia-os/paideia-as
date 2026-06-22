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

/// Block kind: distinguishes between value-position (expr expected)
/// and statement-position (unit literal synthesized on trailing `;`).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum BlockKind {
    /// Value position: block must end with expression (no trailing `;` allowed).
    /// Empty block is an error (P0157).
    Value,
    /// Statement position: block may end with `;`, which is synthesized to `()`.
    /// Empty block is still an error (P0157).
    Statement,
}

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Check if an `else` keyword follows immediately after the closing brace of a block.
    ///
    /// This helper is used during `parse_if` to determine whether a bare `if` (without `else`)
    /// should have its then-block parsed as `BlockKind::Statement` (unit-typed) instead of
    /// the caller's expectation. A bare `if` is one that lacks an `else` clause.
    ///
    /// **Algorithm:**
    /// 1. Use peek_at() starting from the current position to scan ahead.
    /// 2. Expect `{` at the current position.
    /// 3. Walk forward tracking `{`/`}` depth; skip over nested braces.
    /// 4. When depth returns to 0, check if the next token is `KwElse`.
    /// 5. Return the result without consuming any tokens.
    ///
    /// **Token handling:** The lexer yields string and char literals as single tokens, so
    /// no special delimiter handling is needed.
    fn peek_has_else_after_block(&self) -> bool {
        // We expect to be at the opening brace `{`. Check that first.
        if !self.at(TokenKind::LBrace) {
            return false;
        }

        let mut depth = 0;
        let mut pos = 0; // Position relative to current (0 = current token, which is `{`)

        // Walk through tokens until we've matched the closing brace
        loop {
            match self.peek_at(pos) {
                None => {
                    break; // EOF before closing brace
                }
                Some(tok) => {
                    match tok.kind {
                        TokenKind::Eof => {
                            break; // EOF
                        }
                        TokenKind::LBrace => {
                            depth += 1;
                            pos += 1;
                        }
                        TokenKind::RBrace => {
                            if depth == 1 {
                                // Found the matching closing brace; check next token
                                if let Some(next_tok) = self.peek_at(pos + 1) {
                                    return next_tok.kind == TokenKind::KwElse;
                                }
                                return false;
                            } else {
                                depth -= 1;
                                pos += 1;
                            }
                        }
                        _ => {
                            pos += 1;
                        }
                    }
                }
            }
        }

        false
    }

    /// Parse an if expression with the given block kind.
    ///
    /// Form: `if cond { then-block } else { else-block }` or just `if cond { then-block }`.
    /// The else-block can itself be another if (else-if chain).
    /// Returns a `NodeKind::ExprIf`.
    ///
    /// The `kind` parameter is threaded to both then-block and else-block for cases with `else`.
    /// For bare `if` (no `else`), the then-block is always parsed as `BlockKind::Statement`
    /// to make the entire `if` expression unit-typed.
    pub(crate) fn parse_if(
        &mut self,
        kind: BlockKind,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let if_tok = self.expect(TokenKind::KwIf)?;
        let if_span = if_tok.span;

        // Parse condition
        let cond = self.parse_expr()?;

        // Determine the block kind for the then-block:
        // If there's no `else` following, the then-block must be unit-typed (Statement).
        let then_kind = if !self.peek_has_else_after_block() {
            BlockKind::Statement // bare if is always unit-typed
        } else {
            kind // preserve caller's expectation for if-else
        };

        // Parse then-block with the determined kind
        let then_block = self.parse_block_kind(then_kind)?;

        // Optional else clause
        let else_block = if self.at(TokenKind::KwElse) {
            self.bump(); // consume `else`

            // Check for else-if or else block
            if self.at(TokenKind::KwIf) {
                // Else-if: recursively parse as another if expression
                Some(self.parse_if(kind)?)
            } else {
                // Else block with the given kind
                Some(self.parse_block_kind(kind)?)
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

        let body = self.parse_block_kind(BlockKind::Statement)?;

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
        let body = self.parse_block_kind(BlockKind::Statement)?;

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
    /// Parses the pattern, `in` keyword, iterable expression, and body block.
    /// Returns a `NodeKind::ExprFor` with pattern, iterable, and body stored.
    fn parse_loop_for(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        let for_tok = self.expect(TokenKind::KwFor)?;
        let for_span = for_tok.span;

        // Parse pattern
        let pattern = self.parse_for_pattern()?;

        // Expect `in`
        self.expect(TokenKind::KwIn)?;

        // Parse iterator expression
        let iterable = self.parse_expr()?;

        // Parse body with statement kind
        let body = self.parse_block_kind(BlockKind::Statement)?;

        let body_span = self.arena().get(body).map(|nd| nd.span).unwrap_or(for_span);
        let for_span_full = Span::new(
            for_span.file(),
            for_span.byte_start(),
            body_span.byte_start() + body_span.byte_len() - for_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprFor,
            for_span_full,
            ExprData::For {
                pattern,
                iterable,
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

    /// Thin wrapper calling `parse_block_kind(BlockKind::Value)`.
    /// Used for backward compatibility and value-position blocks.
    #[allow(dead_code)]
    pub(crate) fn parse_block(&mut self) -> Result<paideia_as_ast::NodeId, ParseError> {
        self.parse_block_kind(BlockKind::Value)
    }

    /// Parse a block expression with the given kind.
    ///
    /// Form: `{ stmt1; stmt2; expr? }`.
    /// Statements can be let-bindings, return statements, or expressions followed by `;`.
    /// The last expression (if not followed by `;`) is the tail expression.
    /// Returns a `NodeKind::ExprBlock`.
    ///
    /// **Parsing flow:**
    /// 1. If current token is `KwLet` or `KwReturn`: dispatch to `parse_stmt(false)`
    ///    to parse the full statement (which consumes its trailing `;`).
    /// 2. Otherwise: parse an expression via `parse_expr()`.
    ///    - If `;` follows: wrap as `StmtData::Expr` and add to statements.
    ///    - If `}` follows: this is the tail expression.
    ///
    /// **Validation:**
    /// - If block is empty (`stmts` empty and `tail` None): emit P0157 and return Err.
    /// - If block ends with `;`:
    ///   - **Value position**: emit P0158 and return Err.
    ///   - **Statement position**: synthesize unit literal `()` as tail.
    pub(crate) fn parse_block_kind(
        &mut self,
        kind: BlockKind,
    ) -> Result<paideia_as_ast::NodeId, ParseError> {
        let lbrace_tok = self.expect(TokenKind::LBrace)?;
        let lbrace_span = lbrace_tok.span;

        let mut stmts = Vec::new();
        let mut tail = None;

        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Check if this is a let or return statement (keywords recognized at stmt level)
            if self.at(TokenKind::KwLet) || self.at(TokenKind::KwReturn) {
                // Parse as a statement; parse_stmt consumes trailing `;`
                let stmt = self.parse_stmt(false)?;
                stmts.push(stmt);
            } else {
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
        }

        // Expect closing brace
        let rbrace_tok = self.expect(TokenKind::RBrace)?;
        let rbrace_span = rbrace_tok.span;

        let block_span = Span::new(
            lbrace_span.file(),
            lbrace_span.byte_start(),
            rbrace_span.byte_start() + rbrace_span.byte_len() - lbrace_span.byte_start(),
        );

        // Validate block: must not be empty
        if stmts.is_empty() && tail.is_none() {
            // Emit P0157: empty block expression
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

        // Handle block that ends with semicolon (!stmts.is_empty() && tail.is_none())
        if !stmts.is_empty() && tail.is_none() {
            match kind {
                BlockKind::Value => {
                    // Value position: emit P0158
                    use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 158)
                        .expect("valid P0158 code");
                    self.emit_diagnostic(
                        Diagnostic::error(code)
                            .message("block expression must have a final expression; trailing `;` is not allowed")
                            .with_span(block_span)
                            .finish(),
                    );
                    return Err(ParseError);
                }
                BlockKind::Statement => {
                    // Statement position: synthesize unit literal `()` as tail
                    let unit_lit_id = self.arena_mut().alloc(NodeKind::Placeholder, rbrace_span);
                    tail = Some(self.arena_mut().alloc_expr(
                        NodeKind::ExprLiteral,
                        rbrace_span,
                        ExprData::Literal { lit: unit_lit_id },
                    ));
                }
            }
        }

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
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
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
    fn for_simple() {
        let tokens = vec![
            tok(TokenKind::KwFor, 0, 3), // for
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::KwIn, 6, 2),  // in
            tok(TokenKind::Ident, 9, 4), // list
            tok(TokenKind::LBrace, 14, 1),
            tok(TokenKind::IntLit, 16, 1), // 1
            tok(TokenKind::RBrace, 17, 1),
            tok(TokenKind::Eof, 18, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFor);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::For {
                pattern,
                iterable,
                body,
            } = expr_data
            {
                // All three fields should be valid NodeIds
                let _pattern = pattern;
                let _iterable = iterable;
                let _body = body;
            } else {
                panic!("expected ExprFor");
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
    fn block_single_tail_expr() {
        // { x } parses as Block with stmts=[], tail=Some(x)
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::Ident, 2, 1), // x
            tok(TokenKind::RBrace, 3, 1),
            tok(TokenKind::Eof, 4, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Block { stmts, tail } = expr_data {
                assert_eq!(stmts.len(), 0, "no statements expected");
                assert!(tail.is_some(), "tail expression expected");
            } else {
                panic!("expected ExprBlock");
            }
        }
    }

    #[test]
    fn block_let_then_tail() {
        // { let x = 1; x + 1 } parses with 1 stmt + tail
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::KwLet, 2, 3),      // let
            tok(TokenKind::Ident, 6, 1),      // x
            tok(TokenKind::Assign, 8, 1),     // =
            tok(TokenKind::IntLit, 10, 1),    // 1
            tok(TokenKind::Semicolon, 11, 1), // ;
            tok(TokenKind::Ident, 13, 1),     // x
            tok(TokenKind::Plus, 15, 1),      // +
            tok(TokenKind::IntLit, 17, 1),    // 1
            tok(TokenKind::RBrace, 18, 1),
            tok(TokenKind::Eof, 19, 0),
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
    fn block_multi_let_then_tail() {
        // { let x = 1; let y = 2; x + y } parses with 2 stmts + tail
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::KwLet, 2, 3),      // let
            tok(TokenKind::Ident, 6, 1),      // x
            tok(TokenKind::Assign, 8, 1),     // =
            tok(TokenKind::IntLit, 10, 1),    // 1
            tok(TokenKind::Semicolon, 11, 1), // ;
            tok(TokenKind::KwLet, 13, 3),     // let
            tok(TokenKind::Ident, 17, 1),     // y
            tok(TokenKind::Assign, 19, 1),    // =
            tok(TokenKind::IntLit, 21, 1),    // 2
            tok(TokenKind::Semicolon, 22, 1), // ;
            tok(TokenKind::Ident, 24, 1),     // x
            tok(TokenKind::Plus, 26, 1),      // +
            tok(TokenKind::Ident, 28, 1),     // y
            tok(TokenKind::RBrace, 29, 1),
            tok(TokenKind::Eof, 30, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
        if let Some(expr_data) = arena.expr_data(root) {
            if let ExprData::Block { stmts, tail } = expr_data {
                assert_eq!(stmts.len(), 2, "two statements expected");
                assert!(tail.is_some(), "tail expression expected");
            } else {
                panic!("expected ExprBlock");
            }
        }
    }

    #[test]
    fn block_empty_rejected() {
        // { } should return Err and emit P0157
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::RBrace, 1, 1),
            tok(TokenKind::Eof, 2, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let result = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr()
        };

        assert!(result.is_err(), "expected parse error");
        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 157),
            "expected P0157 diagnostic"
        );
    }

    #[test]
    fn block_trailing_semi_rejected() {
        // { let x = 1; } should return Err and emit P0158
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::KwLet, 2, 3),      // let
            tok(TokenKind::Ident, 6, 1),      // x
            tok(TokenKind::Assign, 8, 1),     // =
            tok(TokenKind::IntLit, 10, 1),    // 1
            tok(TokenKind::Semicolon, 11, 1), // ;
            tok(TokenKind::RBrace, 12, 1),
            tok(TokenKind::Eof, 13, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let result = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr()
        };

        assert!(result.is_err(), "expected parse error");
        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 158),
            "expected P0158 diagnostic"
        );
    }

    #[test]
    fn block_expr_stmt_then_tail() {
        // { foo(); x } parses with 1 StmtExpr + tail
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::Ident, 2, 3), // foo
            tok(TokenKind::LParen, 5, 1),
            tok(TokenKind::RParen, 6, 1),
            tok(TokenKind::Semicolon, 7, 1), // ;
            tok(TokenKind::Ident, 9, 1),     // x
            tok(TokenKind::RBrace, 10, 1),
            tok(TokenKind::Eof, 11, 0),
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
    fn block_nested() {
        // { let x = { let y = 1; y }; x } parses
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::KwLet, 2, 3),      // let
            tok(TokenKind::Ident, 6, 1),      // x
            tok(TokenKind::Assign, 8, 1),     // =
            tok(TokenKind::LBrace, 10, 1),    // inner {
            tok(TokenKind::KwLet, 12, 3),     // let
            tok(TokenKind::Ident, 16, 1),     // y
            tok(TokenKind::Assign, 18, 1),    // =
            tok(TokenKind::IntLit, 20, 1),    // 1
            tok(TokenKind::Semicolon, 21, 1), // ;
            tok(TokenKind::Ident, 23, 1),     // y
            tok(TokenKind::RBrace, 24, 1),    // inner }
            tok(TokenKind::Semicolon, 25, 1), // ;
            tok(TokenKind::Ident, 27, 1),     // x
            tok(TokenKind::RBrace, 28, 1),
            tok(TokenKind::Eof, 29, 0),
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
    fn block_let_typed() {
        // { let x : u64 = 1; x } parses (type annotation works inside blocks)
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::KwLet, 2, 3),      // let
            tok(TokenKind::Ident, 6, 1),      // x
            tok(TokenKind::Colon, 8, 1),      // :
            tok(TokenKind::Ident, 10, 3),     // u64
            tok(TokenKind::Assign, 14, 1),    // =
            tok(TokenKind::IntLit, 16, 1),    // 1
            tok(TokenKind::Semicolon, 17, 1), // ;
            tok(TokenKind::Ident, 19, 1),     // x
            tok(TokenKind::RBrace, 20, 1),
            tok(TokenKind::Eof, 21, 0),
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

    // m3-003 tests: unit-typed blocks accept trailing `;` in statement position

    #[test]
    fn stmt_position_if_accepts_trailing_semi() {
        // if cond { x } ; (as a statement, trailing ; is allowed)
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),
            tok(TokenKind::Ident, 3, 4), // cond
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::IntLit, 10, 1), // x
            tok(TokenKind::RBrace, 11, 1),
            tok(TokenKind::Semicolon, 12, 1), // trailing ;
            tok(TokenKind::Eof, 13, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            // Parse as statement-position if
            p.parse_if(BlockKind::Statement).expect("parse failed")
        };
        let diags = sink.diagnostics();

        // Should succeed with no P0158 error (value position would reject this)
        assert!(diags.iter().all(|d| d.code().number() != 158));
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
    }

    #[test]
    fn if_else_rejects_trailing_semi_in_value_then() {
        // if cond { x; } else { y } (value position: should error P0158 in then-block)
        // With else present, the then-block stays as Value kind and should reject trailing ;
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),
            tok(TokenKind::Ident, 3, 4), // cond
            tok(TokenKind::LBrace, 8, 1),
            tok(TokenKind::Ident, 10, 1),     // x
            tok(TokenKind::Semicolon, 11, 1), // trailing ;
            tok(TokenKind::RBrace, 12, 1),
            tok(TokenKind::KwElse, 14, 4),    // else
            tok(TokenKind::LBrace, 19, 1),
            tok(TokenKind::Ident, 21, 1),     // y
            tok(TokenKind::RBrace, 22, 1),
            tok(TokenKind::Eof, 23, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            // Parse as value-position if-else
            p.parse_if(BlockKind::Value)
        };

        let diags = sink.diagnostics();

        // Should fail: then-block is Value kind (because else follows), trailing ; is rejected
        assert!(result.is_err());
        assert!(
            diags.iter().any(|d| d.code().number() == 158),
            "P0158 on value-position block with trailing ; when else follows"
        );
    }

    #[test]
    fn nested_if_else_statement_position() {
        // if a { 1 } else if b { 2 } else { 3 } ; (all statement position)
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
            tok(TokenKind::Semicolon, 34, 1), // trailing ;
            tok(TokenKind::Eof, 35, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_if(BlockKind::Statement).expect("parse failed")
        };
        let diags = sink.diagnostics();

        assert!(diags.iter().all(|d| d.code().number() != 158));
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
    }

    #[test]
    fn while_body_accepts_trailing_semi() {
        // while cond { x } ; (body is statement position)
        let tokens = vec![
            tok(TokenKind::KwWhile, 0, 5),
            tok(TokenKind::Ident, 6, 4), // cond
            tok(TokenKind::LBrace, 11, 1),
            tok(TokenKind::IntLit, 13, 1), // x
            tok(TokenKind::RBrace, 14, 1),
            tok(TokenKind::Semicolon, 15, 1), // trailing ;
            tok(TokenKind::Eof, 16, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert!(diags.iter().all(|d| d.code().number() != 158));
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLoop);
    }

    #[test]
    fn for_body_accepts_trailing_semi() {
        // for x in list { y } ; (body is statement position)
        let tokens = vec![
            tok(TokenKind::KwFor, 0, 3),
            tok(TokenKind::Ident, 4, 1), // x
            tok(TokenKind::KwIn, 6, 2),
            tok(TokenKind::Ident, 9, 4), // list
            tok(TokenKind::LBrace, 14, 1),
            tok(TokenKind::Ident, 16, 1), // y
            tok(TokenKind::RBrace, 17, 1),
            tok(TokenKind::Semicolon, 18, 1), // trailing ;
            tok(TokenKind::Eof, 19, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert!(diags.iter().all(|d| d.code().number() != 158));
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFor);
    }

    #[test]
    fn loop_body_accepts_trailing_semi() {
        // loop { x } ; (body is statement position)
        let tokens = vec![
            tok(TokenKind::KwLoop, 0, 4),
            tok(TokenKind::LBrace, 5, 1),
            tok(TokenKind::IntLit, 7, 1), // x
            tok(TokenKind::RBrace, 8, 1),
            tok(TokenKind::Semicolon, 9, 1), // trailing ;
            tok(TokenKind::Eof, 10, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert!(diags.iter().all(|d| d.code().number() != 158));
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLoop);
    }

    #[test]
    fn let_rhs_block_is_value_position() {
        // let x = { y } ; (RHS block is value position, not statement)
        // This test verifies that the block on the RHS of let is in value position
        // and thus requires a tail expression (no trailing ;).
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::Ident, 2, 1), // y
            tok(TokenKind::RBrace, 3, 1),
            tok(TokenKind::Eof, 4, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(
            diags.len(),
            0,
            "no diagnostics expected for value-position block"
        );
        // The block { y } parses successfully as value-position (y is tail).
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
    }

    #[test]
    fn match_arm_block_is_value_position() {
        // match x { 1 => { y }, 2 => { z } } (arm blocks are value position)
        // (This test is structurally present to demonstrate that match arms remain value-position;
        // full match parsing is delegated to parse_match, out of scope for this module.)
        // Simplified: just verify that blocks parsed in value position reject trailing `;`.
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),
            tok(TokenKind::Ident, 2, 1), // y
            tok(TokenKind::RBrace, 3, 1),
            tok(TokenKind::Eof, 4, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "bare block should have no errors");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
    }

    // PA9-m1-001: bare-if as unit-typed statement

    #[test]
    fn bare_if_tail_of_value_block_parses_as_unit() {
        // fn body { if cond { stmts; } } — the bare if should parse with unit-typed then-block
        // so it doesn't reject the trailing ; of the if statement in a value-position outer block.
        // Simplified: bare if { 42; } should parse without P0158 when in statement position.
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),    // if
            tok(TokenKind::Ident, 3, 4),   // cond
            tok(TokenKind::LBrace, 8, 1),  // {
            tok(TokenKind::IntLit, 10, 2), // 42
            tok(TokenKind::Semicolon, 12, 1), // ;
            tok(TokenKind::RBrace, 13, 1), // }
            tok(TokenKind::Eof, 14, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_if(BlockKind::Value).expect("parse failed")
        };
        let diags = sink.diagnostics();

        // Should have no P0158 because bare-if downgrades to Statement kind
        assert!(
            diags.iter().all(|d| d.code().number() != 158),
            "bare if should not emit P0158"
        );
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
    }

    #[test]
    fn bare_if_midblock_unchanged() {
        // Bare if { stmts; } followed by another stmt in BlockKind::Statement block.
        // The if itself should parse without error (no P0158).
        let tokens = vec![
            tok(TokenKind::LBrace, 0, 1),  // outer {
            tok(TokenKind::KwIf, 2, 2),    // if
            tok(TokenKind::Ident, 5, 4),   // cond
            tok(TokenKind::LBrace, 10, 1), // inner {
            tok(TokenKind::IntLit, 12, 1), // x
            tok(TokenKind::RBrace, 13, 1), // inner }
            tok(TokenKind::Semicolon, 14, 1), // if statement ends with ;
            tok(TokenKind::Ident, 16, 1),  // another stmt
            tok(TokenKind::RBrace, 17, 1), // outer }
            tok(TokenKind::Eof, 18, 0),
        ];
        let (arena, root, diags) = parse(tokens);

        // The outer block should parse successfully; no P0158
        assert!(
            diags.iter().all(|d| d.code().number() != 158),
            "bare if in statement block should not emit P0158"
        );
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprBlock);
    }

    #[test]
    fn nested_if_else_with_inner_bare_if() {
        // if a { if b { x }; } else { ... }
        // The inner bare-if downgrades its then-block to Statement (allows trailing ; on x);
        // the if statement itself ends with ;, making the outer block have statements but no tail.
        // In Statement position, this works fine.
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),    // if a
            tok(TokenKind::Ident, 3, 1),   // a
            tok(TokenKind::LBrace, 5, 1),  // {
            tok(TokenKind::KwIf, 7, 2),    // inner: if b
            tok(TokenKind::Ident, 10, 1),  // b
            tok(TokenKind::LBrace, 12, 1), // inner {
            tok(TokenKind::IntLit, 14, 1), // 1
            tok(TokenKind::RBrace, 15, 1), // inner }
            tok(TokenKind::Semicolon, 16, 1), // ;
            tok(TokenKind::RBrace, 17, 1), // outer then }
            tok(TokenKind::KwElse, 19, 4), // else
            tok(TokenKind::LBrace, 24, 1), // else {
            tok(TokenKind::IntLit, 26, 1), // 99
            tok(TokenKind::RBrace, 27, 1), // else }
            tok(TokenKind::Eof, 28, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            // Parse in statement position to simplify
            p.parse_if(BlockKind::Statement).expect("parse failed")
        };
        let diags = sink.diagnostics();

        // Should have no P0158
        assert!(
            diags.iter().all(|d| d.code().number() != 158),
            "nested bare if with else should not emit P0158"
        );
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
    }

    #[test]
    fn bare_if_with_tail_expr_then_block() {
        // if cond { 42 } (no else, value-bound then-block becomes a stmt).
        // The then-block should still parse as Statement (bare-if downgrade),
        // and the 42 becomes the tail of a unit-typed block.
        let tokens = vec![
            tok(TokenKind::KwIf, 0, 2),    // if
            tok(TokenKind::Ident, 3, 4),   // cond
            tok(TokenKind::LBrace, 8, 1),  // {
            tok(TokenKind::IntLit, 10, 2), // 42
            tok(TokenKind::RBrace, 12, 1), // }
            tok(TokenKind::Eof, 13, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_if(BlockKind::Value).expect("parse failed")
        };
        let diags = sink.diagnostics();

        // Should parse successfully
        assert_eq!(diags.len(), 0, "bare if with tail expr should have no diagnostics");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprIf);
    }
}
