//! Unit tests for let-declaration parsing and trailing symbol attributes.
//!
//! Extracted verbatim from `let_item.rs` when it was split into a directory
//! (paideia-as#1408, 2026-09-07). No test logic changed — this file exists
//! purely to keep each source file within the 300-600 LOC comfort band.

use super::*;
use paideia_as_diagnostics::{DiagnosticSink, SourceMap, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use std::path::PathBuf;

/// Helper: parse source code and return (arena, parse result, diagnostics)
fn parse_and_check(
    source: &str,
) -> (
    paideia_as_ast::AstArena,
    Result<paideia_as_ast::NodeId, ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut source_map = SourceMap::new();
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

/// Assert that at least one diagnostic with the given P<number> code is present.
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
}

/// Assert that a diagnostic with the given P<number> code contains a substring.
fn assert_p_code_message_contains(
    diags: &[paideia_as_diagnostics::Diagnostic],
    number: u16,
    substring: &str,
) {
    let matches: Vec<_> = diags
        .iter()
        .filter(|d| d.code().category().letter() == 'P' && d.code().number() == number)
        .collect();
    assert!(
        !matches.is_empty(),
        "expected at least one P{:04} diagnostic",
        number
    );
    let found = matches.iter().any(|d| d.message().contains(substring));
    assert!(
        found,
        "P{:04} diagnostic should contain '{}', got messages: {:?}",
        number,
        substring,
        matches.iter().map(|d| d.message()).collect::<Vec<_>>()
    );
}

/// Helper: parse a let declaration wrapped in a minimal module structure and extract the Let node.
/// Takes just the let declaration (e.g., "let payload : [u8; 8] = uninit @link_section(\"name\")")
/// and returns (arena, extracted_let_result, diagnostics).
fn parse_let_in_module(
    source: &str,
) -> (
    paideia_as_ast::AstArena,
    Result<paideia_as_ast::NodeId, ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let wrapped = format!(r#"module Test = structure {{ {} }}"#, source);
    let (arena, result, diags) = parse_and_check(&wrapped);

    let extracted_result = match result {
        Ok(root_id) => {
            // parse_source_file returns an implicit root Structure containing top-level items
            if let Some(root_node) = arena.get(root_id) {
                if let paideia_as_ast::NodeKind::Structure = root_node.kind {
                    // Get items from root structure (should contain the Module)
                    if let Some(paideia_as_ast::ItemData::Structure { items: root_items, .. }) = arena.item_data(root_id) {
                        // First item should be the Module
                        if let Some(&module_id) = root_items.first() {
                            if let Some(module_node) = arena.get(module_id) {
                                if let paideia_as_ast::NodeKind::Module = module_node.kind {
                                    // Navigate Module -> body -> Structure -> items -> Let
                                    if let Some(paideia_as_ast::ItemData::Module { body, .. }) = arena.item_data(module_id) {
                                        let struct_id = *body;
                                        if let Some(struct_node) = arena.get(struct_id) {
                                            if let paideia_as_ast::NodeKind::Structure = struct_node.kind {
                                                if let Some(paideia_as_ast::ItemData::Structure { items: struct_items, .. }) = arena.item_data(struct_id) {
                                                    if let Some(&let_id) = struct_items.first() {
                                                        Ok(let_id)
                                                    } else {
                                                        Err(ParseError)
                                                    }
                                                } else {
                                                    Err(ParseError)
                                                }
                                            } else {
                                                Err(ParseError)
                                            }
                                        } else {
                                            Err(ParseError)
                                        }
                                    } else {
                                        Err(ParseError)
                                    }
                                } else {
                                    Err(ParseError)
                                }
                            } else {
                                Err(ParseError)
                            }
                        } else {
                            Err(ParseError)
                        }
                    } else {
                        Err(ParseError)
                    }
                } else {
                    Err(ParseError)
                }
            } else {
                Err(ParseError)
            }
        }
        Err(e) => Err(e),
    };

    (arena, extracted_result, diags)
}

#[test]
fn link_section_happy_path_dot_uefi_hdr() {
    let source = r#"let payload : [u8; 8] = uninit @link_section(".uefi_hdr")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { link_section, .. }) = arena.item_data(root) {
            assert_eq!(*link_section, Some(".uefi_hdr".to_string()));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

#[test]
fn link_section_no_leading_dot_accepted() {
    let source = r#"let payload : [u8; 8] = uninit @link_section("uefi_hdr")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { link_section, .. }) = arena.item_data(root) {
            assert_eq!(*link_section, Some("uefi_hdr".to_string()));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

#[test]
fn link_section_empty_string_p0282() {
    let source = r#"let payload : [u8; 8] = uninit @link_section("")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 282);
    assert_p_code_message_contains(&diags, 282, "non-empty");
}

#[test]
fn link_section_invalid_char_p0282() {
    let source = r#"let payload : [u8; 8] = uninit @link_section(".foo/bar")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 282);
    assert_p_code_message_contains(&diags, 282, "invalid");
}

#[test]
fn link_section_too_long_p0282() {
    // 33-char name
    let source = r#"let payload : [u8; 8] = uninit @link_section("a_very_long_section_name_is_too_long_here")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 282);
    assert_p_code_message_contains(&diags, 282, "32");
}

#[test]
fn link_section_duplicate_p0283() {
    let source = r#"let payload : [u8; 8] = uninit @link_section(".foo") @link_section(".bar")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 283);
    assert_p_code_message_contains(&diags, 283, "duplicate");
}

#[test]
fn link_section_with_align_both_accepted() {
    let source = r#"let payload : [u8; 8] = uninit @align(64) @link_section(".foo")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { align, link_section, .. }) = arena.item_data(root) {
            assert_eq!(*align, Some(64));
            assert_eq!(*link_section, Some(".foo".to_string()));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

#[test]
fn link_section_before_align_also_accepted() {
    let source = r#"let payload : [u8; 8] = uninit @link_section(".foo") @align(64)"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { align, link_section, .. }) = arena.item_data(root) {
            assert_eq!(*align, Some(64));
            assert_eq!(*link_section, Some(".foo".to_string()));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

#[test]
fn abi_ms_parses() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("ms")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { abi, .. }) = arena.item_data(root) {
            assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Ms));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

#[test]
fn abi_sysv_parses() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("sysv")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { abi, .. }) = arena.item_data(root) {
            assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Sysv));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

#[test]
fn abi_unknown_string_p0285() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("unknown")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 285);
    assert_p_code_message_contains(&diags, 285, "invalid");
}

