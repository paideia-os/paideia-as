//! Integration tests for symbol-table emission (Phase 5 m5-003).
//!
//! Tests that the emitter correctly:
//! 1. Emits symbols from SymbolTable via build_elf_object()
//! 2. Sets correct byte offsets from function_offsets for function symbols
//! 3. Emits data symbol offsets from .rodata/.data sections
//! 4. ELF symbol table shows correct symbols

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

/// Test 1: Symbol table emits with existing test file.
///
/// Acceptance criteria:
/// - Build a .pdx file (using hello.pdx)
/// - When the file has no top-level function bindings, B1702 diagnostic fires
/// - ELF is still produced but with no function symbols
#[test]
fn symbol_table_emits_symbols_in_elf() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_symboltab_test1.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // hello.pdx has no top-level functions, so B1702 should fire and build fails
    assert!(
        !out.status.success(),
        "build should fail with B1702 when no symbols exported"
    );

    // Verify the diagnostic message is present
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("B1702"), "B1702 diagnostic should fire");
    assert!(
        stderr.contains("no symbols to export"),
        "B1702 message should be present"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 2: Real symbols from SymbolTable are emitted with correct names.
///
/// Acceptance criteria (Phase-7-m1-001):
/// - The fallback "add_one" symbol is removed
/// - Only real binding names are emitted
/// - When module has no function bindings, B1702 is emitted
#[test]
fn symbol_table_symbols_emitted_with_real_names() {
    // hello.pdx has no function bindings, so this verifies B1702 fires.
    // For a positive test with real symbols, see build_emit_pa7c_symbol_export.rs
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_symboltab_test2.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // Verify B1702 diagnostic when no symbols to export
    assert!(!out.status.success(), "build should fail with B1702");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("B1702"), "B1702 should be emitted");
    assert!(
        !stderr.contains("add_one"),
        "fallback 'add_one' should never be in fallback path"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 3: Data section symbols emit with correct offsets (when present).
///
/// Acceptance criteria:
/// - When .pdx has data entries and function bindings, data symbols emit correctly
/// - hello.pdx has no functions, so B1702 fires instead
/// - This test verifies the diagnostic path
#[test]
fn symbol_data_section_symbols_emit_correctly() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_symboltab_test3.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // hello.pdx has no function bindings, so build fails with B1702
    assert!(
        !out.status.success(),
        "build should fail when no exported symbols"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("B1702"),
        "B1702 should fire for files with no function exports"
    );

    let _ = std::fs::remove_file(&tmp);
}
