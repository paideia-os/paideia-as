//! Slice A fixture corpus for PAS-DEBT-B2-010 (#1503): fragment-kind
//! pattern grammar. Six positive fixtures — one per required fragment
//! kind (expr, ident, type, pat, stmt, block) — plus one negative
//! (unknown fragment kind emits P0110). Template substitution
//! (follow-up B2-010b) and repetition + hygiene (B2-010c) are out of
//! scope here.

use paideia_as_ast::{
    AstArena, ItemData, MacroFragmentKind, MacroPatternElem, NodeId, NodeKind,
};
use paideia_as_diagnostics::{Diagnostic, DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::{ParseError, Parser};

fn parse_source_str(
    source: &str,
) -> (AstArena, Result<NodeId, ParseError>, Vec<Diagnostic>) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
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
    let result = {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        p.parse_source_file()
    };
    (arena, result, sink.into_diagnostics())
}

/// Assert the source parses clean and the first macro rule has exactly
/// one Fragment element with the expected kind, sitting on a
/// `NodeKind::MacroPattern` (not a `Placeholder`) pattern node.
fn assert_single_fragment(source: &str, expect_kind: MacroFragmentKind) {
    let (arena, result, diags) = parse_source_str(source);
    let root = result.expect("source should parse");

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");

    let Some(ItemData::Structure { items, .. }) = arena.item_data(root) else {
        panic!("expected Structure root");
    };
    let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) else {
        panic!("expected MacroDecl item");
    };
    assert_eq!(decl.rules.len(), 1, "one rule expected");
    let rule = &decl.rules[0];

    // Slice A: pattern arena node is MacroPattern, not Placeholder.
    let pattern_node = &arena[rule.pattern];
    assert_eq!(
        pattern_node.kind,
        NodeKind::MacroPattern,
        "pattern node should be MacroPattern (not Placeholder)"
    );

    // Exactly one Fragment among the elements, with the expected kind.
    let frags: Vec<_> = rule
        .pattern_elems
        .iter()
        .filter_map(|e| match e {
            MacroPatternElem::Fragment { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(frags, vec![expect_kind], "fragment-kind mismatch");

    // Fragment-only projection stays in sync.
    assert_eq!(rule.fragments.len(), 1);
    assert_eq!(rule.fragments[0].kind, expect_kind);
}

#[test]
fn fragment_kind_expr_parses_as_fragment_elem() {
    assert_single_fragment("macro m($x:expr) => { x }", MacroFragmentKind::Expr);
}

#[test]
fn fragment_kind_ident_parses_as_fragment_elem() {
    assert_single_fragment("macro m($x:ident) => { x }", MacroFragmentKind::Ident);
}

#[test]
fn fragment_kind_type_parses_as_fragment_elem() {
    // `type` is the long form; `ty` is the short form. Both map to Ty.
    assert_single_fragment("macro m($x:type) => { x }", MacroFragmentKind::Ty);
}

#[test]
fn fragment_kind_pat_parses_as_fragment_elem() {
    assert_single_fragment("macro m($x:pat) => { x }", MacroFragmentKind::Pat);
}

#[test]
fn fragment_kind_stmt_parses_as_fragment_elem() {
    assert_single_fragment("macro m($x:stmt) => { x }", MacroFragmentKind::Stmt);
}

#[test]
fn fragment_kind_block_parses_as_fragment_elem() {
    assert_single_fragment("macro m($x:block) => { x }", MacroFragmentKind::Block);
}

#[test]
fn unknown_fragment_kind_emits_p0110_and_collapses_to_literal() {
    let (arena, result, diags) = parse_source_str("macro m($x:wat) => { x }");
    let root = result.expect("parse recovers past P0110");

    // Exactly one P0110.
    let p0110: Vec<_> = diags.iter().filter(|d| d.code().number() == 110).collect();
    assert_eq!(p0110.len(), 1, "should emit exactly one P0110");

    // The unknown fragment collapses to a Literal element, so the
    // structured pattern list is not lossy — it still spans the whole
    // pattern source.
    let Some(ItemData::Structure { items, .. }) = arena.item_data(root) else {
        panic!("expected Structure root");
    };
    let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) else {
        panic!("expected MacroDecl");
    };
    let rule = &decl.rules[0];
    assert!(
        rule.pattern_elems
            .iter()
            .all(|e| matches!(e, MacroPatternElem::Literal { .. })),
        "unknown fragment kind should collapse to Literal, not survive as Fragment"
    );
    assert!(rule.fragments.is_empty(), "no fragments recognised");
}

#[test]
fn interleaved_literal_and_fragment_elements_preserved_in_order() {
    // Two fragments separated by a comma literal — verifies interleaving
    // order is source-order.
    let (arena, result, diags) =
        parse_source_str("macro pair($a:expr, $b:ident) => { a + b }");
    let root = result.expect("should parse");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let Some(ItemData::Structure { items, .. }) = arena.item_data(root) else {
        panic!("expected Structure root");
    };
    let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) else {
        panic!("expected MacroDecl");
    };
    let rule = &decl.rules[0];

    // Expect: [Fragment(expr), Literal(","), Fragment(ident)] (with
    // possible whitespace-only Literal spans in between).
    let kinds: Vec<_> = rule
        .pattern_elems
        .iter()
        .map(|e| match e {
            MacroPatternElem::Fragment { kind, .. } => Some(*kind),
            MacroPatternElem::Literal { .. } => None,
        })
        .collect();
    let fragment_kinds: Vec<_> = kinds.iter().filter_map(|k| *k).collect();
    assert_eq!(
        fragment_kinds,
        vec![MacroFragmentKind::Expr, MacroFragmentKind::Ident],
        "fragment order must match source order"
    );
    // At least one Literal between the two Fragments (the `, ` separator).
    let mut saw_first_frag = false;
    let mut saw_literal_between = false;
    for elem in &rule.pattern_elems {
        match elem {
            MacroPatternElem::Fragment { kind, .. } if *kind == MacroFragmentKind::Expr => {
                saw_first_frag = true;
            }
            MacroPatternElem::Literal { .. } if saw_first_frag => {
                saw_literal_between = true;
            }
            MacroPatternElem::Fragment { kind, .. }
                if *kind == MacroFragmentKind::Ident && saw_first_frag =>
            {
                break;
            }
            _ => {}
        }
    }
    assert!(
        saw_literal_between,
        "expected a Literal separator between the two fragments"
    );
}
