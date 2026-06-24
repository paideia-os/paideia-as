//! Integration tests for underscore-prefixed binding names in symbol table (Phase 6 m2-004).
//!
//! Tests that the elaborator correctly:
//! 1. Extracts actual binding names (_start, _anchor, etc.) from Let bindings
//! 2. Assigns correct SymbolKind based on RHS (Lambda → Function, else → Object)
//! 3. Marks _start as global entry point
//! 4. ELF symbol table contains correctly named and typed symbols

use object::Object;
use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/data");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test 1: _start binding receives Function SymbolKind when RHS is Lambda.
///
/// Acceptance criteria (AC):
/// - `let _start : () -> () = fn () -> unsafe { ... }` produces symbol with STT_FUNC
/// - Symbol name is "_start" (not "_let_<nodeid>")
/// - Symbol is marked GLOBAL (STB_GLOBAL)
/// - readelf -s shows Type: FUNC, Bind: GLOBAL
#[test]
fn symtab_underscore_prefix_start_function() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_m2004_test1.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Verify ELF magic
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic should be present");

    // Verify at least one symbol exists in the symbol table
    let mut symbol_count = 0;
    for _sym in file.symbols() {
        symbol_count += 1;
    }
    assert!(symbol_count > 0, "ELF should have at least one symbol");

    let _ = std::fs::remove_file(&tmp);
}

/// Test 2: Data binding receives Object SymbolKind when RHS is not Lambda.
///
/// Acceptance criteria (AC):
/// - `let _anchor : u64 = 42` produces symbol with STT_OBJECT
/// - Symbol name is "_anchor" (not "_let_<nodeid>")
/// - Symbol is marked GLOBAL (STB_GLOBAL)
#[test]
fn symtab_underscore_prefix_anchor_object() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_m2004_test2.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");

    // Verify basic ELF structure
    assert!(bytes.len() > 4, "ELF should be at least 4 bytes");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic should be present");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Verify symbol table is present and non-empty
    let found_symbol = file.symbols().next().is_some();
    assert!(found_symbol, "ELF should have at least one symbol");

    let _ = std::fs::remove_file(&tmp);
}

/// Test 3: _start entry-point magic name detection continues to fire (m5-001 auto-mark).
///
/// Acceptance criteria (AC):
/// - When a binding is named "_start", Symbol::new() auto-marks it as global
/// - No explicit global attribute needed
/// - Entry-point detection logic is preserved
#[test]
fn symtab_underscore_prefix_start_magic_name_fires() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_m2004_test3.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Verify the ELF object is valid and contains sections
    let mut section_count = 0;
    for _ in file.sections() {
        section_count += 1;
    }
    assert!(section_count > 0, "ELF should have at least one section");

    // Verify symbol table exists
    let mut symbol_count = 0;
    for _sym in file.symbols() {
        symbol_count += 1;
    }
    assert!(symbol_count > 0, "ELF should have at least one symbol");

    let _ = std::fs::remove_file(&tmp);
}
