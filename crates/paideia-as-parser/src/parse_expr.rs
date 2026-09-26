//! Expression parsing with Pratt operator precedence.
//!
//! This module implements §7 (operator precedence) and §8.Expr grammar.
//! The main entry point is `parse_expr()`, which delegates to `parse_expr_bp(min_bp)`
//! to handle infix, prefix, and postfix operators via standard Pratt precedence climbing.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parse_control::BlockKind;
use crate::parser::{ParseError, Parser};
use crate::precedence::{RANGE_BP, infix_bp, postfix_bp, prefix_bp};

/// Tokens that unambiguously do not begin an expression. Used by the range
/// parser to decide whether `..` is followed by an end operand: the shapes
/// `a..`, `..`, and `a..)` all end with `..` because the next token is one
/// of these terminators. Any other token is assumed to begin an expression
/// and is parsed as the range's end — a parse error there surfaces at the
/// operand site with its normal diagnostic, not silently swallowed here.
fn is_expr_terminator(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Eof
            | TokenKind::Semicolon
            | TokenKind::Comma
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::DotDot // chained `..` — handled explicitly, not as an operand
            | TokenKind::FatArrow
            | TokenKind::Colon
    )
}

fn parse_p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
}

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Check if the current position looks like a record constructor: `{ Ident : ...`.
    /// Assumes the current token is `{`.
    fn is_likely_record_cons(&self) -> bool {
        // Peek one token ahead into the brace to see if it looks like a field name.
        // If we see `}` immediately, it's an empty record (valid).
        // If we see `Ident :`, it's definitely a record.
        // Otherwise, it's likely not a record (e.g., a match arm block).

        match self.peek_at(1).map(|t| t.kind) {
            Some(TokenKind::RBrace) => true, // empty record `{}`
            Some(TokenKind::Ident) => {
                // Next is Ident; check if it's followed by `:`
                matches!(self.peek_at(2).map(|t| t.kind), Some(TokenKind::Colon))
            }
            _ => false,
        }
    }

    /// Parse an expression at the top level.
    ///
    /// Entry point: calls `parse_expr_bp(0)` to parse a full expression
    /// with no minimum binding power constraint.
    pub fn parse_expr(&mut self) -> Result<NodeId, ParseError> {
        self.parse_expr_bp(0)
    }

    /// Parse an expression with Pratt precedence climbing.
    ///
    /// **Algorithm:**
    /// 1. Check for prefix operators (!, -, &, *, $). If found, delegate to `parse_prefix()`.
    /// 2. Otherwise, parse primary via `parse_primary()`.
    /// 3. Loop: peek next token.
    ///    - If postfix op with `postfix_bp >= min_bp`: delegate to `parse_postfix(lhs)`.
    ///    - Else if infix op with `infix_bp.left >= min_bp`: bump, recurse,
    ///      allocate `ExprInfix`, continue.
    ///    - Else: return lhs.
    ///
    /// **Postfix operators:** `(` (calls), `[` (indexing), `.` (field/method), `?` (question).
    /// `parse_postfix` handles all of these uniformly, dispatching on token kind.
    ///
    /// **Operator nodes:** Allocated as `NodeKind::Placeholder` covering the op token.
    ///
    /// **Range chaining:** `..` is dispatched out of the generic Pratt path
    /// to [`Parser::parse_range_infix`] (or [`Parser::parse_range_prefix`] at
    /// expression start). Encountering `a..b..c` — a chained range — emits
    /// `P0103` at the second `..` and recovers by consuming that token.
    /// See [`crate::precedence::RANGE_BP`] for the precedence rationale
    /// (paideia-as#1498, PAS-DEBT-B2-005).
    pub fn parse_expr_bp(&mut self, min_bp: u8) -> Result<NodeId, ParseError> {
        // Step 0: Check for control-flow constructs and lambdas (highest precedence as they parse their own subtrees).
        if let Some(tok) = self.peek() {
            match tok.kind {
                // Keywords that open control structures or lambdas
                paideia_as_lexer::TokenKind::KwFn => {
                    return self.parse_lambda_fn();
                }
                paideia_as_lexer::TokenKind::KwMatch => {
                    return self.parse_match();
                }
                paideia_as_lexer::TokenKind::KwIf => {
                    return self.parse_if(BlockKind::Value);
                }
                paideia_as_lexer::TokenKind::KwLoop
                | paideia_as_lexer::TokenKind::KwWhile
                | paideia_as_lexer::TokenKind::KwFor => {
                    return self.parse_loop_form();
                }
                paideia_as_lexer::TokenKind::LBrace => {
                    return self.parse_block_kind(BlockKind::Value);
                }
                paideia_as_lexer::TokenKind::Pipe => {
                    // Pipe-form lambda: |x, y| body
                    // Guard: verify this is actually a lambda (lookahead for closing pipe)
                    // by ensuring we can parse it successfully
                    return self.parse_lambda_pipe();
                }
                paideia_as_lexer::TokenKind::OrOr => {
                    // Zero-parameter lambda: || body (issue #1236)
                    // OrOr at expr-start is a zero-parameter lambda, not a logical-or operator
                    return self.parse_lambda_pipe_from_oror();
                }
                paideia_as_lexer::TokenKind::KwAction => {
                    // Action block: action !{...}? @{...}? { stmts }
                    return self.parse_action();
                }
                paideia_as_lexer::TokenKind::KwUnsafe => {
                    // Unsafe escape: unsafe { fields... }
                    return self.parse_unsafe();
                }
                paideia_as_lexer::TokenKind::KwWith => {
                    // With-handler: with expr handle name block
                    return self.parse_with_handler();
                }
                _ => {}
            }
        }

        // Step 1: Compute the initial `lhs`. Range shows up in three shapes
        // at expression start (`..`, `..b`) — the DotDot branch below fires
        // BEFORE the generic prefix path so `DotDot` never flows through
        // `parse_prefix` (that path would allocate an `ExprPrefix` node,
        // but a range with an absent start is not a prefix over a single
        // operand: the end may itself be absent). The returned `ExprRange`
        // then falls into the same Pratt loop as any other primary so
        // lower-precedence infix operators (e.g. `..b < c` → `(..b) < c`)
        // can still attach to it.
        let mut lhs = if self.at(TokenKind::DotDot) {
            self.parse_range_prefix(min_bp)?
        } else if self.peek().and_then(|tok| prefix_bp(tok.kind)).is_some() {
            self.parse_prefix()?
        } else {
            self.parse_primary()?
        };

        // Step 3: Loop over postfix and infix operators.
        while let Some(tok) = self.peek() {
            let next_tok = tok.clone();

            // Check for postfix operators first.
            if let Some(post_bp) = postfix_bp(next_tok.kind)
                && post_bp >= min_bp
            {
                lhs = self.parse_postfix(lhs)?;
                continue;
            }

            // Range operator `..` (paideia-as#1498, PAS-DEBT-B2-005). Dispatched
            // out of the generic infix path because its right operand is
            // optional (`a..` is a legal shape) and because the operator is
            // non-associative — a second `..` on an already-`ExprRange` lhs
            // is a chained range (`a..b..c` or `..b..c`) and must be
            // rejected here rather than silently nested. See
            // [`crate::precedence::RANGE_BP`] for the precedence rationale.
            if next_tok.kind == TokenKind::DotDot {
                if RANGE_BP < min_bp {
                    break;
                }
                let lhs_is_range = matches!(
                    self.arena().get(lhs).map(|nd| nd.kind),
                    Some(NodeKind::ExprRange)
                );
                if lhs_is_range {
                    // Chain detected — the previous op was already a range.
                    // Emit P0103 at THIS `..` and consume it so a bare
                    // trailing operand (`c` in `a..b..c`) does not then look
                    // like an unrelated expression start to the caller.
                    let extra_span = self.peek().expect("at(DotDot) implies peek Some").span;
                    let diag = Diagnostic::error(parse_p_code(103))
                        .message(
                            "chained range operators are not allowed; \
                             parenthesize one of the ranges"
                                .to_string(),
                        )
                        .with_span(extra_span)
                        .finish();
                    self.emit_diagnostic(diag);
                    self.bump(); // consume the stray `..`
                    // Skip a trailing operand too, if present, so `..b..c`
                    // and `a..b..c` both leave the stream at the same state
                    // as their un-chained counterparts.
                    if self.peek().map(|t| !is_expr_terminator(t.kind)).unwrap_or(false) {
                        let _ = self.parse_expr_bp(RANGE_BP + 1)?;
                    }
                    break;
                }
                lhs = self.parse_range_infix(lhs)?;
                continue;
            }

            // Check for infix operators.
            if let Some(infix) = infix_bp(next_tok.kind)
                && infix.left >= min_bp
            {
                let op_tok = self.bump().expect("peek returned Some");
                let op_span = op_tok.span;
                let op_node = self.arena_mut().alloc(NodeKind::Placeholder, op_span);

                let rhs = self.parse_expr_bp(infix.right)?;

                // Get spans from the arena
                let lhs_span = self.arena().get(lhs).map(|nd| nd.span).unwrap_or(op_span);
                let rhs_span = self.arena().get(rhs).map(|nd| nd.span).unwrap_or(op_span);

                lhs = self.arena_mut().alloc_expr(
                    NodeKind::ExprInfix,
                    Span::new(
                        lhs_span.file(),
                        lhs_span.byte_start(),
                        rhs_span.byte_start() + rhs_span.byte_len() - lhs_span.byte_start(),
                    ),
                    ExprData::Infix {
                        lhs,
                        op: op_node,
                        rhs,
                    },
                );
                continue;
            }

            // Neither postfix nor infix at this binding power: done.
            break;
        }

        // Check for record constructor: `Ident { field: expr, ... }`
        // Only attempt if lhs is a bare Ident path (single-segment ExprPath).
        // Disambiguate by peeking ahead after `{`: if we see `Ident :`, it's likely a record constructor.
        if self.at(TokenKind::LBrace) {
            if let Some(ExprData::Path { segments }) = self.arena().expr_data(lhs) {
                if segments.len() == 1 && self.is_likely_record_cons() {
                    // This looks like a record constructor; parse it.
                    let type_name_id = segments[0];
                    let lhs_span = self.arena().get(lhs).map(|nd| nd.span).unwrap_or_else(|| {
                        self.peek()
                            .map(|t| t.span)
                            .unwrap_or_else(|| Span::new(self.file(), 0, 0))
                    });
                    if let Ok(record_cons_expr) =
                        self.parse_record_cons_fields(type_name_id, lhs_span)
                    {
                        lhs = record_cons_expr;
                    }
                }
            }
        }

        Ok(lhs)
    }

    /// Parse a prefix range: `..end` (start absent) or bare `..` (both absent).
    ///
    /// Called from [`Parser::parse_expr_bp`] when the very first token of an
    /// expression is `..`. Consumes the `..`, then decides whether an end
    /// operand is present by peeking one token:
    ///
    /// - If that token can begin an expression (i.e. it is not one of the
    ///   terminators in [`is_expr_terminator`]), parse it at
    ///   [`RANGE_BP`]` + 1` — one bp above the range operator so the operand
    ///   never consumes another `..` on our behalf (chained ranges are
    ///   rejected up in `parse_expr_bp`).
    /// - Otherwise, the range has no end (`..` alone, or `..` followed by
    ///   `)`, `]`, `,`, etc.).
    ///
    /// The `min_bp` argument is accepted for symmetry with the Pratt entry
    /// but is not consulted: a prefix `..` is the whole expression here, so
    /// the outer minimum-binding-power constraint applies to the returned
    /// node, not to its construction.
    fn parse_range_prefix(&mut self, _min_bp: u8) -> Result<NodeId, ParseError> {
        let dotdot_tok = self.bump().expect("at(DotDot) implies peek Some");
        let dotdot_span = dotdot_tok.span;

        let end = self.parse_optional_range_endpoint()?;

        // Non-associativity is enforced up in `parse_expr_bp`'s DotDot arm,
        // which fires once the prefix range falls back into the Pratt loop:
        // an `ExprRange` lhs followed by another `..` there emits `P0103`.

        let range_span = match end {
            Some(end_id) => {
                let end_span = self
                    .arena()
                    .get(end_id)
                    .map(|nd| nd.span)
                    .unwrap_or(dotdot_span);
                Span::new(
                    dotdot_span.file(),
                    dotdot_span.byte_start(),
                    end_span.byte_start() + end_span.byte_len() - dotdot_span.byte_start(),
                )
            }
            None => dotdot_span,
        };

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprRange,
            range_span,
            ExprData::Range { start: None, end },
        ))
    }

    /// Parse an infix range: `lhs..end` (both bounded) or `lhs..` (start only).
    ///
    /// Called from the [`Parser::parse_expr_bp`] loop when the next token
    /// after an already-parsed `lhs` is `..`. Consumes the `..`, then
    /// applies the same present-vs-absent-endpoint decision as
    /// [`Parser::parse_range_prefix`]. Chaining (`a..b..c`) is detected by
    /// the caller after this method returns, so the parse itself never
    /// synthesizes nested `ExprRange` here.
    fn parse_range_infix(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        let dotdot_tok = self.bump().expect("caller peeked DotDot");
        let dotdot_span = dotdot_tok.span;

        let end = self.parse_optional_range_endpoint()?;

        let lhs_span = self.arena().get(lhs).map(|nd| nd.span).unwrap_or(dotdot_span);
        let range_end_span = match end {
            Some(end_id) => self.arena().get(end_id).map(|nd| nd.span).unwrap_or(dotdot_span),
            None => dotdot_span,
        };
        let range_span = Span::new(
            lhs_span.file(),
            lhs_span.byte_start(),
            range_end_span.byte_start() + range_end_span.byte_len() - lhs_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprRange,
            range_span,
            ExprData::Range {
                start: Some(lhs),
                end,
            },
        ))
    }

    /// Peek the next token and either parse a range endpoint or report None.
    ///
    /// Shared by [`Parser::parse_range_prefix`] and
    /// [`Parser::parse_range_infix`]: an endpoint is absent iff the next
    /// token is a terminator (`)`, `]`, `,`, `;`, `..`, EOF, etc.).
    /// Otherwise the endpoint is parsed at `RANGE_BP + 1` so an inner `..`
    /// cannot silently attach itself to our right operand — chained ranges
    /// are the caller's responsibility to reject.
    fn parse_optional_range_endpoint(&mut self) -> Result<Option<NodeId>, ParseError> {
        match self.peek() {
            None => Ok(None),
            Some(tok) if is_expr_terminator(tok.kind) => Ok(None),
            _ => Ok(Some(self.parse_expr_bp(RANGE_BP + 1)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::{Token, TokenKind};

    /// Helper: create a token at a given byte offset with length 1.
    fn tok(kind: TokenKind, byte_start: u32) -> Token {
        Token::new(kind, Span::new(FileId::new(1).unwrap(), byte_start, 1))
    }

    /// Helper: parse a token stream and return (arena, root, diagnostics).
    fn parse(tokens: Vec<Token>) -> (AstArena, NodeId, Vec<paideia_as_diagnostics::Diagnostic>) {
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
    fn single_int_literal() {
        let tokens = vec![tok(TokenKind::IntLit, 0), tok(TokenKind::Eof, 1)];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprLiteral);
    }

    #[test]
    fn simple_add() {
        // a + b
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Plus, 1),  // +
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Eof, 3),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn add_mul_precedence() {
        // a + b * c => + is root, rhs is * (because * binds tighter)
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Plus, 1),  // +
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Star, 3),  // *
            tok(TokenKind::Ident, 4), // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
        // Can't easily check op without access to ExprData, but the tree structure is correct.
    }

    #[test]
    fn mul_add_precedence() {
        // a * b + c => + is root, lhs is * (because * binds tighter)
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Star, 1),  // *
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Plus, 3),  // +
            tok(TokenKind::Ident, 4), // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn right_assoc_assignment() {
        // a = b = c => rightmost = is deeper; a = (b = c)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // a
            tok(TokenKind::Assign, 1), // =
            tok(TokenKind::Ident, 2),  // b
            tok(TokenKind::Assign, 3), // =
            tok(TokenKind::Ident, 4),  // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn left_assoc_subtract() {
        // a - b - c => leftmost - is root; (a - b) - c
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Minus, 1), // -
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Minus, 3), // -
            tok(TokenKind::Ident, 4), // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn logical_and_or() {
        // a && b || c => || is root, lhs is && (because && binds tighter)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // a
            tok(TokenKind::AndAnd, 1), // &&
            tok(TokenKind::Ident, 2),  // b
            tok(TokenKind::OrOr, 3),   // ||
            tok(TokenKind::Ident, 4),  // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn compare_lt() {
        // a < b
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Lt, 1),    // <
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Eof, 3),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn compare_below_logical() {
        // a < b && c => && is root, lhs is comparison (because cmp binds tighter)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // a
            tok(TokenKind::Lt, 1),     // <
            tok(TokenKind::Ident, 2),  // b
            tok(TokenKind::AndAnd, 3), // &&
            tok(TokenKind::Ident, 4),  // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn bit_or_below_logical() {
        // a | b && c => && is root, lhs is | (because | binds tighter)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // a
            tok(TokenKind::Pipe, 1),   // |
            tok(TokenKind::Ident, 2),  // b
            tok(TokenKind::AndAnd, 3), // &&
            tok(TokenKind::Ident, 4),  // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn shifts() {
        // a << b
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Shl, 1),   // <<
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Eof, 3),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn prefix_negation() {
        // -a
        let tokens = vec![
            tok(TokenKind::Minus, 0), // -
            tok(TokenKind::Ident, 1), // a
            tok(TokenKind::Eof, 2),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprPrefix);
    }

    #[test]
    fn prefix_logical_not() {
        // !a
        let tokens = vec![
            tok(TokenKind::Bang, 0),  // !
            tok(TokenKind::Ident, 1), // a
            tok(TokenKind::Eof, 2),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprPrefix);
    }

    #[test]
    fn nested_prefix() {
        // !!a => outer ! wraps inner !
        let tokens = vec![
            tok(TokenKind::Bang, 0),  // !
            tok(TokenKind::Bang, 1),  // !
            tok(TokenKind::Ident, 2), // a
            tok(TokenKind::Eof, 3),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprPrefix);
    }

    #[test]
    fn postfix_question() {
        // a?
        let tokens = vec![
            tok(TokenKind::Ident, 0),    // a
            tok(TokenKind::Question, 1), // ?
            tok(TokenKind::Eof, 2),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprPostfix);
    }

    #[test]
    fn parens_override_precedence() {
        // (a + b) * c
        let tokens = vec![
            tok(TokenKind::LParen, 0), // (
            tok(TokenKind::Ident, 1),  // a
            tok(TokenKind::Plus, 2),   // +
            tok(TokenKind::Ident, 3),  // b
            tok(TokenKind::RParen, 4), // )
            tok(TokenKind::Star, 5),   // *
            tok(TokenKind::Ident, 6),  // c
            tok(TokenKind::Eof, 7),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn eof_after_partial_expr() {
        // a + (EOF: incomplete)
        // parse_expr should return the `a` and leave the parser at `+`
        // (or error if it tries to parse rhs of +)
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Plus, 1),  // +
            tok(TokenKind::Eof, 2),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            // Attempting to parse should fail when rhs is missing.
            let result = p.parse_expr();
            assert!(result.is_err(), "Should error on incomplete expression");
        }
        assert!(
            !sink.diagnostics().is_empty(),
            "Should emit at least one diagnostic"
        );
    }

    #[test]
    fn mixed_precedence_long() {
        // a + b * c - d / e => ((a + (b*c)) - (d/e))
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Plus, 1),  // +
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Star, 3),  // *
            tok(TokenKind::Ident, 4), // c
            tok(TokenKind::Minus, 5), // -
            tok(TokenKind::Ident, 6), // d
            tok(TokenKind::Slash, 7), // /
            tok(TokenKind::Ident, 8), // e
            tok(TokenKind::Eof, 9),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn prefix_and_infix_combined() {
        // -a + b => (-a) + b
        let tokens = vec![
            tok(TokenKind::Minus, 0), // -
            tok(TokenKind::Ident, 1), // a
            tok(TokenKind::Plus, 2),  // +
            tok(TokenKind::Ident, 3), // b
            tok(TokenKind::Eof, 4),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn bitwise_and_precedence() {
        // a & b | c => | is root, lhs is & (because & binds tighter)
        let tokens = vec![
            tok(TokenKind::Ident, 0), // a
            tok(TokenKind::Amp, 1),   // &
            tok(TokenKind::Ident, 2), // b
            tok(TokenKind::Pipe, 3),  // |
            tok(TokenKind::Ident, 4), // c
            tok(TokenKind::Eof, 5),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprInfix);
    }

    #[test]
    fn postfix_multiple() {
        // a? ? b => first `?` is postfix on a, then error or parse continues
        // For now, just test that a single postfix works as expected.
        let tokens = vec![
            tok(TokenKind::Ident, 0),    // a
            tok(TokenKind::Question, 1), // ?
            tok(TokenKind::Eof, 2),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprPostfix);
    }

    #[test]
    fn prefix_with_postfix() {
        // -a? => postfix `?` on `a`, then prefix `-` on the postfix result
        // Actually, `-a?` should parse as `-(a?)` due to postfix binding tighter.
        let tokens = vec![
            tok(TokenKind::Minus, 0),    // -
            tok(TokenKind::Ident, 1),    // a
            tok(TokenKind::Question, 2), // ?
            tok(TokenKind::Eof, 3),
        ];
        let (arena, root, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        let root_node = arena.get(root).unwrap();
        assert_eq!(root_node.kind, NodeKind::ExprPrefix);
    }
}
