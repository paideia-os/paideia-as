//! PAS-DEBT-B2-004 — parser support for top-level `struct` type-definition
//! syntax needed by `tests/build-emit/cap_set_rights.pdx`.
//!
//! The parser already carries `parse_struct_decl` (see
//! `parse_item/struct_enum.rs`) and dispatches to it from `parse_item`
//! at both file scope and inside `module M = structure { ... }`. This
//! file pins that surface end-to-end: three shapes B1-001 will lean on
//! (empty body, monomorphic with a scalar field, generic with a type
//! parameter) plus a mini-parse of the updated cap_set_rights fixture.
//!
//! Any regression here becomes a real diagnostic instead of B1-001
//! silently going red on parse.

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind};
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

/// Walk the arena for the first `Struct` item. Iterating NodeIds by
/// raw index mirrors `packed_struct_snapshots::field_ids_of_first_struct`.
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

#[test]
fn struct_monomorphic_single_field_parses() {
    let (arena, res, diags) = parse_source("struct Foo { x: u64 }");
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item");
    match data {
        ItemData::Struct {
            generic_params,
            fields,
            ..
        } => {
            assert!(generic_params.is_empty(), "no generics expected");
            assert_eq!(fields.len(), 1, "one field");
        }
        _ => panic!("expected Struct"),
    }
}

#[test]
fn struct_generic_parses() {
    let (arena, res, diags) = parse_source("struct Bar<T> { y: T }");
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item");
    match data {
        ItemData::Struct {
            generic_params,
            fields,
            ..
        } => {
            assert_eq!(generic_params.len(), 1, "one generic param");
            assert_eq!(fields.len(), 1, "one field");
        }
        _ => panic!("expected Struct"),
    }
}

#[test]
fn struct_empty_body_parses() {
    let (arena, res, diags) = parse_source("struct Zero {}");
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item");
    match data {
        ItemData::Struct {
            generic_params,
            fields,
            ..
        } => {
            assert!(generic_params.is_empty());
            assert!(fields.is_empty(), "empty body has zero fields");
        }
        _ => panic!("expected Struct"),
    }
}

#[test]
fn struct_inside_module_structure_parses() {
    // Shape matches cap_set_rights.pdx after the B2-004 fixture update.
    let src = "module M = structure { struct Capability { kind: u64, target: u64, rights: u64, generation: u64 } }";
    let (arena, res, diags) = parse_source(src);
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(errors_of(&diags).is_empty());

    let (_id, data) = first_struct(&arena).expect("Struct item nested in structure");
    match data {
        ItemData::Struct { fields, .. } => {
            assert_eq!(fields.len(), 4, "four fields");
        }
        _ => panic!("expected Struct"),
    }
}

#[test]
fn cap_set_rights_fixture_parses() {
    // Pull the fixture verbatim so a regression on either the parser or
    // the fixture itself trips this crate before the workspace build
    // ever compiles B1-001. Elaboration + emit of the field-write are
    // B1-001's territory; parse must be green today.
    let src = include_str!("../../../tests/build-emit/cap_set_rights.pdx");
    let (_arena, res, diags) = parse_source(src);
    assert!(res.is_ok(), "parse failed: {:?}", errors_of(&diags));
    assert!(
        errors_of(&diags).is_empty(),
        "unexpected parse errors: {:?}",
        errors_of(&diags)
    );
}
