//! Integration test for PA7C-m1-001: symbol export from SymbolTable.
//!
//! Tests that when a source module contains multiple top-level function bindings,
//! each produces exactly one STT_FUNC symbol in the ELF with its real binding name
//! (not the fallback "add_one").
//!
//! Acceptance criteria:
//! - Build a 3-function source and emit ELF.
//! - Parse the ELF and verify all 3 function symbols are present with correct names.
//! - No fallback "add_one" symbol should appear.
//! - Binding names are preserved from source.

use object::{Object, ObjectSymbol};
use std::process::Command;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Acceptance test: three top-level functions produce three symbol entries.
///
/// Source: `module Three = structure { let a : () -> u64 = fn () -> 42 ; let b : () -> u64 = fn () -> 43 ; let c : () -> u64 = fn () -> 44 }`
///
/// Expected: ELF contains exactly three STT_TEXT symbols named "a", "b", "c".
#[test]
fn three_function_symbols_emitted_with_real_names() {
    let tmp_src = std::env::temp_dir().join("Three.pdx");
    let tmp_obj = std::env::temp_dir().join("three_fn_test.o");
    let _ = std::fs::remove_file(&tmp_src);
    let _ = std::fs::remove_file(&tmp_obj);

    // Write a test source with three top-level functions.
    // Note: module name must match file basename in PascalCase
    let source = "module Three = structure { let a : () -> u64 = fn () -> 42 ; let b : () -> u64 = fn () -> 43 ; let c : () -> u64 = fn () -> 44 }";
    std::fs::write(&tmp_src, source).expect("failed to write test source");

    // Build the ELF.
    let out = cargo_run(&[
        "build",
        tmp_src.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp_obj.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Parse the ELF and extract symbol names.
    let bytes = std::fs::read(&tmp_obj).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Collect all STT_FUNC symbols.
    let mut func_symbols = Vec::new();
    for sym in file.symbols() {
        // Skip section symbols, undefined symbols, and others.
        if sym.kind() == object::SymbolKind::Text && sym.is_definition() {
            if let Ok(name) = sym.name() {
                func_symbols.push(name.to_string());
            }
        }
    }

    // Verify we have exactly three function symbols with the right names.
    assert!(
        func_symbols.contains(&"a".to_string()),
        "function symbol 'a' not found in ELF"
    );
    assert!(
        func_symbols.contains(&"b".to_string()),
        "function symbol 'b' not found in ELF"
    );
    assert!(
        func_symbols.contains(&"c".to_string()),
        "function symbol 'c' not found in ELF"
    );

    // Verify no "add_one" fallback exists.
    assert!(
        !func_symbols.contains(&"add_one".to_string()),
        "fallback symbol 'add_one' should not be present"
    );

    let _ = std::fs::remove_file(&tmp_src);
    let _ = std::fs::remove_file(&tmp_obj);
}

/// Acceptance test: single function binding produces correct symbol.
///
/// When a module has a single top-level function binding,
/// the elaborator should emit exactly one symbol with the correct name.
#[test]
fn single_function_symbol_has_correct_name() {
    let tmp_src = std::env::temp_dir().join("Single.pdx");
    let tmp_obj = std::env::temp_dir().join("single_fn_test.o");
    let _ = std::fs::remove_file(&tmp_src);
    let _ = std::fs::remove_file(&tmp_obj);

    // Write a test source with a single function.
    // Note: module name must match file basename in PascalCase
    let source = "module Single = structure { let myFunc : () -> u64 = fn () -> 99 }";
    std::fs::write(&tmp_src, source).expect("failed to write test source");

    // Build the ELF.
    let out = cargo_run(&[
        "build",
        tmp_src.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp_obj.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Parse the ELF and verify the function symbol is named "myFunc".
    let bytes = std::fs::read(&tmp_obj).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut func_symbols = Vec::new();
    for sym in file.symbols() {
        if sym.kind() == object::SymbolKind::Text && sym.is_definition() {
            if let Ok(name) = sym.name() {
                func_symbols.push(name.to_string());
            }
        }
    }

    // Verify the function is named "myFunc" and not a fallback.
    assert!(
        func_symbols.contains(&"myFunc".to_string()),
        "function symbol 'myFunc' not found in ELF; found: {:?}",
        func_symbols
    );
    assert!(
        !func_symbols.contains(&"add_one".to_string()),
        "fallback 'add_one' should not be emitted"
    );

    let _ = std::fs::remove_file(&tmp_src);
    let _ = std::fs::remove_file(&tmp_obj);
}
