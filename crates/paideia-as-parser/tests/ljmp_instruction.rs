//! Parser integration tests for ljmp (far jump) instruction with selector and offset operands.
//! Issue #896 (Phase 6 m6-001b): Parser surface for `ljmp selector : offset` syntax.
//!
//! Tests parsing of the two-operand ljmp instruction within an unsafe block:
//! - `ljmp imm16, symbol` (selector immediate, offset symbol reference)
//! - `ljmp imm16, imm32` (both immediates)

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

/// Helper function to parse source code and return the arena, parse result, and diagnostics.
fn parse_source(
    source: &str,
) -> (
    AstArena,
    Result<paideia_as_ast::NodeId, paideia_as_parser::ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
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

/// PA10-006h Test: Full program with `ljmp 0x08, long_mode_entry` in unsafe block.
/// Tests that ljmp with immediate selector and symbol offset parses correctly.
#[test]
fn ljmp_imm16_symbol_in_unsafe_block_parses() {
    let source = r#"
    module Test = structure {
        let target : () -> () = fn() -> 0;
        let _start : () -> () !{} @{} = fn(_: ()) -> unsafe {
            effects: {},
            capabilities: {},
            justification: "test ljmp with symbol",
            block: {
                ljmp 0x08, target
            }
        }
    }
    "#;

    let (_arena, result, diags) = parse_source(source);

    // Check for parse errors
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "no errors expected for ljmp with symbol: {:?}",
        errors
    );

    assert!(result.is_ok(), "parse should succeed");
}

/// PA10-006h Test: Full program with `ljmp 0x08, 0x100000` (two immediates).
#[test]
fn ljmp_imm16_imm32_in_unsafe_block_parses() {
    let source = r#"
    module Test = structure {
        let _start : () -> () !{} @{} = fn(_: ()) -> unsafe {
            effects: {},
            capabilities: {},
            justification: "test ljmp with immediates",
            block: {
                ljmp 0x08, 0x100000
            }
        }
    }
    "#;

    let (_arena, result, diags) = parse_source(source);

    // Check for parse errors
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "no errors expected for ljmp with two immediates: {:?}",
        errors
    );

    assert!(result.is_ok(), "parse should succeed");
}

/// PA10-006h Test: Multiple ljmp instructions in the same unsafe block.
#[test]
fn ljmp_multiple_instructions_parse() {
    let source = r#"
    module Test = structure {
        let entry : () -> () = fn() -> 0;
        let next : () -> () = fn() -> 0;
        let _start : () -> () !{} @{} = fn(_: ()) -> unsafe {
            effects: {},
            capabilities: {},
            justification: "test multiple ljmp",
            block: {
                ljmp 0x08, entry;
                ljmp 0x10, next
            }
        }
    }
    "#;

    let (_arena, result, diags) = parse_source(source);

    // Check for parse errors
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "no errors expected for multiple ljmp: {:?}",
        errors
    );

    assert!(result.is_ok(), "parse should succeed");
}

/// PA10-006h Test: ljmp with different selector values.
#[test]
fn ljmp_various_selectors_parse() {
    let source = r#"
    module Test = structure {
        let target : () -> () = fn() -> 0;
        let _start : () -> () !{} @{} = fn(_: ()) -> unsafe {
            effects: {},
            capabilities: {},
            justification: "test ljmp selectors",
            block: {
                ljmp 0x00, target;
                ljmp 0x08, target;
                ljmp 0x10, target;
                ljmp 0x18, target
            }
        }
    }
    "#;

    let (_arena, result, diags) = parse_source(source);

    // Check for parse errors
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "no errors expected for ljmp with various selectors: {:?}",
        errors
    );

    assert!(result.is_ok(), "parse should succeed");
}
