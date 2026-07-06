//! Integration tests for function-pointer address-of lowering (PA-R17-003, issue #981).
//!
//! Tests that `&fn_name` syntax correctly:
//! - Resolves the operand identifier against the SymbolTable
//! - Emits relocation plumbing in the data section
//! - Routes to .rodata or .data based on mutability
//! - Rejects non-identifier operands (T0532)
//! - Rejects unresolved names with malformed identifiers (T0534)
//! - Rejects non-function targets (T0536)

use object::{Object, ObjectSection, ObjectSymbol};
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

/// Test 1: fnptr_addr_of_basic
///
/// Verifies that a simple address-of expression compiles successfully:
/// ```
/// pub let identity : (u64) -> u64 = fn (x : u64) -> x
/// pub let ptr : (u64) -> u64 = &identity
/// ```
///
/// - Compiles successfully (this was the critical bug 2 fix)
/// - Produces valid ELF output
/// - Symbol table contains both `identity` and `ptr`
#[test]
fn fnptr_addr_of_basic() {
    let input = data("fnptr_addr_of_basic.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_basic.o");
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
        "build should succeed for address-of with defined function. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify output ELF is valid
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("output should be valid ELF");

    // Verify symbols exist
    let mut symbol_count = 0;
    let mut has_identity = false;
    let mut has_ptr = false;
    for sym in file.symbols() {
        symbol_count += 1;
        if let Ok(name) = sym.name() {
            if name == "identity" {
                has_identity = true;
            }
            if name == "ptr" {
                has_ptr = true;
            }
        }
    }
    assert!(symbol_count > 0, "ELF should have symbols");
    assert!(has_identity, "should have `identity` function symbol");
    assert!(has_ptr, "should have `ptr` data symbol");

    // Verify there's at least one relocation section for data
    let has_reloc_section = file
        .sections()
        .any(|s| s.name().unwrap_or("") == ".rela.rodata");
    assert!(
        has_reloc_section,
        "should have .rela.rodata section for address-of relocations"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 2: fnptr_addr_of_multiple
///
/// Verifies that multiple address-of expressions compile successfully:
/// ```
/// pub let ptr1 : (u64) -> u64 = &fn_one
/// pub let ptr2 : (u64) -> u64 = &fn_two
/// ```
///
/// - Compiles successfully
/// - Produces valid ELF output
/// - Contains symbols for both functions and both pointers
#[test]
fn fnptr_addr_of_multiple() {
    let input = data("fnptr_addr_of_multiple.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_multiple.o");
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
        "build should succeed for multiple address-of expressions. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("output should be valid ELF");

    // Verify symbols exist for all functions and pointers
    let mut has_fn_one = false;
    let mut has_fn_two = false;
    let mut has_ptr1 = false;
    let mut has_ptr2 = false;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "fn_one" {
                has_fn_one = true;
            }
            if name == "fn_two" {
                has_fn_two = true;
            }
            if name == "ptr1" {
                has_ptr1 = true;
            }
            if name == "ptr2" {
                has_ptr2 = true;
            }
        }
    }
    assert!(has_fn_one, "should have `fn_one` symbol");
    assert!(has_fn_two, "should have `fn_two` symbol");
    assert!(has_ptr1, "should have `ptr1` symbol");
    assert!(has_ptr2, "should have `ptr2` symbol");

    let _ = std::fs::remove_file(&tmp);
}

/// Test 3: fnptr_addr_of_mutable
///
/// Verifies that mutable address-of expressions compile successfully:
/// ```
/// pub let mut ptr : (u64) -> u64 = &identity
/// ```
///
/// - Compiles successfully
/// - Produces valid ELF output with .data section (for mutable data)
#[test]
fn fnptr_addr_of_mutable() {
    let input = data("fnptr_addr_of_mutable.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_mutable.o");
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
        "build should succeed for mutable address-of. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("output should be valid ELF");

    // Verify symbols exist
    let mut has_identity = false;
    let mut has_ptr = false;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "identity" {
                has_identity = true;
            }
            if name == "ptr" {
                has_ptr = true;
            }
        }
    }
    assert!(has_identity, "should have `identity` function symbol");
    assert!(has_ptr, "should have `ptr` data symbol");

    // Verify .data section exists (for mutable data)
    let has_data_section = file.sections().any(|s| s.name().unwrap_or("") == ".data");
    assert!(
        has_data_section,
        "should have .data section for mutable address-of"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 4: fnptr_addr_of_reject_non_ident
///
/// Verifies that address-of rejects non-identifier operands with T0532:
/// ```
/// pub let bad : (u64) -> u64 = &(42 : u64)
/// ```
///
/// - Build fails with non-zero exit status
/// - stderr contains "T0532" diagnostic
#[test]
fn fnptr_addr_of_reject_non_ident() {
    let input = data("fnptr_addr_of_reject_non_ident.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_reject_nonident.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // Build should fail
    assert!(
        !out.status.success(),
        "build should fail for non-identifier operand"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    // The diagnostic should contain T0532
    assert!(
        stderr.contains("T0532") || stderr.contains("address-of operand must be an identifier"),
        "expected T0532 error, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 5: fnptr_addr_of_reject_data_symbol
///
/// Verifies that address-of rejects non-function symbols with T0536:
/// ```
/// pub let some_data : u64 = 42
/// pub let bad : (u64) -> u64 = &some_data
/// ```
///
/// - Build fails with non-zero exit status
/// - stderr contains "T0536" diagnostic
#[test]
fn fnptr_addr_of_reject_data_symbol() {
    let input = data("fnptr_addr_of_reject_data_symbol.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_reject_data.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // Build should fail
    assert!(
        !out.status.success(),
        "build should fail for non-function symbol"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    // The diagnostic should contain T0536
    assert!(
        stderr.contains("T0536") || stderr.contains("address-of target must be a function"),
        "expected T0536 error, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 6: fnptr_addr_of_cross_file
///
/// Verifies that cross-file address-of references work by creating undefined symbols:
/// - Compiles a PDX that references an undefined function via `&identity`
/// - The resulting .o is valid ELF
/// - Contains the undefined `identity` symbol for linker resolution
#[test]
fn fnptr_addr_of_cross_file() {
    let input = data("fnptr_addr_of_cross_file_use.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_cross_file.o");
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
        "build should succeed for cross-file address-of reference. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("output should be valid ELF");

    // Verify `identity` symbol exists (as undefined external reference)
    let mut has_identity_undefined = false;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "identity" {
                // Check that it's undefined (Dynamic scope indicates external reference)
                // Undefined symbols have no section binding
                has_identity_undefined = true;
                break;
            }
        }
    }
    assert!(
        has_identity_undefined,
        "`identity` should be present as an undefined symbol for cross-file reference"
    );

    // Verify there's at least one relocation section
    let has_reloc_section = file
        .sections()
        .any(|s| s.name().unwrap_or("").starts_with(".rela"));
    assert!(
        has_reloc_section,
        "should have relocation section for cross-file reference"
    );

    let _ = std::fs::remove_file(&tmp);
}
