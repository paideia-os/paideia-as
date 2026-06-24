//! Tests for #![bits = N] inner attribute parsing
//!
//! Validates that the parser correctly handles the bits inner attribute
//! in module and structure scopes, and emits the correct diagnostics for
//! invalid values (B0010 for 16-bit, P0301 for other invalid values).

use paideia_as_ast::{AttrValue, ItemAttribute, ItemData, NodeKind};
use paideia_as_diagnostics::{Diagnostic, DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

fn parse_and_check(
    source: &str,
) -> (
    paideia_as_ast::AstArena,
    Result<paideia_as_ast::NodeId, paideia_as_parser::ParseError>,
    Vec<Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = paideia_as_ast::AstArena::new();
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let mut collector = VecSink::new();
    let tokens = lex.collect_tokens(&mut collector);
    // Forward lexer diagnostics into the main sink
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
fn module_head_bits_32_parses() {
    // Test: module-head inner attribute for bits=32
    let source = "#![bits = 32]\nlet foo = 0";
    let (arena, result, diags) = parse_and_check(source);

    // Should parse successfully without errors
    assert!(result.is_ok(), "should parse bits=32 module-head attribute");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "should have no parse errors for bits=32");

    // Verify inner_attrs on root Structure
    let root = result.unwrap();
    if let Some(node) = arena.get(root) {
        if let NodeKind::Structure = node.kind {
            if let Some(ItemData::Structure {
                inner_attrs,
                items: _,
                doc: _,
            }) = arena.item_data(root)
            {
                assert_eq!(inner_attrs.len(), 1, "should have one inner attribute");
                if let ItemAttribute::InnerAttr { name: _, value } = &inner_attrs[0] {
                    if let AttrValue::Int(bits_val) = value {
                        assert_eq!(*bits_val, 32, "bits value should be 32");
                    } else {
                        panic!("expected Int value");
                    }
                } else {
                    panic!("expected InnerAttr variant");
                }
            }
        }
    }
}

#[test]
fn scope_head_bits_32_parses() {
    // Test: scope-head inner attribute inside a module structure
    // Module declaration with structure that has inner attributes
    let source = "module M = structure { #![bits = 32] }";
    let (arena, result, diags) = parse_and_check(source);

    // Should parse successfully
    assert!(
        result.is_ok(),
        "should parse bits=32 scope-head attribute"
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "should have no parse errors for scope-head bits=32"
    );

    // Verify inner_attrs on the module's structure body
    let root = result.unwrap();
    if let Some(node) = arena.get(root) {
        if let NodeKind::Structure = node.kind {
            if let Some(ItemData::Structure {
                items,
                inner_attrs: _,
                doc: _,
            }) = arena.item_data(root)
            {
                // The module declaration is in items[0]
                if let Some(&module_id) = items.first() {
                    if let Some(ItemData::Module {
                        body,
                        inner_attrs: _,
                        ..
                    }) = arena.item_data(module_id)
                    {
                        // body points to the Structure
                        if let Some(ItemData::Structure {
                            inner_attrs,
                            items: _,
                            doc: _,
                        }) = arena.item_data(*body)
                        {
                            assert_eq!(
                                inner_attrs.len(),
                                1,
                                "module structure should have one inner attribute"
                            );
                            if let ItemAttribute::InnerAttr { name: _, value } = &inner_attrs[0]
                            {
                                if let AttrValue::Int(bits_val) = value {
                                    assert_eq!(*bits_val, 32, "bits value should be 32");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn nested_scope_shadows_outer() {
    // Test: nested scope-head attributes override outer scope attributes
    let source = "#![bits = 32]\nmodule M = structure { #![bits = 64] }";
    let (arena, result, diags) = parse_and_check(source);

    assert!(result.is_ok(), "should parse nested attributes");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "should have no parse errors");

    // Verify that both attributes are present and have correct values
    let root = result.unwrap();
    if let Some(node) = arena.get(root) {
        if let NodeKind::Structure = node.kind {
            if let Some(ItemData::Structure {
                items,
                inner_attrs: root_attrs,
                doc: _,
            }) = arena.item_data(root)
            {
                assert_eq!(root_attrs.len(), 1, "root should have module-head attribute");
                if let ItemAttribute::InnerAttr { name: _, value } = &root_attrs[0] {
                    if let AttrValue::Int(bits_val) = value {
                        assert_eq!(*bits_val, 32, "module-head bits should be 32");
                    }
                }

                if let Some(&module_id) = items.first() {
                    if let Some(ItemData::Module {
                        body,
                        inner_attrs: _,
                        ..
                    }) = arena.item_data(module_id)
                    {
                        if let Some(ItemData::Structure {
                            inner_attrs,
                            items: _,
                            doc: _,
                        }) = arena.item_data(*body)
                        {
                            assert_eq!(inner_attrs.len(), 1, "nested should have scope-head attribute");
                            if let ItemAttribute::InnerAttr { name: _, value } = &inner_attrs[0] {
                                if let AttrValue::Int(bits_val) = value {
                                    assert_eq!(*bits_val, 64, "scope-head bits should be 64");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn bits_16_emits_b0010() {
    // Test: bits=16 emits B1700 error (16-bit mode not supported)
    let source = "#![bits = 16]";
    let (_arena, result, diags) = parse_and_check(source);

    // Should still parse (error is a diagnostic, not a parse error)
    assert!(result.is_ok(), "should parse despite B1700");

    // Look for B1700 diagnostic
    let b1700_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code().category().letter() == 'B' && d.code().number() == 1700)
        .collect();
    assert!(
        !b1700_diags.is_empty(),
        "should emit at least one B1700 diagnostic for bits=16"
    );
}

#[test]
fn bits_64_is_noop() {
    // Test: bits=64 parses without diagnostics
    let source = "#![bits = 64]";
    let (_arena, result, diags) = parse_and_check(source);

    assert!(result.is_ok(), "should parse bits=64");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "should have no errors for bits=64");
}

#[test]
fn bits_99_emits_p0301() {
    // Test: bits=99 emits P0240 error (invalid bits value)
    let source = "#![bits = 99]";
    let (_arena, result, diags) = parse_and_check(source);

    // Should still parse
    assert!(result.is_ok(), "should parse despite P0240");

    // Look for P0240 diagnostic
    let p0240_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code().category().letter() == 'P' && d.code().number() == 240)
        .collect();
    assert!(
        !p0240_diags.is_empty(),
        "should emit at least one P0240 diagnostic for bits=99"
    );
}