#[test]
fn abi_uppercase_ms_p0285() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("MS")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 285);
    assert_p_code_message_contains(&diags, 285, "invalid");
}

#[test]
fn abi_empty_string_p0285() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 285);
    assert_p_code_message_contains(&diags, 285, "non-empty");
}

#[test]
fn abi_non_string_arg_p0285() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi(42)"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 285);
}

#[test]
fn abi_missing_parens_p0285() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi"ms""#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 285);
    assert_p_code_message_contains(&diags, 285, "expected '('");
}

#[test]
fn abi_duplicate_p0250() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @abi("ms") @abi("sysv")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "expected parse error");
    assert_has_p_code(&diags, 250);
    assert_p_code_message_contains(&diags, 250, "duplicate");
}

#[test]
fn abi_with_align_and_link_section_all_accepted() {
    let source = r#"let f : (u64) -> u64 = fn(x: u64) -> x @align(64) @link_section(".text_custom") @abi("ms")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok());

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    if let paideia_as_ast::NodeKind::Let = node.kind {
        if let Some(paideia_as_ast::ItemData::Let { align, link_section, abi, .. }) = arena.item_data(root) {
            assert_eq!(*align, Some(64));
            assert_eq!(*link_section, Some(".text_custom".to_string()));
            assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Ms));
        } else {
            panic!("expected ItemData::Let");
        }
    } else {
        panic!("expected NodeKind::Let");
    }
}

