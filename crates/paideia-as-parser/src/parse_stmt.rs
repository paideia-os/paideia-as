//! Statement parsing: let, return, expression, and instruction statements.
//!
//! Implements §8 Stmt grammar: let bindings, return expressions, expression
//! statements, and assembly instruction statements (mnemonic Operand*).
//!
//! **Deferred capabilities (documented per AC):
//! - §9.2 Continuation rule: Multi-line expressions are not yet supported.
//!   The lexer emits newlines as Trivia, not tokens, so statement-continuation
//!   detection requires lexer changes. For now, statements must be on a single
//!   line or separated by `;`.
//! - §9.3 Newline as statement separator: Currently relying on `;` separator.
//!   Newline handling will be added in a follow-up PR once the lexer exposes
//!   newlines as tokens.

use paideia_as_ast::{AtomicOrdering, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parse_control::BlockKind;
use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a statement: let binding, return, expression, or instruction.
    ///
    /// **Algorithm:**
    /// 1. If current token is `KwLet`: parse `let [mut] Pattern (: Type)? = Expr ;`
    ///    Allocate `StmtData::Let { mutable, name, ty, value }`.
    /// 2. If current token is `KwReturn`: parse `return Expr? ;`
    ///    Allocate `StmtData::Return { value }`.
    /// 3. If `in_action_context` is true and current token is `Ident` AND next token
    ///    exists and is `Ident`, `LBracket`, or `IntLit`: parse as instruction.
    ///    Allocate `StmtData::Instruction { mnemonic, operands }`.
    /// 4. Otherwise: parse expression via `parse_expr()`, consume optional `;`,
    ///    allocate `StmtData::Expr { expr }`.
    ///
    /// The `in_action_context` parameter controls whether instruction statements
    /// are recognized. Only within action blocks should this be true.
    ///
    /// **Dispatch heuristic for instructions (phase-1):**
    /// The heuristic at step 3 is position-sensitive but fragile. We peek at the
    /// next token to disambiguate: if Ident-Ident or Ident-LBracket, it looks
    /// like "mnemonic operand", so we try instruction parsing. This works for
    /// the common case but fails for edge cases like `foo ();` (which parse as
    /// expression statements). A more robust approach would require lookahead
    /// across the full operand list. This is acceptable for phase-1 and can be
    /// refined in later phases.
    ///
    /// Returns the `NodeId` of the allocated statement on success.
    pub(crate) fn parse_stmt(&mut self, in_action_context: bool) -> Result<NodeId, ParseError> {
        if self.at(TokenKind::KwLet) {
            self.parse_let_stmt()
        } else if self.at(TokenKind::KwReturn) {
            self.parse_return_stmt()
        } else if in_action_context && self.at(TokenKind::Ident) && self.peek_at(1).is_some() {
            // Heuristic for instruction statement: current is Ident, and next token exists.
            // If next is Ident, LBracket, IntLit, Semicolon, or RBrace, it looks like an instruction.
            // Semicolon and RBrace indicate zero-operand instructions (phase 6 m6-001).
            let next_kind = self.peek_at(1).map(|t| t.kind);
            if matches!(
                next_kind,
                Some(TokenKind::Ident)
                    | Some(TokenKind::LBracket)
                    | Some(TokenKind::IntLit)
                    | Some(TokenKind::Semicolon)
                    | Some(TokenKind::RBrace)
            ) {
                // Attempt to parse as instruction statement.
                self.parse_instruction_stmt()
            } else {
                self.parse_expr_stmt()
            }
        } else {
            self.parse_expr_stmt()
        }
    }

    /// Parse a let statement: `let [@atomic(Ordering)] [mut] Pat (: Type)? = Expr ;`
    ///
    /// paideia-as#1296 (phase 1, v0.21-003b): the optional `@atomic(Ordering)`
    /// binding-position attribute sits between `let` and the optional `mut`
    /// keyword. When present, it stamps the binding with a memory-ordering
    /// discipline (Relaxed / Acquire / Release / SeqCst) that a later
    /// elaborator phase honours by emitting fence-bracketed or lock-prefixed
    /// load/store sequences at every access site.
    fn parse_let_stmt(&mut self) -> Result<NodeId, ParseError> {
        let let_tok = self.expect(TokenKind::KwLet)?;
        let let_span = let_tok.span;

        // Optional `@atomic(Ordering)` binding-position attribute (paideia-as#1296).
        // Must come *before* the optional `mut` keyword so that the code-review
        // eye lands on the memory-ordering discipline immediately after `let`.
        let atomic = self.parse_optional_atomic_prefix()?;

        // Check for optional `mut` keyword
        let mutable = self.eat(TokenKind::KwMut);

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

        // Validation: if value is Uninit, check mutability
        if let Some(expr_data) = self.arena().expr_data(value) {
            if let paideia_as_ast::ExprData::Uninit = expr_data {
                if !mutable {
                    // uninit only valid for let mut
                    let expr_node = self.arena().get(value).expect("expr exists");
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 220)
                        .expect("valid P code");
                    let diag = Diagnostic::error(code)
                        .message("uninit only valid for let mut")
                        .with_span(expr_node.span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        }

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
                mutable,
                name: name_id,
                ty,
                value,
                atomic,
            },
        ))
    }

    /// Parse the optional `@atomic(Ordering)` binding-position attribute
    /// (paideia-as#1296, v0.21-003b).
    ///
    /// Called immediately after consuming `let` in `parse_let_stmt`, before the
    /// optional `mut` keyword. Returns `Ok(None)` when the current token is not
    /// `@` — the common non-atomic case that must stay hot-path fast.
    ///
    /// **Grammar:** `@ atomic ( <ordering-name> )`
    ///
    /// The ordering identifier must be exactly one of `Relaxed`, `Acquire`,
    /// `Release`, or `SeqCst` — the C11 / Rust `std::sync::atomic::Ordering`
    /// vocabulary. Case-sensitive by design so a reader familiar with either
    /// model can port intent one-to-one.
    ///
    /// **Diagnostics (all P-category, following the `@abi`/`@align` convention).
    /// Numbers slot into the free block above the `@abi` P0285 range:**
    /// - P0286 — the token after `@` is not the identifier `atomic`.
    /// - P0287 — missing `(` after `atomic`.
    /// - P0288 — the ordering argument is not one of the four recognised names.
    /// - P0289 — missing `)` after the ordering identifier.
    fn parse_optional_atomic_prefix(&mut self) -> Result<Option<AtomicOrdering>, ParseError> {
        if !self.at(TokenKind::At) {
            return Ok(None);
        }

        let at_tok = self.expect(TokenKind::At)?;
        let at_span = at_tok.span;

        // Attribute name — must be `atomic`.
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_text = self.source_text_for_span(name_tok.span);
        if name_text != "atomic" {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 286)
                .expect("valid P0286 code");
            let diag = Diagnostic::error(code)
                .message(format!(
                    "unknown binding-position attribute '@{}' on let (only '@atomic(Ordering)' is recognised)",
                    name_text
                ))
                .with_span(name_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Expect `(`.
        if !self.eat(TokenKind::LParen) {
            let span = self.peek().map(|t| t.span).unwrap_or(at_span);
            let code = DiagnosticCode::new(Category::P, Severity::Error, 287)
                .expect("valid P0287 code");
            let diag = Diagnostic::error(code)
                .message("malformed @atomic(Ordering) syntax: expected '(' after 'atomic'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Ordering argument.
        let ord_tok = self.expect(TokenKind::Ident)?;
        let ord_text = self.source_text_for_span(ord_tok.span);
        let ordering = match ord_text {
            "Relaxed" => AtomicOrdering::Relaxed,
            "Acquire" => AtomicOrdering::Acquire,
            "Release" => AtomicOrdering::Release,
            "SeqCst" => AtomicOrdering::SeqCst,
            other => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 288)
                    .expect("valid P0288 code");
                let diag = Diagnostic::error(code)
                    .message(format!(
                        "unknown atomic ordering '{}' (expected one of: Relaxed, Acquire, Release, SeqCst)",
                        other
                    ))
                    .with_span(ord_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Expect `)`.
        if !self.eat(TokenKind::RParen) {
            let span = self.peek().map(|t| t.span).unwrap_or(ord_tok.span);
            let code = DiagnosticCode::new(Category::P, Severity::Error, 289)
                .expect("valid P0289 code");
            let diag = Diagnostic::error(code)
                .message("malformed @atomic(Ordering) syntax: expected ')' after ordering name")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(Some(ordering))
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
    ///
    /// When expression is a control structure (`if`, `while`, `loop`, `for`, `{`),
    /// dispatch to statement-position parsing (BlockKind::Statement), allowing
    /// trailing semicolons to be synthesized as unit literals.
    fn parse_expr_stmt(&mut self) -> Result<NodeId, ParseError> {
        let expr_start = self.peek().map(|tok| tok.span).ok_or(ParseError)?;

        // Check if expression is a statement-position control structure
        let expr = if let Some(tok) = self.peek() {
            match tok.kind {
                TokenKind::KwIf => self.parse_if(BlockKind::Statement)?,
                TokenKind::KwWhile | TokenKind::KwLoop | TokenKind::KwFor => {
                    self.parse_loop_form()?
                }
                TokenKind::LBrace => self.parse_block_kind(BlockKind::Statement)?,
                _ => self.parse_expr()?,
            }
        } else {
            self.parse_expr()?
        };

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

    /// Parse an instruction statement: `Mnemonic Operand ("," Operand)*`
    ///
    /// Assumes the current token is an Ident (the mnemonic). Consumes the
    /// mnemonic, interns it via `arena.intern_mnemonic()`, then parses
    /// comma-separated operands.
    ///
    /// After each comma, a valid operand MUST follow. Memory-operand re-sync
    /// requires that after consuming a comma, if no valid operand can be parsed,
    /// we must check that a statement separator or block-end follows (phase-6 m2-003).
    ///
    /// No trailing semicolon is required (the action block's `}` terminates).
    fn parse_instruction_stmt(&mut self) -> Result<NodeId, ParseError> {
        let mnem_tok = self.expect(TokenKind::Ident)?;
        let mnem_span = mnem_tok.span;

        // Extract mnemonic text from source before borrowing arena mutably.
        let mnem_text = {
            let source = self.source();
            source[mnem_span.byte_start() as usize
                ..(mnem_span.byte_start() as usize + mnem_span.byte_len() as usize)]
                .to_string()
        };

        // Now intern the mnemonic
        let mnemonic = self.arena_mut().intern_mnemonic(&mnem_text);

        let mut operands = Vec::new();

        // Parse operands: comma-separated list
        loop {
            // Check if we're at a boundary where operands should end
            if self.at(TokenKind::Semicolon) || self.at(TokenKind::RBrace) {
                break;
            }
            // paideia-as#1320: `ident:` immediately following the mnemonic (or a
            // preceding operand) is a label declaration, not an operand. Without this
            // check, a bare zero-operand instruction like `ret` directly followed by
            // a label (no `;` separator) would have the label's identifier consumed
            // as a stray operand, leaving the `:` to cascade into a P0100 error on
            // the next statement. Segment-override colon syntax (`fs:`/`gs:`) only
            // ever appears inside `[...]` memory refs, so `Ident Colon` in bare
            // operand position is unambiguously a label start, never a valid operand.
            if self.at(TokenKind::Ident) && self.peek_at(1).map(|t| t.kind) == Some(TokenKind::Colon)
            {
                break;
            }
            if self.peek().is_none() {
                break;
            }

            let operand = self.parse_operand()?;
            operands.push(operand);

            // Check for comma (more operands) or end of statement
            if !self.eat(TokenKind::Comma) {
                break;
            }

            // Phase-6 m2-003: After consuming a comma, we must have another operand.
            // Check if we're at a boundary (semicolon, RBrace, or EOF).
            // If so, this is a trailing comma error (P0102).
            if self.at(TokenKind::Semicolon) || self.at(TokenKind::RBrace) || self.peek().is_none()
            {
                let next_span = self.peek().map(|t| t.span).unwrap_or(mnem_span);
                let code =
                    DiagnosticCode::new(Category::P, Severity::Error, 102).expect("valid P code");
                let diag = Diagnostic::error(code)
                    .message("expected operand after comma in instruction")
                    .with_span(next_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        }

        // Consume optional trailing semicolon
        self.eat(TokenKind::Semicolon);

        let end_span = self.peek().map(|tok| tok.span).unwrap_or(mnem_span);
        let stmt_span = Span::new(
            mnem_span.file(),
            mnem_span.byte_start(),
            end_span.byte_start() - mnem_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_stmt(
            NodeKind::StmtInstruction,
            stmt_span,
            StmtData::Instruction { mnemonic, operands },
        ))
    }

    /// Parse a single operand: Register | MemoryRef | ImmediateExpr.
    ///
    /// **Algorithm:**
    /// 1. If current token is `LBracket`: parse memory reference `[addr_expr]`.
    /// 2. If current token is `Ident`: parse as register operand.
    /// 3. Otherwise: parse as immediate (any expression).
    fn parse_operand(&mut self) -> Result<NodeId, ParseError> {
        if self.at(TokenKind::LBracket) {
            self.parse_memref()
        } else if self.at(TokenKind::Ident) {
            // Register operand
            let reg_tok = self.expect(TokenKind::Ident)?;
            let reg_id = self.arena_mut().alloc(NodeKind::Ident, reg_tok.span);
            let span = reg_tok.span;
            Ok(self.arena_mut().alloc_expr(
                NodeKind::OperandRegister,
                span,
                paideia_as_ast::ExprData::OperandRegister { reg: reg_id },
            ))
        } else {
            // Immediate operand: any expression
            let expr = self.parse_expr()?;
            let span = self
                .arena()
                .get(expr)
                .map(|nd| nd.span)
                .unwrap_or_else(|| self.peek().map(|tok| tok.span).unwrap_or(self.file_span()));
            Ok(self.arena_mut().alloc_expr(
                NodeKind::OperandImmediate,
                span,
                paideia_as_ast::ExprData::OperandImmediate { expr },
            ))
        }
    }

    /// Helper: get a dummy span for the current file (used in error cases).
    fn file_span(&self) -> Span {
        use paideia_as_diagnostics::Span;
        Span::new(self.file(), 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, span(byte_start, byte_len))
    }

    #[test]
    fn let_typed_stmt() {
        // `let x : u64 = 1 ;`
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::Ident, 4, 1), // "x"
            tok(TokenKind::Colon, 6, 1),
            tok(TokenKind::Ident, 8, 3), // "u64"
            tok(TokenKind::Assign, 12, 1),
            tok(TokenKind::IntLit, 14, 1), // "1"
            tok(TokenKind::Semicolon, 16, 1),
        ];
        let source = "let x : u64 = 1 ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtLet node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtLet);
    }

    #[test]
    fn return_stmt_with_expr() {
        // `return x ;`
        let toks = vec![
            tok(TokenKind::KwReturn, 0, 6),
            tok(TokenKind::Ident, 7, 1), // "x"
            tok(TokenKind::Semicolon, 9, 1),
        ];
        let source = "return x ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtReturn node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtReturn);
    }

    #[test]
    fn return_stmt_no_expr() {
        // `return ;`
        let toks = vec![
            tok(TokenKind::KwReturn, 0, 6),
            tok(TokenKind::Semicolon, 7, 1),
        ];
        let source = "return ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtReturn node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtReturn);
    }

    #[test]
    fn expr_stmt() {
        // `foo();`
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "foo"
            tok(TokenKind::LParen, 3, 1),
            tok(TokenKind::RParen, 4, 1),
            tok(TokenKind::Semicolon, 5, 1),
        ];
        let source = "foo();";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtExpr node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtExpr);
    }

    #[test]
    fn instruction_stmt_with_register_operands() {
        // `mov rax, rbx`
        // Within action context, this should parse as an instruction statement.
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "mov"
            tok(TokenKind::Ident, 4, 3), // "rax"
            tok(TokenKind::Comma, 7, 1),
            tok(TokenKind::Ident, 9, 3), // "rbx"
        ];
        let source = "mov rax, rbx";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true); // in_action_context = true
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtInstruction node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtInstruction);
    }

    #[test]
    fn instruction_stmt_with_memref_operand() {
        // `mov rax, [rbp - 8]`
        // Within action context, this should parse as an instruction statement.
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "mov"
            tok(TokenKind::Ident, 4, 3), // "rax"
            tok(TokenKind::Comma, 7, 1),
            tok(TokenKind::LBracket, 9, 1),
            tok(TokenKind::Ident, 10, 3), // "rbp"
            tok(TokenKind::Minus, 14, 1),
            tok(TokenKind::IntLit, 16, 1), // "8"
            tok(TokenKind::RBracket, 17, 1),
        ];
        let source = "mov rax, [rbp - 8]";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true); // in_action_context = true
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtInstruction node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtInstruction);
    }

    #[test]
    fn instruction_stmt_with_immediate_operand() {
        // `add rax, 42`
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "add"
            tok(TokenKind::Ident, 4, 3), // "rax"
            tok(TokenKind::Comma, 7, 1),
            tok(TokenKind::IntLit, 9, 2), // "42"
        ];
        let source = "add rax, 42";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true); // in_action_context = true
        assert!(result.is_ok());
        let stmt_id = result.unwrap();

        // Verify it's a StmtInstruction node
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtInstruction);
    }

    /// Phase 6 m2-003: Test 1 — Single-line mov + lea with semicolon.
    /// `mov rax, rdi; lea rax, [rdi + 1]; ret` should parse cleanly.
    #[test]
    fn instruction_resync_mov_lea_semicolon() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "mov"
            tok(TokenKind::Ident, 4, 3), // "rax"
            tok(TokenKind::Comma, 7, 1),
            tok(TokenKind::Ident, 9, 3), // "rdi"
            tok(TokenKind::Semicolon, 12, 1),
            tok(TokenKind::Ident, 14, 3), // "lea"
            tok(TokenKind::Ident, 18, 3), // "rax"
            tok(TokenKind::Comma, 21, 1),
            tok(TokenKind::LBracket, 23, 1),
            tok(TokenKind::Ident, 24, 3), // "rdi"
            tok(TokenKind::Plus, 28, 1),
            tok(TokenKind::IntLit, 30, 1), // "1"
            tok(TokenKind::RBracket, 31, 1),
            tok(TokenKind::Semicolon, 32, 1),
            tok(TokenKind::Ident, 34, 3), // "ret"
        ];
        let source = "mov rax, rdi; lea rax, [rdi + 1]; ret";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true); // First instruction: mov rax, rdi
        assert!(result.is_ok(), "mov rax, rdi should parse");

        let result = p.parse_stmt(true); // Second instruction: lea rax, [rdi + 1]
        assert!(result.is_ok(), "lea rax, [rdi + 1] should parse");

        let result = p.parse_stmt(true); // Third instruction: ret
        assert!(result.is_ok(), "ret should parse");
    }

    /// Phase 6 m2-003: Test 2 — add + sub with semicolon.
    /// `add rax, 42; sub rbx, 1; ret` should parse cleanly.
    #[test]
    fn instruction_resync_add_sub_semicolon() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "add"
            tok(TokenKind::Ident, 4, 3), // "rax"
            tok(TokenKind::Comma, 7, 1),
            tok(TokenKind::IntLit, 9, 2), // "42"
            tok(TokenKind::Semicolon, 11, 1),
            tok(TokenKind::Ident, 13, 3), // "sub"
            tok(TokenKind::Ident, 17, 3), // "rbx"
            tok(TokenKind::Comma, 20, 1),
            tok(TokenKind::IntLit, 22, 1), // "1"
            tok(TokenKind::Semicolon, 23, 1),
            tok(TokenKind::Ident, 25, 3), // "ret"
        ];
        let source = "add rax, 42; sub rbx, 1; ret";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "add rax, 42 should parse");

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "sub rbx, 1 should parse");

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "ret should parse");
    }

    /// Phase 6 m2-003: Test 3 — xor + mov with memory reference.
    /// `xor rcx, rcx; mov rax, [rbp - 8]; ret` should parse cleanly.
    #[test]
    fn instruction_resync_xor_mov_memref() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 3), // "xor"
            tok(TokenKind::Ident, 4, 3), // "rcx"
            tok(TokenKind::Comma, 7, 1),
            tok(TokenKind::Ident, 9, 3), // "rcx"
            tok(TokenKind::Semicolon, 12, 1),
            tok(TokenKind::Ident, 14, 3), // "mov"
            tok(TokenKind::Ident, 18, 3), // "rax"
            tok(TokenKind::Comma, 21, 1),
            tok(TokenKind::LBracket, 23, 1),
            tok(TokenKind::Ident, 24, 3), // "rbp"
            tok(TokenKind::Minus, 28, 1),
            tok(TokenKind::IntLit, 30, 1), // "8"
            tok(TokenKind::RBracket, 31, 1),
            tok(TokenKind::Semicolon, 32, 1),
            tok(TokenKind::Ident, 34, 3), // "ret"
        ];
        let source = "xor rcx, rcx; mov rax, [rbp - 8]; ret";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "xor rcx, rcx should parse");

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "mov rax, [rbp - 8] should parse");

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "ret should parse");
    }

    /// Phase 6 m2-003: Test 4 — imul + lea with immediate and complex memref.
    /// `imul rax, rdi, 2; lea rsi, [rax + rcx]; ret` should parse cleanly.
    #[test]
    fn instruction_resync_imul_lea_immediate() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 4), // "imul"
            tok(TokenKind::Ident, 5, 3), // "rax"
            tok(TokenKind::Comma, 8, 1),
            tok(TokenKind::Ident, 10, 3), // "rdi"
            tok(TokenKind::Comma, 13, 1),
            tok(TokenKind::IntLit, 15, 1), // "2"
            tok(TokenKind::Semicolon, 16, 1),
            tok(TokenKind::Ident, 18, 3), // "lea"
            tok(TokenKind::Ident, 22, 3), // "rsi"
            tok(TokenKind::Comma, 25, 1),
            tok(TokenKind::LBracket, 27, 1),
            tok(TokenKind::Ident, 28, 3), // "rax"
            tok(TokenKind::Plus, 32, 1),
            tok(TokenKind::Ident, 34, 3), // "rcx"
            tok(TokenKind::RBracket, 37, 1),
            tok(TokenKind::Semicolon, 38, 1),
            tok(TokenKind::Ident, 40, 3), // "ret"
        ];
        let source = "imul rax, rdi, 2; lea rsi, [rax + rcx]; ret";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "imul rax, rdi, 2 should parse");

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "lea rsi, [rax + rcx] should parse");

        let result = p.parse_stmt(true);
        assert!(result.is_ok(), "ret should parse");
    }

    /// Phase 6 m5-001: Test 1 — `let mut x : u64 = 0 ;` parses with mutable flag.
    #[test]
    fn let_mut_typed_stmt() {
        // `let mut x : u64 = 0 ;`
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::KwMut, 4, 3), // "mut"
            tok(TokenKind::Ident, 8, 1), // "x"
            tok(TokenKind::Colon, 10, 1),
            tok(TokenKind::Ident, 12, 3), // "u64"
            tok(TokenKind::Assign, 16, 1),
            tok(TokenKind::IntLit, 18, 1), // "0"
            tok(TokenKind::Semicolon, 20, 1),
        ];
        let source = "let mut x : u64 = 0 ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        assert!(result.is_ok(), "let mut x : u64 = 0 should parse");
        let stmt_id = result.unwrap();

        // Verify it's a StmtLet node with mutable=true
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtLet);
        if let Some(StmtData::Let { mutable, .. }) = arena.stmt_data(stmt_id) {
            assert!(*mutable, "let mut should have mutable=true");
        } else {
            panic!("expected Let statement");
        }
    }

    /// Phase 6 m5-001: Test 2 — `let mut arr : [u64; 512] = uninit ;` parses.
    #[test]
    fn let_mut_with_uninit() {
        // `let mut arr : [u64; 512] = uninit ;`
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::KwMut, 4, 3), // "mut"
            tok(TokenKind::Ident, 8, 3), // "arr"
            tok(TokenKind::Colon, 12, 1),
            tok(TokenKind::LBracket, 14, 1),
            tok(TokenKind::Ident, 15, 3), // "u64"
            tok(TokenKind::Semicolon, 18, 1),
            tok(TokenKind::IntLit, 20, 3), // "512"
            tok(TokenKind::RBracket, 23, 1),
            tok(TokenKind::Assign, 25, 1),
            tok(TokenKind::Ident, 27, 6), // "uninit"
            tok(TokenKind::Semicolon, 33, 1),
        ];
        let source = "let mut arr : [u64; 512] = uninit ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        assert!(
            result.is_ok(),
            "let mut arr : [u64; 512] = uninit should parse"
        );
        let stmt_id = result.unwrap();

        // Verify it's a StmtLet node with mutable=true
        let node = arena.get(stmt_id).unwrap();
        assert_eq!(node.kind, NodeKind::StmtLet);
    }

    /// Phase 6 m5-001: Test 3 — `let x : u64 = uninit ;` rejects with P0220.
    #[test]
    fn let_immutable_uninit_rejected() {
        // `let x : u64 = uninit ;` should error with P0220
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::Ident, 4, 1), // "x"
            tok(TokenKind::Colon, 6, 1),
            tok(TokenKind::Ident, 8, 3), // "u64"
            tok(TokenKind::Assign, 12, 1),
            tok(TokenKind::Ident, 14, 6), // "uninit"
            tok(TokenKind::Semicolon, 20, 1),
        ];
        let source = "let x : u64 = uninit ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        // Should fail because uninit is only valid for let mut
        assert!(result.is_err(), "let x : u64 = uninit should fail");
        // Verify P0220 was emitted
        let diags = sink.diagnostics();
        assert!(
            !diags.is_empty(),
            "expected diagnostic for uninit in immutable let"
        );
        assert_eq!(diags[0].code().number(), 220, "expected P0220 error code");
    }

    // ---- paideia-as#1296 (v0.21-003b) ----
    // `@atomic(Ordering)` binding-position attribute on statement-let.
    //
    // Grammar under test:  `let @atomic(<Ord>) [mut] <name> : <T> = <expr> ;`
    // The `@atomic(...)` prefix sits between `let` and the optional `mut`.
    // Four accepted orderings — Relaxed, Acquire, Release, SeqCst — map
    // directly onto C11 / Rust `std::sync::atomic::Ordering`.

    /// Common driver: build the token stream `let @atomic(<Ord>) mut a : u64 = 0 ;`
    /// for a given ordering token, run the statement parser, and return the
    /// parsed `StmtData::Let`'s `atomic` field.
    fn parse_let_with_atomic_prefix(
        ord_ident: &str,
    ) -> (Vec<paideia_as_diagnostics::Diagnostic>, Option<AtomicOrdering>) {
        // Byte layout (matches the source below):
        //   0:  let         (len 3)
        //   4:  @           (len 1)
        //   5:  atomic      (len 6)
        //   11: (           (len 1)
        //   12: <Ord>       (len ord_ident.len())
        //   ..
        let base = 12u32;
        let ord_len = ord_ident.len() as u32;
        let after_ord = base + ord_len;      // position of `)`
        let mut_pos  = after_ord + 2;        // skip `) `
        let mut_len  = 3u32;                 // `mut`
        let name_pos = mut_pos + mut_len + 1;
        let colon_pos = name_pos + 2;
        let ty_pos    = colon_pos + 2;
        let assign_pos = ty_pos + 4;
        let lit_pos    = assign_pos + 2;
        let semi_pos   = lit_pos + 2;

        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::At, 4, 1),
            tok(TokenKind::Ident, 5, 6),           // "atomic"
            tok(TokenKind::LParen, 11, 1),
            tok(TokenKind::Ident, base, ord_len),  // ordering ident
            tok(TokenKind::RParen, after_ord, 1),
            tok(TokenKind::KwMut, mut_pos, mut_len),
            tok(TokenKind::Ident, name_pos, 1),    // "a"
            tok(TokenKind::Colon, colon_pos, 1),
            tok(TokenKind::Ident, ty_pos, 3),      // "u64"
            tok(TokenKind::Assign, assign_pos, 1),
            tok(TokenKind::IntLit, lit_pos, 1),    // "0"
            tok(TokenKind::Semicolon, semi_pos, 1),
        ];
        let source = format!("let @atomic({}) mut a : u64 = 0 ;", ord_ident);
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            &source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_stmt(false);
        let diags = sink.diagnostics().to_vec();
        let atomic = result.ok().and_then(|stmt_id| match arena.stmt_data(stmt_id) {
            Some(paideia_as_ast::StmtData::Let { atomic, .. }) => *atomic,
            _ => None,
        });
        (diags, atomic)
    }

    #[test]
    fn let_atomic_relaxed_parses() {
        let (diags, atomic) = parse_let_with_atomic_prefix("Relaxed");
        assert!(diags.is_empty(), "expected no diagnostics: {:?}", diags);
        assert_eq!(atomic, Some(AtomicOrdering::Relaxed));
    }

    #[test]
    fn let_atomic_acquire_parses() {
        let (diags, atomic) = parse_let_with_atomic_prefix("Acquire");
        assert!(diags.is_empty(), "expected no diagnostics: {:?}", diags);
        assert_eq!(atomic, Some(AtomicOrdering::Acquire));
    }

    #[test]
    fn let_atomic_release_parses() {
        let (diags, atomic) = parse_let_with_atomic_prefix("Release");
        assert!(diags.is_empty(), "expected no diagnostics: {:?}", diags);
        assert_eq!(atomic, Some(AtomicOrdering::Release));
    }

    #[test]
    fn let_atomic_seqcst_parses() {
        let (diags, atomic) = parse_let_with_atomic_prefix("SeqCst");
        assert!(diags.is_empty(), "expected no diagnostics: {:?}", diags);
        assert_eq!(atomic, Some(AtomicOrdering::SeqCst));
    }

    /// Non-atomic `let` bindings keep `atomic == None` — the hot-path default
    /// must not regress and the pretty-printer / IR-propagation code must be
    /// able to distinguish "no `@atomic(...)` seen" from "attribute present".
    #[test]
    fn let_without_atomic_prefix_is_none() {
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::Ident, 4, 1), // "x"
            tok(TokenKind::Colon, 6, 1),
            tok(TokenKind::Ident, 8, 3), // "u64"
            tok(TokenKind::Assign, 12, 1),
            tok(TokenKind::IntLit, 14, 1), // "1"
            tok(TokenKind::Semicolon, 16, 1),
        ];
        let source = "let x : u64 = 1 ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );
        let stmt_id = p.parse_stmt(false).expect("let parses");
        match arena.stmt_data(stmt_id) {
            Some(paideia_as_ast::StmtData::Let { atomic, .. }) => {
                assert!(atomic.is_none(), "non-atomic let must not carry an ordering");
            }
            other => panic!("expected StmtData::Let, got {:?}", other),
        }
    }

    /// Unknown ordering name emits P0288 and refuses the parse — the
    /// grammar only recognises Relaxed / Acquire / Release / SeqCst.
    #[test]
    fn let_atomic_unknown_ordering_emits_p0288() {
        // `let @atomic(Total) mut a : u64 = 0 ;` — "Total" is not a valid ordering.
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::At, 4, 1),
            tok(TokenKind::Ident, 5, 6),  // "atomic"
            tok(TokenKind::LParen, 11, 1),
            tok(TokenKind::Ident, 12, 5), // "Total"
            tok(TokenKind::RParen, 17, 1),
            tok(TokenKind::KwMut, 19, 3),
            tok(TokenKind::Ident, 23, 1),
            tok(TokenKind::Colon, 25, 1),
            tok(TokenKind::Ident, 27, 3),
            tok(TokenKind::Assign, 31, 1),
            tok(TokenKind::IntLit, 33, 1),
            tok(TokenKind::Semicolon, 35, 1),
        ];
        let source = "let @atomic(Total) mut a : u64 = 0 ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );
        let result = p.parse_stmt(false);
        assert!(result.is_err(), "unknown ordering must refuse the parse");
        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 288),
            "expected P0288 emitted, got {:?}",
            diags
        );
    }

    /// Missing `(` after `atomic` emits P0287.
    #[test]
    fn let_atomic_missing_open_paren_emits_p0287() {
        // `let @atomic Release mut a : u64 = 0 ;` — no `(`.
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::At, 4, 1),
            tok(TokenKind::Ident, 5, 6),   // "atomic"
            tok(TokenKind::Ident, 12, 7),  // "Release" — appears where `(` should
            tok(TokenKind::KwMut, 20, 3),
            tok(TokenKind::Ident, 24, 1),
            tok(TokenKind::Colon, 26, 1),
            tok(TokenKind::Ident, 28, 3),
            tok(TokenKind::Assign, 32, 1),
            tok(TokenKind::IntLit, 34, 1),
            tok(TokenKind::Semicolon, 36, 1),
        ];
        let source = "let @atomic Release mut a : u64 = 0 ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );
        let result = p.parse_stmt(false);
        assert!(result.is_err(), "missing '(' must refuse the parse");
        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 287),
            "expected P0287 emitted, got {:?}",
            diags
        );
    }

    /// A non-`atomic` attribute after `@` on a let statement is refused with
    /// P0286 — only `@atomic(Ordering)` is recognised in binding-prefix position.
    #[test]
    fn let_unknown_prefix_attribute_emits_p0286() {
        // `let @foo(Release) mut a : u64 = 0 ;`
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::At, 4, 1),
            tok(TokenKind::Ident, 5, 3),   // "foo"
            tok(TokenKind::LParen, 8, 1),
            tok(TokenKind::Ident, 9, 7),   // "Release"
            tok(TokenKind::RParen, 16, 1),
            tok(TokenKind::KwMut, 18, 3),
            tok(TokenKind::Ident, 22, 1),
            tok(TokenKind::Colon, 24, 1),
            tok(TokenKind::Ident, 26, 3),
            tok(TokenKind::Assign, 30, 1),
            tok(TokenKind::IntLit, 32, 1),
            tok(TokenKind::Semicolon, 34, 1),
        ];
        let source = "let @foo(Release) mut a : u64 = 0 ;";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );
        let result = p.parse_stmt(false);
        assert!(result.is_err(), "unknown prefix attribute must refuse the parse");
        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == 286),
            "expected P0286 emitted, got {:?}",
            diags
        );
    }
}
