//! Statement parsing: let, return, and expression statements.
//!
//! Implements §8 Stmt grammar: let bindings, return expressions, and
//! general expression statements. Instruction statements (mnemonic Operand*)
//! are deferred to a later phase (PR-26+).

use paideia_as_ast::{NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a statement: let binding, return, or expression.
    ///
    /// **Algorithm:**
    /// 1. If current token is `KwLet`: parse `let Pattern (: Type)? = Expr ;`
    ///    Allocate `StmtData::Let { name, ty, value }`.
    /// 2. If current token is `KwReturn`: parse `return Expr? ;`
    ///    Allocate `StmtData::Return { value }`.
    /// 3. Otherwise: parse expression via `parse_expr()`, consume optional `;`,
    ///    allocate `StmtData::Expr { expr }`.
    ///
    /// Note: Instruction statements (mnemonic Operand*, e.g., `mov rax, [bar]`)
    /// are deferred to PR-26+. For phase-1, such forms parse as expression
    /// statements where the mnemonic is an identifier-shaped path.
    ///
    /// Returns the `NodeId` of the allocated statement on success.
    pub(crate) fn parse_stmt(&mut self) -> Result<NodeId, ParseError> {
        if self.at(TokenKind::KwLet) {
            self.parse_let_stmt()
        } else if self.at(TokenKind::KwReturn) {
            self.parse_return_stmt()
        } else {
            self.parse_expr_stmt()
        }
    }

    /// Parse a let statement: `let Pat (: Type)? = Expr ;`
    fn parse_let_stmt(&mut self) -> Result<NodeId, ParseError> {
        let let_tok = self.expect(TokenKind::KwLet)?;
        let let_span = let_tok.span;

        // Parse pattern (for now, just an identifier)
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Optional type annotation
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect `=`
        self.expect(TokenKind::Assign)?;

        // Parse the value expression
        let value = self.parse_expr()?;

        // Expect optional `;` (or it's the end of block)
        self.eat(TokenKind::Semicolon);

        let rbrace_span = self.peek().map(|tok| tok.span).unwrap_or(let_span);
        let stmt_span = Span::new(
            let_span.file(),
            let_span.byte_start(),
            rbrace_span.byte_start() - let_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_stmt(
            NodeKind::StmtLet,
            stmt_span,
            StmtData::Let {
                name: name_id,
                ty,
                value,
            },
        ))
    }

    /// Parse a return statement: `return Expr? ;`
    fn parse_return_stmt(&mut self) -> Result<NodeId, ParseError> {
        let ret_tok = self.expect(TokenKind::KwReturn)?;
        let ret_span = ret_tok.span;

        // Check if there's a return value or we're at end of statement
        let value = if !self.at(TokenKind::Semicolon) && !self.at(TokenKind::RBrace) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        // Expect optional `;`
        self.eat(TokenKind::Semicolon);

        let end_span = self.peek().map(|tok| tok.span).unwrap_or(ret_span);
        let stmt_span = Span::new(
            ret_span.file(),
            ret_span.byte_start(),
            end_span.byte_start() - ret_span.byte_start(),
        );

        Ok(self
            .arena_mut()
            .alloc_stmt(NodeKind::StmtReturn, stmt_span, StmtData::Return { value }))
    }

    /// Parse an expression statement: `Expr ;?`
    fn parse_expr_stmt(&mut self) -> Result<NodeId, ParseError> {
        let expr_start = self.peek().map(|tok| tok.span).ok_or(ParseError)?;

        let expr = self.parse_expr()?;

        // Consume optional `;`
        self.eat(TokenKind::Semicolon);

        let expr_span = self
            .arena()
            .get(expr)
            .map(|nd| nd.span)
            .unwrap_or(expr_start);

        Ok(self
            .arena_mut()
            .alloc_stmt(NodeKind::StmtExpr, expr_span, StmtData::Expr { expr }))
    }
}

// Tests will be in integration tests; parse_stmt is internal to the parser module.