/// paideia-as#1276 phase 1: `@no_frame` bare-flag attribute parses and populates
/// `ItemData::Let::no_frame = true`. The attribute is inert this landing — no emit
/// change — but it must round-trip through the parser so paideia-os can annotate
/// hand-crafted trampolines in phase 2 without waiting on the elaborator.
///
/// Lambda body shape mirrors the sibling `abi_*_parses` tests: `fn(params) -> body_expr`
/// (bare-expression body). Only the attribute state is under test here.
#[test]
fn parse_no_frame_attribute() {
    let source = r#"pub let foo : (u64) -> u64 = fn(x: u64) -> x @no_frame"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    assert!(matches!(node.kind, paideia_as_ast::NodeKind::Let), "expected NodeKind::Let");
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { no_frame, public, .. }) => {
            assert!(*no_frame, "@no_frame should set ItemData::Let::no_frame = true");
            assert!(*public, "`pub let` should preserve public=true alongside @no_frame");
        }
        _ => panic!("expected ItemData::Let"),
    }
}

/// paideia-as#1276 phase 1: absence of `@no_frame` leaves the flag at its
/// `false` default — the emit path stays walkable-by-default and existing
/// bindings inherit no unintended opt-out.
#[test]
fn no_frame_absent_by_default() {
    let source = r#"pub let foo : (u64) -> u64 = fn(x: u64) -> x"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    let node = arena.get(root).unwrap();
    assert!(matches!(node.kind, paideia_as_ast::NodeKind::Let), "expected NodeKind::Let");
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { no_frame, .. }) => {
            assert!(
                !*no_frame,
                "no_frame must default to false when the attribute is omitted"
            );
        }
        _ => panic!("expected ItemData::Let"),
    }
}

/// paideia-as#1276 phase 1: `@no_frame` composes cleanly with the other four
/// symbol attributes (`@align`, `@link_section`, `@abi`) in a single trailing
/// attribute run, in any order. Guards against a future refactor that
/// accidentally short-circuits the attribute loop once `no_frame` is seen.
#[test]
fn no_frame_composes_with_other_attributes() {
    let source = r#"pub let foo : (u64) -> u64 = fn(x: u64) -> x @no_frame @abi("sysv")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { no_frame, abi, .. }) => {
            assert!(*no_frame, "@no_frame flag set");
            assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Sysv));
        }
        _ => panic!("expected ItemData::Let"),
    }
}

/// paideia-as#1276 phase 3: `@no_frame` on a non-function binding is a
/// category error — the attribute toggles the SysV frame-pointer
/// prologue/epilogue emitted around function bodies, and there is no
/// prologue/epilogue to suppress for e.g. `pub let CONST : u64 = 42`.
/// The parser rejects such placement with P0250 so the elaborator can
/// trust that `LetInfo::no_frame == true` implies a Lambda RHS.
#[test]
fn no_frame_on_non_fn_binding_errors() {
    // Literal RHS — must be diagnosed.
    let source = r#"pub let CONST : u64 = 42 @no_frame"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(
        result.is_err(),
        "@no_frame on a non-function binding must be a parse error, got Ok"
    );
    assert!(
        !diags.is_empty(),
        "expected at least one diagnostic for @no_frame on non-fn binding, got none"
    );
    let has_p0250 = diags.iter().any(|d| {
        let code = d.code();
        code.category() == paideia_as_diagnostics::Category::P
            && code.number() == 250
    });
    assert!(
        has_p0250,
        "expected P0250 diagnostic for @no_frame on non-fn binding, got: {:?}",
        diags
    );
}

// ---- paideia-as#1278 (v0.21-002): @interrupt / @interrupt_error sugar ----
//
// Phase-1 landing tests the trailing symbol-attribute parse path:
//   pub let f : ... = fn(...) -> body @interrupt("page_fault")
// records InterruptAttr { has_error_code: false, vector: 14, name: "page_fault" }
// on ItemData::Let::interrupt; the @interrupt_error variant flips the flag.

/// Canonical exception name resolves to its SDM vector number and stamps
/// `has_error_code = false` for the plain `@interrupt` form. `page_fault` is
/// vector 14 in the Intel SDM; despite being a real error-code exception,
/// the parser does not cross-check against the `_error` variant in phase-1
/// (kept as a deferred phase-2 validation, per the CANONICAL_VECTORS table).
#[test]
fn interrupt_page_fault_parses_to_vector_14() {
    let source = r#"pub let isr_pf : () -> () = fn() -> 0 @interrupt("page_fault")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { interrupt: Some(attr), .. }) => {
            assert!(!attr.has_error_code, "@interrupt sets has_error_code = false");
            assert_eq!(attr.vector, 14, "page_fault → vector 14");
            assert_eq!(attr.name, "page_fault");
        }
        other => panic!("expected ItemData::Let with interrupt set, got {:?}", other),
    }
}

