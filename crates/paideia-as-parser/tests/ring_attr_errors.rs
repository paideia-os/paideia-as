//! PA14-r14-008 (issue #951): `@ring(slots=N, slot_size=M)` ring buffer directive.
//!
//! Diagnostic coverage for malformed or invalid `@ring(...)` suffixes on
//! `let` declarations:
//! - P0253: malformed `@ring(...)` syntax (missing parameters, bad format)
//! - P0260: `@ring` slots value is not a power of two, or is zero
//! - P0261: `@ring` slot_size is zero or causes overflow

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
fn ring_attr_malformed_missing_slots_emits_p0253() {
    // `@ring(slot_size=32)` is missing the slots parameter.
    let source = "let mut buf : [u64; 4] = uninit @ring(slot_size=32)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 253);
}

#[test]
fn ring_attr_malformed_missing_slot_size_emits_p0253() {
    // `@ring(slots=256)` is missing the slot_size parameter.
    let source = "let mut buf : [u64; 4] = uninit @ring(slots=256)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 253);
}

#[test]
fn ring_attr_non_power_of_two_slots_emits_p0260() {
    // 7 is not a power of two for slots.
    let source = "let mut buf : [u64; 4] = uninit @ring(slots=7, slot_size=32)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 260);
}

#[test]
fn ring_attr_zero_slots_emits_p0260() {
    // 0 is not a valid slots value.
    let source = "let mut buf : [u64; 4] = uninit @ring(slots=0, slot_size=32)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 260);
}

#[test]
fn ring_attr_zero_slot_size_emits_p0261() {
    // 0 is not a valid slot_size value.
    let source = "let mut buf : [u64; 4] = uninit @ring(slots=256, slot_size=0)";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 261);
}
