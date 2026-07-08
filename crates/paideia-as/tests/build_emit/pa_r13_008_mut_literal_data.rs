//! Integration test for PA-R13-008: mutable literal globals → .data (not .rodata).
//!
//! Verifies that `pub let mut X : u64 = <literal>` and `pub let mut arr = [...]`
//! emit to .data section instead of .rodata section.
//!
//! Root cause: two branches in cmd_build.rs (IrKind::Literal and IrKind::ArrayLit)
//! unconditionally called DataEntry::new_rodata(...) without consulting let_meta().mutable.
//! The StringLiteral branch already had the correct mutable-aware routing.

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

fn build_emit_data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/build-emit");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn compile_pdx(input_path: &str) -> Vec<u8> {
    let tmp = std::env::temp_dir().join("paideia_as_pa_r13_008_test.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input_path,
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

    std::fs::read(&tmp).expect("output ELF should exist")
}

fn assert_symbol_in_section(elf_bytes: &[u8], symbol_name: &str, expected_section: &str) {
    let elf = object::File::parse(elf_bytes).expect("should parse as valid ELF64");

    let symbols: Vec<_> = elf.symbols().collect();
    let symbol = symbols
        .iter()
        .find(|sym| sym.name().unwrap_or("") == symbol_name)
        .unwrap_or_else(|| panic!("symbol '{}' should exist", symbol_name));

    let section_idx = symbol
        .section_index()
        .unwrap_or_else(|| panic!("symbol '{}' should have section index", symbol_name));
    let section = elf
        .section_by_index(section_idx)
        .unwrap_or_else(|_| panic!("section index {} should exist", section_idx.0));
    let section_name = section.name().unwrap_or("<unnamed>");

    assert_eq!(
        section_name, expected_section,
        "symbol '{}' should be in '{}' section, but is in '{}' section",
        symbol_name, expected_section, section_name
    );
}

fn assert_section_size(elf_bytes: &[u8], section_name: &str, min_size: u64) {
    let elf = object::File::parse(elf_bytes).expect("should parse as valid ELF64");

    let mut found = false;
    for section in elf.sections() {
        if section.name().unwrap_or("") == section_name {
            let size = section.size();
            assert!(
                size >= min_size,
                "section '{}' should have size >= {}, but has {}",
                section_name, min_size, size
            );
            found = true;
            break;
        }
    }
    assert!(found, "section '{}' should exist", section_name);
}

#[test]
fn pa_r13_008_mut_scalar_literal_goes_to_data() {
    // Test 1: pub let mut counter : u64 = 0 → .data
    // This is the regression guard: mutable scalar with literal initializer
    // must live in .data (writable), not .rodata (read-only).
    let input = build_emit_data("pa_r13_008_mut_scalar.pdx");
    let elf_bytes = compile_pdx(input.to_str().unwrap());

    assert_symbol_in_section(&elf_bytes, "counter", ".data");
    assert_section_size(&elf_bytes, ".data", 8);
}

#[test]
fn pa_r13_008_immutable_scalar_literal_stays_in_rodata() {
    // Test 2: pub let counter : u64 = 0 (immutable) → .rodata
    // Guards against over-correction: immutable literals should still go to .rodata.
    let input = build_emit_data("pa_r13_008_immut_scalar.pdx");
    let elf_bytes = compile_pdx(input.to_str().unwrap());

    assert_symbol_in_section(&elf_bytes, "counter", ".rodata");
}

#[test]
fn pa_r13_008_mut_array_uninit_goes_to_bss() {
    // Test 3: pub let mut arr : [u64; 4] = uninit → .bss
    // Placeholder branch (uninit) should stay in .bss regardless of mutability.
    // This test verifies we didn't accidentally break the Placeholder branch.
    let input = build_emit_data("pa_r13_008_mut_array_uninit.pdx");
    let elf_bytes = compile_pdx(input.to_str().unwrap());

    assert_symbol_in_section(&elf_bytes, "arr", ".bss");
}

#[test]
fn pa_r13_008_mut_array_literal_goes_to_data() {
    // Test 4: pub let mut buf : [u64; 4] = [1, 2, 3, 4] → .data
    // Mutable array with literal initializer (ArrayLit branch) must live in .data.
    let input = build_emit_data("pa_r13_008_mut_array_literal.pdx");
    let elf_bytes = compile_pdx(input.to_str().unwrap());

    assert_symbol_in_section(&elf_bytes, "buf", ".data");
    assert_section_size(&elf_bytes, ".data", 32); // 4 x u64 = 32 bytes
}

#[test]
fn pa_r13_008_immutable_array_literal_stays_in_rodata() {
    // Guard: immutable array literal should stay in .rodata.
    let input = build_emit_data("pa_r13_008_immut_array_literal.pdx");
    let elf_bytes = compile_pdx(input.to_str().unwrap());

    assert_symbol_in_section(&elf_bytes, "buf", ".rodata");
}