/// The `@interrupt_error` variant flips `has_error_code` so the phase-2
/// elaborator emits an `add rsp, 8` skip of the CPU-pushed error code
/// before `iretq`.
#[test]
fn interrupt_error_general_protection_parses_to_vector_13() {
    let source = r#"pub let isr_gp : () -> () = fn() -> 0 @interrupt_error("general_protection")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { interrupt: Some(attr), .. }) => {
            assert!(attr.has_error_code, "@interrupt_error sets has_error_code = true");
            assert_eq!(attr.vector, 13, "general_protection → vector 13");
            assert_eq!(attr.name, "general_protection");
        }
        other => panic!("expected ItemData::Let with interrupt set, got {:?}", other),
    }
}

/// Numeric string spelling — the escape hatch for vectors outside the
/// canonical table (e.g. custom LAPIC IPI vectors above 32 that
/// paideia-os may add before the table catches up). Any u8 must accept.
#[test]
fn interrupt_numeric_string_accepts_arbitrary_u8_vector() {
    let source = r#"pub let isr42 : () -> () = fn() -> 0 @interrupt("42")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { interrupt: Some(attr), .. }) => {
            assert!(!attr.has_error_code);
            assert_eq!(attr.vector, 42);
            assert_eq!(attr.name, "42");
        }
        other => panic!("expected ItemData::Let with interrupt set, got {:?}", other),
    }
}

/// An unknown vector name is rejected with P0291.
#[test]
fn interrupt_unknown_vector_emits_p0291() {
    let source = r#"pub let isr_junk : () -> () = fn() -> 0 @interrupt("blorp")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "unknown vector name must fail parse");
    let has_p0291 = diags.iter().any(|d| {
        let code = d.code();
        code.category() == paideia_as_diagnostics::Category::P && code.number() == 291
    });
    assert!(has_p0291, "expected P0291 diagnostic, got: {:?}", diags);
}

/// Out-of-range numeric (256+) is rejected — u8 boundary is the ceiling.
#[test]
fn interrupt_out_of_range_number_emits_p0291() {
    let source = r#"pub let isr_hi : () -> () = fn() -> 0 @interrupt("256")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "vector > 255 must fail parse");
    let has_p0291 = diags.iter().any(|d| {
        let code = d.code();
        code.category() == paideia_as_diagnostics::Category::P && code.number() == 291
    });
    assert!(has_p0291, "expected P0291 diagnostic, got: {:?}", diags);
}

/// Applying `@interrupt(...)` to a non-lambda binding is a category error —
/// there is no function body to synthesise an ISR prologue around. Shares
/// the P0250 code with the `@no_frame` non-lambda rejection.
#[test]
fn interrupt_on_non_fn_binding_emits_p0250() {
    let source = r#"pub let CONST : u64 = 42 @interrupt("breakpoint")"#;
    let (_arena, result, diags) = parse_let_in_module(source);

    assert!(result.is_err(), "@interrupt on non-fn must fail parse");
    let has_p0250 = diags.iter().any(|d| {
        let code = d.code();
        code.category() == paideia_as_diagnostics::Category::P && code.number() == 250
    });
    assert!(has_p0250, "expected P0250 diagnostic, got: {:?}", diags);
}

/// `@interrupt` composes with other attributes (e.g. `@abi("sysv")`) in a
/// single trailing attribute run. Guards against a future refactor that
/// accidentally short-circuits the loop once `interrupt` is seen.
#[test]
fn interrupt_composes_with_abi() {
    let source = r#"pub let isr : () -> () = fn() -> 0 @interrupt("nmi") @abi("sysv")"#;
    let (arena, result, diags) = parse_let_in_module(source);

    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(result.is_ok(), "expected successful parse");

    let root = result.unwrap();
    match arena.item_data(root) {
        Some(paideia_as_ast::ItemData::Let { interrupt: Some(attr), abi, .. }) => {
            assert_eq!(attr.vector, 2, "nmi → vector 2");
            assert_eq!(*abi, Some(paideia_as_ast::CallingConvention::Sysv));
        }
        other => panic!("expected ItemData::Let with interrupt+abi, got {:?}", other),
    }
}
