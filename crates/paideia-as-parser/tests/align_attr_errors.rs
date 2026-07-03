//! PA10-006y (issue #878): `@align(N)` per-symbol alignment directive.
//!
//! Diagnostic coverage for malformed or invalid `@align(N)` suffixes on
//! `let` declarations:
//! - P0250: unknown symbol attribute (only `@align` is supported)
//! - P0251: malformed `@align(N)` syntax (missing `(` or `)`)
//! - P0252: `@align` value is not a power of two, or is zero

use paideia_as_diagnostics::{DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

/// Parse `source` and return the resulting arena, parse result, and diagnostics.
fn parse_and_check(
    source: &str,
) -> (
    paideia_as_ast::AstArena,
    Result<paideia_as_ast::NodeId, paideia_as_parser::ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = paideia_as_ast::AstArena::new();
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

/// Assert that at least one diagnostic with the given `P<number>` code is present.
fn assert_has_p_code(diags: &[paideia_as_diagnostics::Diagnostic], number: u16) {
    let matches: Vec<_> = diags
        .iter()
        .filter(|d| d.code().category().letter() == 'P' && d.code().number() == number)
        .collect();
    assert!(
        !matches.is_empty(),
        "expected at least one P{:04} diagnostic, got: {:?}",
        number,
        diags
            .iter()
            .map(|d| format!("{}{:04}", d.code().category().letter(), d.code().number()))
            .collect::<Vec<_>>()
    );
    for d in &matches {
        assert_eq!(
            d.code().severity(),
            Severity::Error,
            "P{:04} diagnostic should be an error",
            number
        );
    }
}

#[test]
fn align_attr_unknown_name_emits_p0250() {
    // `@bad_attr(8)` is not a recognized symbol attribute.
    let source = "let mut buf : [u64; 4] = uninit @bad_attr(8)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 250);
}

#[test]
fn align_attr_missing_parens_emits_p0251() {
    // `@align 4096` is missing the opening `(`.
    let source = "let mut buf : [u64; 4] = uninit @align 4096";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 251);
}

#[test]
fn align_attr_non_power_of_two_emits_p0252() {
    // 7 is not a power of two.
    let source = "let mut buf : [u64; 4] = uninit @align(7)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 252);
}

#[test]
fn align_attr_zero_emits_p0252() {
    // 0 is not a valid alignment.
    let source = "let mut buf : [u64; 4] = uninit @align(0)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 252);
}
