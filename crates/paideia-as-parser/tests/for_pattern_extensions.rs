//! Tests for `for` loop pattern shapes (paideia-as#1538, PAS-DEBT-B2-001).
//!
//! Before this fix, `parse_for_pattern` only accepted a bare `Ident`; any
//! other pattern shape (`(a,b)`, `&p`, `[a,..]`, …) fell through to a
//! silent `ParseError`. The fix routes the `for`-header pattern through
//! the general `Parser::parse_pattern` (promoted `pub` by Wave 7 B2-013).
//!
//! Fixtures use source-based lexing so both grammar and token classification
//! are exercised end-to-end. Covers:
//! - `for x in iter { 1 }`         — Ident regression.
//! - `for _ in iter { 1 }`         — Wildcard (lexed as Ident today, so
//!                                   parses to `PatIdent` — pin the shape).
//! - `for (a, b) in iter { 1 }`    — Tuple pattern.
//! - `for &p in iter { 1 }`        — Reference pattern.
//! - `for [a, b, ..] in iter { 1 }`— Slice pattern with trailing rest.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, PatternData};
use paideia_as_diagnostics::{DiagnosticSink, SourceMap, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

fn parse_expr_source(
    source: &str,
) -> (AstArena, NodeId, Vec<paideia_as_diagnostics::Diagnostic>) {
    let mut source_map = SourceMap::new();
    let file = source_map.add_file(std::path::PathBuf::from("test.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let mut collector = VecSink::new();
    let tokens = lex.collect_tokens(&mut collector);
    for d in collector.into_diagnostics() {
        let _ = sink.emit(d);
    }
    let root = {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        p.parse_expr().expect("parse_expr failed")
    };
    let diags = sink.into_diagnostics();
    (arena, root, diags)
}

/// Extract the pattern NodeId from a parsed `for` expression.
fn for_pattern(arena: &AstArena, root: NodeId) -> NodeId {
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::ExprFor);
    match arena.expr_data(root) {
        Some(ExprData::For { pattern, .. }) => *pattern,
        other => panic!("expected ExprData::For, got {:?}", other),
    }
}

/// `for x in list { 1 }` — Ident regression.
#[test]
fn for_ident_pattern() {
    let (arena, root, diags) = parse_expr_source("for x in list { 1 }");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let pat = for_pattern(&arena, root);
    assert_eq!(arena.get(pat).unwrap().kind, NodeKind::PatIdent);
}

/// `for _ in list { 1 }` — Wildcard. `_` lexes as Ident today, so the AST
/// shape is `PatIdent` (name = `_`); the point is that the header parses at
/// all — under the old Ident-only guard it happened to work by accident, but
/// this pins it against future divergence.
#[test]
fn for_wildcard_pattern() {
    let (arena, root, diags) = parse_expr_source("for _ in list { 1 }");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let pat = for_pattern(&arena, root);
    assert_eq!(arena.get(pat).unwrap().kind, NodeKind::PatIdent);
}

/// `for (a, b) in list { 1 }` — Tuple pattern. Was rejected pre-fix.
#[test]
fn for_tuple_pattern() {
    let (arena, root, diags) = parse_expr_source("for (a, b) in list { 1 }");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let pat = for_pattern(&arena, root);
    assert_eq!(arena.get(pat).unwrap().kind, NodeKind::PatTuple);
    match arena.pattern_data(pat) {
        Some(PatternData::Tuple { elements }) => {
            assert_eq!(elements.len(), 2);
            for e in elements {
                assert_eq!(arena.get(*e).unwrap().kind, NodeKind::PatIdent);
            }
        }
        other => panic!("expected PatternData::Tuple, got {:?}", other),
    }
}

/// `for &p in list { 1 }` — Reference pattern. Was rejected pre-fix.
#[test]
fn for_reference_pattern() {
    let (arena, root, diags) = parse_expr_source("for &p in list { 1 }");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let pat = for_pattern(&arena, root);
    assert_eq!(arena.get(pat).unwrap().kind, NodeKind::PatReference);
    match arena.pattern_data(pat) {
        Some(PatternData::Reference { inner, mutable }) => {
            assert!(!mutable);
            assert_eq!(arena.get(*inner).unwrap().kind, NodeKind::PatIdent);
        }
        other => panic!("expected PatternData::Reference, got {:?}", other),
    }
}

/// `for [a, b, ..] in list { 1 }` — Slice pattern with trailing rest.
/// Was rejected pre-fix.
#[test]
fn for_slice_pattern_with_rest() {
    let (arena, root, diags) = parse_expr_source("for [a, b, ..] in list { 1 }");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let pat = for_pattern(&arena, root);
    assert_eq!(arena.get(pat).unwrap().kind, NodeKind::PatSlice);
    match arena.pattern_data(pat) {
        Some(PatternData::Slice { elements }) => {
            assert_eq!(elements.len(), 3);
            assert_eq!(arena.get(elements[0]).unwrap().kind, NodeKind::PatIdent);
            assert_eq!(arena.get(elements[1]).unwrap().kind, NodeKind::PatIdent);
            assert_eq!(arena.get(elements[2]).unwrap().kind, NodeKind::PatRest);
        }
        other => panic!("expected PatternData::Slice, got {:?}", other),
    }
}
