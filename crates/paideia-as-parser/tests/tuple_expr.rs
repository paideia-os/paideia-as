//! Tests for tuple-expression parsing (paideia-as#1500, PAS-DEBT-B2-007).
//!
//! Prior to this issue, `(a, b, c)` was walked into a discarded
//! `let _elements: Vec<NodeId>` and the parser returned a bare
//! `NodeKind::Placeholder` — dropping every element on the floor.
//! `parse_primary/collection.rs` now allocates an `ExprTuple` node whose
//! `ExprData::Tuple { elements }` field carries the parsed subtree.
//!
//! Disambiguation this corpus pins down:
//! - `()`         → `ExprLiteral` (unit) — NEVER a zero-element tuple.
//! - `(x)`        → the inner expression, unwrapped — NEVER a 1-tuple.
//! - `(x,)`       → 1-tuple; the trailing comma is what promotes it.
//! - `(x, y)`, `(x, y, z)`, `(x, y, z,)` → N-tuples for N >= 2.
//!
//! Written against the public `Parser::parse_expr` surface so no internal
//! reshuffle of the AST arena breaks them.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, Term, TermHead};
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::Parser;

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
    Option<NodeId>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let root = {
        let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.parse_expr().ok()
    };
    let diags = sink.diagnostics().to_vec();
    (arena, root, diags)
}

/// Fixture 1: `(x, y)` — 2-tuple.
#[test]
fn tuple_two_elements() {
    // ( x , y )
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::Ident, 4, 1),
        tok(TokenKind::RParen, 5, 1),
        tok(TokenKind::Eof, 6, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprTuple);
    match arena.expr_data(root) {
        Some(ExprData::Tuple { elements }) => {
            assert_eq!(elements.len(), 2);
            assert_eq!(arena.get(elements[0]).unwrap().kind, NodeKind::ExprPath);
            assert_eq!(arena.get(elements[1]).unwrap().kind, NodeKind::ExprPath);
        }
        other => panic!("expected ExprData::Tuple, got {:?}", other),
    }
}

/// Fixture 2: `(x, y, z)` — 3-tuple.
#[test]
fn tuple_three_elements() {
    // ( x , y , z )
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::Ident, 4, 1),
        tok(TokenKind::Comma, 5, 1),
        tok(TokenKind::Ident, 7, 1),
        tok(TokenKind::RParen, 8, 1),
        tok(TokenKind::Eof, 9, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprTuple);
    match arena.expr_data(root) {
        Some(ExprData::Tuple { elements }) => {
            assert_eq!(elements.len(), 3);
            for &id in elements {
                assert_eq!(arena.get(id).unwrap().kind, NodeKind::ExprPath);
            }
        }
        other => panic!("expected ExprData::Tuple, got {:?}", other),
    }
}

/// Fixture 3: `(x,)` — 1-tuple (trailing comma promotes to tuple).
#[test]
fn tuple_singleton_with_trailing_comma() {
    // ( x , )
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::RParen, 3, 1),
        tok(TokenKind::Eof, 4, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(
        arena.get(root).unwrap().kind,
        NodeKind::ExprTuple,
        "trailing comma must promote to a tuple, not a grouping"
    );
    match arena.expr_data(root) {
        Some(ExprData::Tuple { elements }) => {
            assert_eq!(elements.len(), 1, "1-tuple");
            assert_eq!(arena.get(elements[0]).unwrap().kind, NodeKind::ExprPath);
        }
        other => panic!("expected ExprData::Tuple, got {:?}", other),
    }
}

/// Fixture 4: `(x)` — parenthesized grouping, NOT a tuple.
///
/// No trailing comma => the parser must return the inner expression
/// unwrapped. The `NodeKind` must NOT be `ExprTuple`.
#[test]
fn paren_grouping_is_not_a_tuple() {
    // ( x )
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),
        tok(TokenKind::RParen, 2, 1),
        tok(TokenKind::Eof, 3, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    let kind = arena.get(root).unwrap().kind;
    assert_ne!(
        kind,
        NodeKind::ExprTuple,
        "`(x)` must NOT be a 1-tuple; it is a grouping"
    );
    assert_eq!(
        kind,
        NodeKind::ExprPath,
        "`(x)` should be the unwrapped inner ident path"
    );
}

/// Fixture 5: `()` — unit literal, NOT a zero-element tuple.
#[test]
fn unit_literal_is_not_a_tuple() {
    // ( )
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::RParen, 1, 1),
        tok(TokenKind::Eof, 2, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    let kind = arena.get(root).unwrap().kind;
    assert_ne!(
        kind,
        NodeKind::ExprTuple,
        "`()` must remain the unit literal; never a 0-tuple"
    );
    assert_eq!(kind, NodeKind::ExprLiteral, "`()` is `ExprLiteral`");
}

/// Fixture 6: `(x, y, z,)` — trailing comma on N-tuple is allowed and
/// does NOT change the element count.
#[test]
fn tuple_trailing_comma_on_n_tuple() {
    // ( x , y , z , )
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::Ident, 4, 1),
        tok(TokenKind::Comma, 5, 1),
        tok(TokenKind::Ident, 7, 1),
        tok(TokenKind::Comma, 8, 1),
        tok(TokenKind::RParen, 9, 1),
        tok(TokenKind::Eof, 10, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprTuple);
    match arena.expr_data(root) {
        Some(ExprData::Tuple { elements }) => {
            assert_eq!(elements.len(), 3, "trailing comma must not add an element");
        }
        other => panic!("expected ExprData::Tuple, got {:?}", other),
    }
}

/// Fixture 7: elements are NOT dropped on the floor — walker sees them.
///
/// Guards against a regression to the pre-B2-007 shape, where a bare
/// `Placeholder` was returned and `Term::children()` reported zero
/// children for what the source clearly wrote as three.
#[test]
fn tuple_elements_reach_reflect_children() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::Ident, 4, 1),
        tok(TokenKind::Comma, 5, 1),
        tok(TokenKind::Ident, 7, 1),
        tok(TokenKind::RParen, 8, 1),
        tok(TokenKind::Eof, 9, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty());
    let root = root_opt.expect("parse should succeed");

    let term = Term::new(&arena, root);
    assert_eq!(term.head(), TermHead::Tuple);
    let children = term.children();
    assert_eq!(
        children.len(),
        3,
        "reflect walker must see all three elements — no silent drop"
    );
}
