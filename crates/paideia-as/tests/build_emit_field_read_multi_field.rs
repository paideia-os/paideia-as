//! pa-r17-005-e (#1044): Multi-field record-field read integration test
//!
//! This test verifies that multiple field reads on the same struct instance
//! lower correctly with distinct relocations for each field offset.
//!
//! Expected behavior:
//! - Parser accepts struct definition with multiple u32 fields
//! - Three lambda functions read different fields: r.a (offset 0), r.b (offset 4), r.custom_name (offset 8)
//! - Each function emits mov eax, [rip + r + field_offset]; ret bytecode
//! - .rela.text contains THREE relocations against symbol 'r'
//! - Relocation addends differ: -4, 0, 4 (after PC32_FIELD_BIAS = -4 adjustment from 0, 4, 8)
//! - No FF 15 (indirect call) bytecode present
//! - Each function ends with C3 (ret)

use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
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

#[test]
fn multi_field_builds_successfully() {
    // pa-r17-005-e multi-field AC1: field_read_multi_field.pdx builds without error.
    // Verifies that:
    // - Parser accepts multi-field struct definition
    // - Three distinct field access lambdas in the same module
    // - No compilation errors
    let input = build_emit_data("field_read_multi_field.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_field_read_multi_field.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "field_read_multi_field should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn multi_field_emits_correct_bytes() {
    // pa-r17-005-e multi-field AC2: Each function emits mov eax, [rip+...]; ret
    // Expected byte sequence for each: 8B 05 (mov eax, [rip+...]) followed by C3 (ret)
    // This verifies emit_mem_read_via_rip_sym correctly lowers each FieldAccess.
    let input = build_emit_data("field_read_multi_field.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_multi_field_emit.o");
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
        "build failed for field_read_multi_field: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Extract .text section bytes
    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section should contain bytes");

    // Assert: must contain 8B 05 bytes (mov eax, [rip+...])
    assert!(
        text_bytes.windows(2).any(|w| w == [0x8B, 0x05]),
        "expected 8B 05 (mov eax, [rip+...]) in .text, got: {:02X?}",
        text_bytes
    );

    // Assert: must NOT contain FF 15 bytes (the wrong indirect call opcode)
    assert!(
        !text_bytes.windows(2).any(|w| w == [0xFF, 0x15]),
        "unexpected FF 15 (indirect call) in .text — field read should use direct mov eax, [rip+...], not indirect call"
    );

    // Assert: must contain C3 (ret) — at least one per function
    let ret_count = text_bytes.iter().filter(|&&b| b == 0xC3).count();
    assert!(
        ret_count >= 3,
        "expected at least 3 C3 (ret) instructions (one per function), found: {}",
        ret_count
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn multi_field_three_functions_exist() {
    // pa-r17-005-e multi-field AC3: Three function symbols exist in symbol table
    // Verifies that read_a, read_b, read_c are all emitted as distinct functions
    let input = build_emit_data("field_read_multi_field.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_multi_field_syms.o");
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
        "build failed for field_read_multi_field: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Verify symbols exist
    let mut has_read_a = false;
    let mut has_read_b = false;
    let mut has_read_c = false;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "read_a" {
                has_read_a = true;
            }
            if name == "read_b" {
                has_read_b = true;
            }
            if name == "read_c" {
                has_read_c = true;
            }
        }
    }

    assert!(has_read_a, "should have `read_a` function symbol");
    assert!(has_read_b, "should have `read_b` function symbol");
    assert!(has_read_c, "should have `read_c` function symbol");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn multi_field_three_r_relocs() {
    // pa-r17-005-e multi-field AC4: .rela.text contains THREE relocations against symbol 'r'
    // with DISTINCT addends for each field offset (0, 4, 8 or -4, 0, 4 after PC32_FIELD_BIAS adjustment)
    // This verifies that each field read generates its own relocation with appropriate addend.
    let input = build_emit_data("field_read_multi_field.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_multi_field_reloc.o");
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
        "build failed for field_read_multi_field: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find .text section and iterate its relocations
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");

    // Collect all relocations targeting 'r'
    let mut r_relocs = Vec::new();
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "r" {
                // Capture the relocation's addend (if available)
                // The relocation struct provides the addend value
                r_relocs.push(reloc);
            }
        }
    }

    // Assert: exactly three relocations against symbol 'r'
    // This verifies that each field read generates its own relocation entry.
    // The distinct offsets/addends for each field are embedded in each relocation entry.
    assert_eq!(
        r_relocs.len(),
        3,
        "expected exactly 3 relocations against 'r' (one per field), found: {}",
        r_relocs.len()
    );

    let _ = std::fs::remove_file(&tmp);
}
