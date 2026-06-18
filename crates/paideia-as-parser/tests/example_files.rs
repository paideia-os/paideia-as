//! Integration tests for the item parser with real source code examples.
//!
//! Tests the `parse_source_file` function with various item declaration combinations.

use paideia_as_ast::{AstArena, NodeId};
use paideia_as_diagnostics::{DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

/// Helper function to parse source code and return the arena, parse result, and diagnostics.
fn parse_source(
    source: &str,
) -> (
    AstArena,
    Result<NodeId, paideia_as_parser::ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(std::path::PathBuf::from("test.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let mut collector = VecSink::new();
    let tokens = lex.collect_tokens(&mut collector);
    // Forward lexer diagnostics into the main sink.
    for d in collector.into_diagnostics() {
        let _ = sink.emit(d);
    }
    let result = {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        p.parse_source_file()
    };
    (arena, result, sink.into_diagnostics())
}

#[test]
fn test_simple_let_decl() {
    let (_arena, _err, diags) = parse_source("let x : u64 = 1");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse simple let declaration without errors"
    );
}

#[test]
fn test_single_module() {
    let (_arena, _err, diags) = parse_source("module M = structure { let x = 1 }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse single module without errors"
    );
}

#[test]
fn test_effect_with_one_op() {
    let (_arena, _err, diags) = parse_source("effect Io { op read : u8 }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse effect declaration without errors"
    );
}

#[test]
fn test_signature_decl() {
    let (_arena, _err, diags) = parse_source("signature S = structure { let t = T }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse signature declaration without errors"
    );
}

#[test]
fn test_two_modules_emits_m0306() {
    let (_arena, _err, diags) =
        parse_source("module A = structure { let x = 1 } module B = structure { let y = 2 }");
    // Check for M0306 diagnostic
    let m0306_diags: Vec<_> = diags.iter().filter(|d| d.code().number() == 306).collect();
    assert_eq!(
        m0306_diags.len(),
        1,
        "should emit exactly one M0306 diagnostic for two modules"
    );
}

#[test]
fn test_enum_decl() {
    let (_arena, _err, diags) = parse_source("enum Color { Red, Green, Blue }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse enum declaration without errors"
    );
}

#[test]
fn test_multiple_items_mixed() {
    let source = r#"
let global_x : u64 = 42
effect File { op read : u8 }
signature S = structure { let t = T }
enum Status { Ok, Error }
"#;
    let (_arena, _err, diags) = parse_source(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse multiple mixed items without errors"
    );
}

#[test]
fn test_struct_decl() {
    let (_arena, _err, diags) = parse_source("struct Point { x: i32, y: i32 }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse struct declaration without errors"
    );
}

#[test]
fn test_capability_decl() {
    let (_arena, _err, diags) = parse_source("capability Console { print: (s: string) -> unit }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse capability declaration without errors"
    );
}

#[test]
fn test_functor_module() {
    let (_arena, _err, diags) =
        parse_source("module M = functor (S: Sig) -> structure { let x = 1 }");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should parse functor module without errors"
    );
}
