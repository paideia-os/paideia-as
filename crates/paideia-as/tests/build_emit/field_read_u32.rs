//! pa-r17-005-e (#1079): Global record-field read emitter
//!
//! This test verifies that `fn (_: ()) -> r.a` where r is a module-level struct
//! lowers to emit_mem_read_via_rip_sym, producing mov eax, [rip + r]; ret bytecode.
//!
//! Expected behavior:
//! - Parser accepts struct definition and field access in lambda return
//! - IR lowering emits IrKind::FieldAccess as lambda body
//! - visit_lambda FieldAccess arm detects module-level symbol and field info
//! - emit_mem_read_via_rip_sym emits MovSized{W32} + MemRipRelSym operands
//! - Bytecode contains 8B 05 (mov eax, [rip+...]) followed by C3 (ret)
//! - .rela.text contains PC32 reloc against symbol 'r'

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
fn field_read_u32_builds_successfully() {
    // pa-r17-005-e AC1: field_read_u32.pdx builds without error.
    // Verifies that:
    // - Parser accepts struct definition and field access in lambda
    // - Lowering creates FieldAccess IR nodes
    // - No compilation errors for module-level struct field read
    let input = build_emit_data("field_read_u32.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_field_read_u32.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "field_read_u32 should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn field_read_u32_emits_correct_bytes() {
    // pa-r17-005-e AC2: fn(_: ()) -> r.a emits mov eax, [rip+0]; ret
    // Expected byte sequence: 8B 05 (mov eax, [rip+...]) followed by C3 (ret)
    // This verifies emit_mem_read_via_rip_sym correctly lowers FieldAccess to RIP-relative load.
    let input = build_emit_data("field_read_u32.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_field_read_u32_emit.o");
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
        "build failed for field_read_u32: {}",
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

    // Assert: must contain C3 (ret) — ideally after the mov
    assert!(
        text_bytes.iter().any(|&b| b == 0xC3),
        "expected C3 (ret) in .text, got: {:02X?}",
        text_bytes
    );

    // Assert: must NOT contain FF 15 bytes (the wrong indirect call opcode from workerbee's earlier attempt)
    assert!(
        !text_bytes.windows(2).any(|w| w == [0xFF, 0x15]),
        "unexpected FF 15 (indirect call) in .text — field read should use direct mov eax, [rip+...], not indirect call"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn field_read_u32_has_reloc() {
    // pa-r17-005-e AC3: .rela.text contains PC32 relocation against symbol 'r'
    // This verifies that the relocation record is correctly generated for the RIP-relative reference.
    let input = build_emit_data("field_read_u32.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_field_read_u32_reloc.o");
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
        "build failed for field_read_u32: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find .text section and iterate its relocations
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");

    // Iterate relocations and check for at least one targeting symbol 'r'
    let mut found_r_reloc = false;
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "r" {
                found_r_reloc = true;
                break;
            }
        }
    }

    assert!(
        found_r_reloc,
        ".rela.text should contain relocation against symbol 'r'"
    );

    let _ = std::fs::remove_file(&tmp);
}
