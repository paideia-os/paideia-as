//! Tests for range-expression parsing (paideia-as#1498, PAS-DEBT-B2-005).
//!
//! The lexer already emits `TokenKind::DotDot` for `..` (v0.36.42,
//! PAS-DEBT-B2-021 / #1540). These tests exercise the parser production:
//!
//! - `1..2`  → `Range { start: Some(1), end: Some(2) }`
//! - `..2`   → `Range { start: None,    end: Some(2) }`
//! - `1..`   → `Range { start: Some(1), end: None    }`
//! - `..`    → `Range { start: None,    end: None    }`
//! - `arr[i..j]` — range parses inside an index expression
//! - `1 + 2 .. 3 + 4` — arithmetic binds tighter, so this groups as
//!   `(1+2)..(3+4)` (see [`crate::precedence::RANGE_BP`] for rationale)
//! - `a < b .. c` — comparison binds looser, so this groups as
//!   `a < (b..c)`
//! - `a..b..c` — chained ranges are rejected with `P0103`
//!
//! Written against the public `Parser::parse_expr` surface so no internal
//! reshuffling of the AST arena breaks them.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, FileId, Severity, Span, VecSink};
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

/// Fixture 1: `1..2` — both endpoints bounded.
#[test]
fn range_both_bounded() {
    let tokens = vec![
        tok(TokenKind::IntLit, 0, 1),
        tok(TokenKind::DotDot, 1, 2),
        tok(TokenKind::IntLit, 3, 1),
        tok(TokenKind::Eof, 4, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprRange);
    match arena.expr_data(root) {
        Some(ExprData::Range { start, end }) => {
            let s = start.expect("start present");
            let e = end.expect("end present");
            assert_eq!(arena.get(s).unwrap().kind, NodeKind::ExprLiteral);
            assert_eq!(arena.get(e).unwrap().kind, NodeKind::ExprLiteral);
        }
        other => panic!("expected ExprData::Range, got {:?}", other),
    }
}

/// Fixture 2: `..2` — start absent (prefix form).
#[test]
fn range_open_start() {
    let tokens = vec![
        tok(TokenKind::DotDot, 0, 2),
        tok(TokenKind::IntLit, 2, 1),
        tok(TokenKind::Eof, 3, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprRange);
    match arena.expr_data(root) {
        Some(ExprData::Range { start, end }) => {
            assert!(start.is_none(), "start must be None for `..2`");
            let e = end.expect("end present");
            assert_eq!(arena.get(e).unwrap().kind, NodeKind::ExprLiteral);
        }
        other => panic!("expected ExprData::Range, got {:?}", other),
    }
}

/// Fixture 3: `1..` — end absent (infix form followed by EOF/terminator).
#[test]
fn range_open_end() {
    let tokens = vec![
        tok(TokenKind::IntLit, 0, 1),
        tok(TokenKind::DotDot, 1, 2),
        tok(TokenKind::Eof, 3, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprRange);
    match arena.expr_data(root) {
        Some(ExprData::Range { start, end }) => {
            let s = start.expect("start present");
            assert_eq!(arena.get(s).unwrap().kind, NodeKind::ExprLiteral);
            assert!(end.is_none(), "end must be None for `1..`");
        }
        other => panic!("expected ExprData::Range, got {:?}", other),
    }
}

/// Fixture 4: `..` — both endpoints absent (fully-open range).
#[test]
fn range_fully_open() {
    let tokens = vec![
        tok(TokenKind::DotDot, 0, 2),
        tok(TokenKind::Eof, 2, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprRange);
    match arena.expr_data(root) {
        Some(ExprData::Range { start, end }) => {
            assert!(start.is_none(), "start must be None for `..`");
            assert!(end.is_none(), "end must be None for `..`");
        }
        other => panic!("expected ExprData::Range, got {:?}", other),
    }
}

/// Fixture 5: `arr[i..j]` — range parses inside an index expression.
///
/// The current parser lowers `x[e]` to an `ExprCall` whose `callee` is `x`
/// and whose single arg is `e` (see `parse_postfix::parse_index`). So the
/// tree is `Call { callee: arr, args: [Range { start: i, end: j }] }`.
#[test]
fn range_inside_index() {
    let tokens = vec![
        tok(TokenKind::Ident, 0, 3),    // arr
        tok(TokenKind::LBracket, 3, 1), // [
        tok(TokenKind::Ident, 4, 1),    // i
        tok(TokenKind::DotDot, 5, 2),   // ..
        tok(TokenKind::Ident, 7, 1),    // j
        tok(TokenKind::RBracket, 8, 1), // ]
        tok(TokenKind::Eof, 9, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprCall);
    match arena.expr_data(root) {
        Some(ExprData::Call { args, .. }) => {
            assert_eq!(args.len(), 1, "index has exactly one arg");
            let inner = args[0];
            assert_eq!(
                arena.get(inner).unwrap().kind,
                NodeKind::ExprRange,
                "inner arg should be a range"
            );
            match arena.expr_data(inner) {
                Some(ExprData::Range { start, end }) => {
                    assert!(start.is_some());
                    assert!(end.is_some());
                }
                other => panic!("expected ExprData::Range inside index, got {:?}", other),
            }
        }
        other => panic!("expected ExprData::Call (index shape), got {:?}", other),
    }
}

/// Fixture 6: precedence — `1 + 2 .. 3 + 4` groups as `(1+2)..(3+4)`.
///
/// `..` binds looser than `+`/`-` (see [`crate::precedence::RANGE_BP`]),
/// matching Rust. So both operands of the range are `ExprInfix` nodes.
#[test]
fn range_precedence_below_arithmetic() {
    let tokens = vec![
        tok(TokenKind::IntLit, 0, 1),  // 1
        tok(TokenKind::Plus, 2, 1),    // +
        tok(TokenKind::IntLit, 4, 1),  // 2
        tok(TokenKind::DotDot, 6, 2),  // ..
        tok(TokenKind::IntLit, 9, 1),  // 3
        tok(TokenKind::Plus, 11, 1),   // +
        tok(TokenKind::IntLit, 13, 1), // 4
        tok(TokenKind::Eof, 14, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(
        arena.get(root).unwrap().kind,
        NodeKind::ExprRange,
        "root must be the range — `+` binds tighter"
    );
    match arena.expr_data(root) {
        Some(ExprData::Range { start, end }) => {
            let s = start.expect("start present");
            let e = end.expect("end present");
            assert_eq!(
                arena.get(s).unwrap().kind,
                NodeKind::ExprInfix,
                "start `(1+2)` must be an infix"
            );
            assert_eq!(
                arena.get(e).unwrap().kind,
                NodeKind::ExprInfix,
                "end `(3+4)` must be an infix"
            );
        }
        other => panic!("expected ExprData::Range, got {:?}", other),
    }
}

/// Fixture 7: precedence — `a < b .. c` groups as `a < (b..c)`.
///
/// Comparison binds LOOSER than `..`, so range's right subtree becomes
/// the `..` expression and the root is `<`.
#[test]
fn range_precedence_above_comparison() {
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),  // a
        tok(TokenKind::Lt, 2, 1),     // <
        tok(TokenKind::Ident, 4, 1),  // b
        tok(TokenKind::DotDot, 5, 2), // ..
        tok(TokenKind::Ident, 7, 1),  // c
        tok(TokenKind::Eof, 8, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(
        arena.get(root).unwrap().kind,
        NodeKind::ExprInfix,
        "root must be the `<` — `..` binds tighter"
    );
    match arena.expr_data(root) {
        Some(ExprData::Infix { rhs, .. }) => {
            assert_eq!(
                arena.get(*rhs).unwrap().kind,
                NodeKind::ExprRange,
                "rhs of `<` must be the range"
            );
        }
        other => panic!("expected ExprData::Infix, got {:?}", other),
    }
}

/// Fixture 8: chained range `a..b..c` — the second `..` is rejected with
/// `P0103`. The parser recovers by consuming the stray `..` so the caller
/// does not see it as a downstream syntax error.
#[test]
fn range_chaining_rejected() {
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),  // a
        tok(TokenKind::DotDot, 1, 2), // ..
        tok(TokenKind::Ident, 3, 1),  // b
        tok(TokenKind::DotDot, 4, 2), // ..
        tok(TokenKind::Ident, 6, 1),  // c
        tok(TokenKind::Eof, 7, 0),
    ];
    let (_arena, _root_opt, diags) = parse(tokens);

    assert!(
        diags.iter().any(|d| {
            let code = d.code();
            code.category() == Category::P
                && code.severity() == Severity::Error
                && code.number() == 103
        }),
        "expected P0103 for chained range, got: {:?}",
        diags
    );
}
