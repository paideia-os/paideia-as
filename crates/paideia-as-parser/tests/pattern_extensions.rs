//! Tests for pattern-extension parsing (paideia-as#1506, PAS-DEBT-B2-013).
//!
//! Covers the four pattern shapes previously rejected with `P0100`:
//!
//! - `0..10`             — Range pattern (exclusive).
//! - `a | b`             — Or-pattern (already worked; pinned here so a
//!                         reshuffle doesn't regress it).
//! - `a | b | c`         — Or-pattern with 3 alternatives.
//! - `&x`                — Reference pattern.
//! - `[a, b, c]`         — Slice pattern without rest.
//! - `[a, .., last]`     — Slice pattern with rest sub-pattern.
//! - `..end`             — Prefix-range pattern (open start).
//! - `[..name]`          — Slice with named rest binder.
//!
//! Written against `Parser::parse_pattern`, which PAS-DEBT-B2-013 promoted
//! from `pub(crate)` to `pub` for direct integration testing.

use paideia_as_ast::{AstArena, NodeId, NodeKind, PatternData};
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::Parser;

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

fn parse(tokens: Vec<Token>) -> (AstArena, Option<NodeId>, Vec<paideia_as_diagnostics::Diagnostic>) {
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let root = {
        let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.parse_pattern().ok()
    };
    let diags = sink.diagnostics().to_vec();
    (arena, root, diags)
}

/// Fixture 1: `0..10` — range pattern, both endpoints present.
#[test]
fn pattern_range_both_bounded() {
    let tokens = vec![
        tok(TokenKind::IntLit, 0, 1),  // 0
        tok(TokenKind::DotDot, 1, 2),  // ..
        tok(TokenKind::IntLit, 3, 2),  // 10
        tok(TokenKind::Eof, 5, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatRange);
    match arena.pattern_data(root) {
        Some(PatternData::Range { start, end }) => {
            let s = start.expect("start present");
            let e = end.expect("end present");
            assert_eq!(arena.get(s).unwrap().kind, NodeKind::PatLiteral);
            assert_eq!(arena.get(e).unwrap().kind, NodeKind::PatLiteral);
        }
        other => panic!("expected PatternData::Range, got {:?}", other),
    }
}

/// Fixture 2: `..end` — open-start range pattern.
#[test]
fn pattern_range_open_start() {
    let tokens = vec![
        tok(TokenKind::DotDot, 0, 2),  // ..
        tok(TokenKind::IntLit, 2, 2),  // 10
        tok(TokenKind::Eof, 4, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatRange);
    match arena.pattern_data(root) {
        Some(PatternData::Range { start, end }) => {
            assert!(start.is_none(), "start must be None for `..end`");
            assert!(end.is_some(), "end present");
        }
        other => panic!("expected PatternData::Range, got {:?}", other),
    }
}

/// Fixture 3: `a | b` — or-pattern with 2 alternatives (pinning).
#[test]
fn pattern_or_two_alts() {
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),  // a
        tok(TokenKind::Pipe, 2, 1),   // |
        tok(TokenKind::Ident, 4, 1),  // b
        tok(TokenKind::Eof, 5, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatOr);
    match arena.pattern_data(root) {
        Some(PatternData::Or { alternatives }) => {
            assert_eq!(alternatives.len(), 2);
        }
        other => panic!("expected PatternData::Or, got {:?}", other),
    }
}

/// Fixture 4: `a | b | c` — or-pattern with 3 alternatives.
#[test]
fn pattern_or_three_alts() {
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),  // a
        tok(TokenKind::Pipe, 2, 1),   // |
        tok(TokenKind::Ident, 4, 1),  // b
        tok(TokenKind::Pipe, 6, 1),   // |
        tok(TokenKind::Ident, 8, 1),  // c
        tok(TokenKind::Eof, 9, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatOr);
    match arena.pattern_data(root) {
        Some(PatternData::Or { alternatives }) => {
            assert_eq!(alternatives.len(), 3);
        }
        other => panic!("expected PatternData::Or, got {:?}", other),
    }
}

