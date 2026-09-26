//! Tests for associated-type projection extraction in generic bounds
//! (paideia-as#1496, PAS-DEBT-B2-002).
//!
//! Pre-fix, `parse_generic_params` accepted `T: Iterator<Item = u64>` at
//! lex level but pushed a synthetic marker Ident into the flat `bounds:
//! Vec<NodeId>` and depth-skipped the projection type body — both the
//! projection name and the projection RHS were dropped from the AST.
//!
//! Post-fix, `bounds` is `Vec<TraitBound>` and every bound-position
//! `<Name = Type, ...>` list is parsed as pairs of `(assoc_name_ident, ty)`
//! stored on the bound. Regular type arguments interleaved in the same
//! list are parsed for well-formedness and dropped (elaborator concern).
//!
//! Validation of the projection name against the trait's associated-type
//! set is a separate elaborator concern (needs trait-registry access) —
//! this test file pins parser-side extraction only.
//!
//! Fixtures use a space between the two closing angle brackets
//! (`> >` rather than `>>`) because the lexer collapses `>>` into a
//! single `Shr` token and no `Gt`/`Gt` splitter is wired in the parser
//! today — a wider concern outside this ticket. The projections
//! themselves parse identically.
//!
//! Fixtures:
//! - `struct S<T: Iterator<Item = u64> > {}`              — one projection.
//! - `struct S<T: Iterator<Item = u64, Yield = u32> > {}` — two projections.
//! - `struct S<T: Iterator> {}`                           — zero projections (regression).

use paideia_as_ast::{AstArena, GenericParam, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Diagnostic, DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

fn parse_source(
    source: &str,
) -> (
    AstArena,
    Result<NodeId, paideia_as_parser::ParseError>,
    Vec<Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
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

fn errors_of(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect()
}

/// Walk the arena for the first `Struct` item.
fn first_struct(arena: &AstArena) -> Option<(NodeId, &ItemData)> {
    for raw in 1u32..u32::MAX {
        let id = NodeId::new(raw)?;
        let node = arena.get(id)?;
        if node.kind == NodeKind::Struct {
            let data = arena.item_data(id)?;
            return Some((id, data));
        }
    }
    None
}

/// `T: Iterator<Item = u64>` — parses with one projection on the bound.
#[test]
fn single_projection_on_bound_is_extracted() {
    let (arena, res, diags) = parse_source("struct S<T: Iterator<Item = u64> > {}");
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item");
    match data {
        ItemData::Struct { generic_params, .. } => {
            assert_eq!(generic_params.len(), 1, "one type parameter");
            match &generic_params[0] {
                GenericParam::Type { bounds, .. } => {
                    assert_eq!(bounds.len(), 1, "one trait bound (Iterator)");
                    assert_eq!(
                        bounds[0].projections.len(),
                        1,
                        "one projection (Item = u64)"
                    );
                    let (name_id, ty_id) = bounds[0].projections[0];
                    assert_eq!(
                        arena.get(name_id).unwrap().kind,
                        NodeKind::Ident,
                        "projection name is an Ident"
                    );
                    // ty is a TypeName node for `u64`.
                    assert_eq!(
                        arena.get(ty_id).unwrap().kind,
                        NodeKind::TypeName,
                        "projection type is a TypeName"
                    );
                }
                other => panic!("expected GenericParam::Type, got {:?}", other),
            }
        }
        _ => panic!("expected Struct"),
    }
}

/// `T: Iterator<Item = u64, Yield = u32>` — two projections on one bound.
#[test]
fn two_projections_on_bound_are_extracted() {
    let src = "struct S<T: Iterator<Item = u64, Yield = u32> > {}";
    let (arena, res, diags) = parse_source(src);
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item");
    match data {
        ItemData::Struct { generic_params, .. } => {
            assert_eq!(generic_params.len(), 1);
            match &generic_params[0] {
                GenericParam::Type { bounds, .. } => {
                    assert_eq!(bounds.len(), 1, "still one trait bound");
                    assert_eq!(
                        bounds[0].projections.len(),
                        2,
                        "two projections (Item, Yield)"
                    );
                    for (name_id, ty_id) in &bounds[0].projections {
                        assert_eq!(arena.get(*name_id).unwrap().kind, NodeKind::Ident);
                        assert_eq!(arena.get(*ty_id).unwrap().kind, NodeKind::TypeName);
                    }
                }
                other => panic!("expected GenericParam::Type, got {:?}", other),
            }
        }
        _ => panic!("expected Struct"),
    }
}

/// `T: Iterator` — regression: no projections, bounds still populated.
#[test]
fn bare_bound_has_zero_projections() {
    let (arena, res, diags) = parse_source("struct S<T: Iterator> {}");
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item");
    match data {
        ItemData::Struct { generic_params, .. } => {
            assert_eq!(generic_params.len(), 1);
            match &generic_params[0] {
                GenericParam::Type { bounds, .. } => {
                    assert_eq!(bounds.len(), 1);
                    assert!(
                        bounds[0].projections.is_empty(),
                        "bare `Iterator` has no projections"
                    );
                }
                other => panic!("expected GenericParam::Type, got {:?}", other),
            }
        }
        _ => panic!("expected Struct"),
    }
}

/// Defensive: pretty-printed dump surfaces bound + projections so a silent
/// arm miss on `format_generic_param` shows up (Wave-3/4 lesson).
#[test]
fn projection_appears_in_pretty_dump() {
    let (arena, res, diags) = parse_source("struct S<T: Iterator<Item = u64> > {}");
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    let (id, _data) = first_struct(&arena).expect("Struct item");
    let printed = paideia_as_ast::pretty::print_item(&arena, id);
    // format_generic_param emits `<name-id>: <path-id><<proj-name>=<proj-ty>>`
    // (NodeId displays as `n<N>`). `=n` is unique to the projection formatter
    // — nothing else in the item dump produces that shape, so it pins the
    // projection extraction path against a silent regression.
    assert!(
        printed.contains("=n"),
        "expected `Name=Ty` projection formatting: {}",
        printed
    );
}
