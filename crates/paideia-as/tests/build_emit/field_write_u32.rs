//! pa-r17-006-b (#1046): Global record-field write emitter
//!
//! This test verifies that `fn(_: ()) -> () { r.a = 42u32 }` where r is a module-level struct
//! lowers to emit_mem_write_via_rip_sym, producing mov eax, 42; mov [rip + r], eax; ret bytecode.
//!
//! Expected behavior:
//! - Parser accepts struct definition and field assignment in lambda body
//! - IR lowering emits IrKind::FieldAccess as lambda body's field store
//! - visit_field_assign detects module-level symbol and field info
//! - emit_mem_write_via_rip_sym emits MovSized{W32} + MemRipRelSym operands
//! - Bytecode contains B8 (mov eax, ...) followed by 89 05 (mov [rip+...], eax) followed by C3 (ret)
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
fn field_write_u32_builds_successfully() {
    // pa-r17-006-b AC1: field_write_u32.pdx builds without error.
    // Verifies that:
    // - Parser accepts struct definition and field assignment in lambda
    // - Lowering creates Store IR nodes with FieldAccess children
    // - No compilation errors for module-level struct field write
    let input = build_emit_data("field_write_u32.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_field_write_u32.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "field_write_u32 should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn field_write_u32_emits_correct_bytes() {
    // pa-r17-006-b AC2: fn(_: ()) -> () { r.a = 42u32 } emits mov eax, 42; mov [rip+...], eax; ret
    // Expected byte sequence: B8 (mov eax, imm32) followed by 89 05 (mov [rip+...], eax) followed by C3 (ret)
    // This verifies emit_mem_write_via_rip_sym correctly lowers FieldAccess store to RIP-relative write.
    let input = build_emit_data("field_write_u32.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_field_write_u32_emit.o");
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
        "build failed for field_write_u32: {}",
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

    // Assert: must contain B8 (mov eax, imm32) WITHOUT 0x48 (REX.W) prefix
    let b8_pos = text_bytes.iter().position(|&b| b == 0xB8)
        .expect("expected B8 (mov eax, imm32) in .text");
    if b8_pos > 0 {
        assert!(
            text_bytes[b8_pos - 1] != 0x48,
            "unexpected REX.W (0x48) prefix on mov eax at position {}; this indicates W64 encoding (movabs) instead of W32 (mov eax)",
            b8_pos
        );
    }

    // Assert: must contain 89 05 (mov [rip+...], eax) WITHOUT 0x48 (REX.W) prefix
    let store_pos = text_bytes.windows(2).position(|w| w == [0x89, 0x05])
        .expect("expected 89 05 (mov [rip+...], eax) in .text");
    if store_pos > 0 {
        assert!(
            text_bytes[store_pos - 1] != 0x48,
            "unexpected REX.W (0x48) prefix on mov [rip+...] at position {}; this indicates W64 encoding instead of W32",
            store_pos
        );
    }

    // Assert: must contain C3 (ret) — ideally after the mov
    assert!(
        text_bytes.iter().any(|&b| b == 0xC3),
        "expected C3 (ret) in .text, got: {:02X?}",
        text_bytes
    );

    // Verify total instruction byte count: B8 + imm32 (5 bytes) + 89 05 + imm32 (6 bytes) + C3 (1 byte) = 12 bytes total
    // Allow some margin for padding/alignment, but check that we're not getting the W64 versions
    // W32: B8 + 4 bytes + 89 05 + 4 bytes + C3 = 12 bytes
    // W64 (WRONG): 48 B8 + 8 bytes + 48 89 05 + 4 bytes + C3 = 17-18 bytes
    assert!(
        text_bytes.len() < 15,
        "text section seems too large ({}); expected W32 encoding (~12 bytes) but got something closer to W64 encoding (~17 bytes): {:02X?}",
        text_bytes.len(),
        text_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn field_write_u32_has_reloc() {
    // pa-r17-006-b AC3: .rela.text contains PC32 relocation against symbol 'r'
    // This verifies that the relocation record is correctly generated for the RIP-relative reference.
    let input = build_emit_data("field_write_u32.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_field_write_u32_reloc.o");
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
        "build failed for field_write_u32: {}",
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