/// Fixture 5: `&x` — reference pattern (immutable).
#[test]
fn pattern_reference_immut() {
    let tokens = vec![
        tok(TokenKind::Amp, 0, 1),    // &
        tok(TokenKind::Ident, 1, 1),  // x
        tok(TokenKind::Eof, 2, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatReference);
    match arena.pattern_data(root) {
        Some(PatternData::Reference { inner, mutable }) => {
            assert!(!mutable, "&x is immutable reference");
            assert_eq!(arena.get(*inner).unwrap().kind, NodeKind::PatIdent);
        }
        other => panic!("expected PatternData::Reference, got {:?}", other),
    }
}

/// Fixture 6: `&mut x` — mutable reference pattern.
#[test]
fn pattern_reference_mut() {
    let tokens = vec![
        tok(TokenKind::Amp, 0, 1),    // &
        tok(TokenKind::KwMut, 1, 3),  // mut
        tok(TokenKind::Ident, 5, 1),  // x
        tok(TokenKind::Eof, 6, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatReference);
    match arena.pattern_data(root) {
        Some(PatternData::Reference { mutable, .. }) => assert!(mutable),
        other => panic!("expected PatternData::Reference, got {:?}", other),
    }
}

/// Fixture 7: `[a, b, c]` — slice pattern, no rest.
#[test]
fn pattern_slice_no_rest() {
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1), // [
        tok(TokenKind::Ident, 1, 1),    // a
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::Ident, 4, 1),    // b
        tok(TokenKind::Comma, 5, 1),
        tok(TokenKind::Ident, 7, 1),    // c
        tok(TokenKind::RBracket, 8, 1), // ]
        tok(TokenKind::Eof, 9, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatSlice);
    match arena.pattern_data(root) {
        Some(PatternData::Slice { elements }) => {
            assert_eq!(elements.len(), 3);
            for e in elements {
                assert_eq!(arena.get(*e).unwrap().kind, NodeKind::PatIdent);
            }
        }
        other => panic!("expected PatternData::Slice, got {:?}", other),
    }
}

/// Fixture 8: `[a, .., last]` — slice pattern with rest sub-pattern in the
/// middle.
#[test]
fn pattern_slice_with_rest() {
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),  // [
        tok(TokenKind::Ident, 1, 1),     // a
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::DotDot, 4, 2),    // ..
        tok(TokenKind::Comma, 6, 1),
        tok(TokenKind::Ident, 8, 4),     // last
        tok(TokenKind::RBracket, 12, 1), // ]
        tok(TokenKind::Eof, 13, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatSlice);
    match arena.pattern_data(root) {
        Some(PatternData::Slice { elements }) => {
            assert_eq!(elements.len(), 3);
            assert_eq!(arena.get(elements[0]).unwrap().kind, NodeKind::PatIdent);
            assert_eq!(arena.get(elements[1]).unwrap().kind, NodeKind::PatRest);
            assert_eq!(arena.get(elements[2]).unwrap().kind, NodeKind::PatIdent);
            // Rest has no binder in `[a, .., last]`.
            match arena.pattern_data(elements[1]) {
                Some(PatternData::Rest { binder }) => assert!(binder.is_none()),
                other => panic!("expected PatternData::Rest, got {:?}", other),
            }
        }
        other => panic!("expected PatternData::Slice, got {:?}", other),
    }
}

/// Fixture 9: `[..tail]` — slice pattern with named rest binder.
#[test]
fn pattern_slice_named_rest() {
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1), // [
        tok(TokenKind::DotDot, 1, 2),   // ..
        tok(TokenKind::Ident, 3, 4),    // tail
        tok(TokenKind::RBracket, 7, 1), // ]
        tok(TokenKind::Eof, 8, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::PatSlice);
    match arena.pattern_data(root) {
        Some(PatternData::Slice { elements }) => {
            assert_eq!(elements.len(), 1);
            match arena.pattern_data(elements[0]) {
                Some(PatternData::Rest { binder }) => {
                    let b = binder.expect("named rest binder");
                    assert_eq!(arena.get(b).unwrap().kind, NodeKind::Ident);
                }
                other => panic!("expected PatternData::Rest, got {:?}", other),
            }
        }
        other => panic!("expected PatternData::Slice, got {:?}", other),
    }
}
