//! Issue #1182: Cross-module field assignment via RIP-relative store
//!
//! This test verifies that `fn(v: u64) -> () { Runqueue._current_tcb = v }` where
//! Runqueue is an external module lowers to emit_mem_write_via_rip_sym, producing
//! mov [rip + _current_tcb], rsi bytecode (u64 write with REX.W prefix).
//!
//! Expected behavior:
//! - Parser accepts module-qualified field assignment (Runqueue._current_tcb = v)
//! - Elaborator populate_field_access_info skips struct-type lookup (module receiver)
//! - Elaborator populate_field_access_info records field name in module_field_refs
//! - visit_field_assign detects module_field_refs entry and routes to emit_module_field_write
//! - emit_module_field_write emits MovSized{W64} + MemRipRelSym operands
//! - Bytecode contains 48 89 3d (mov [rip+...], rsi) with REX.W + opcode + ModR/M
//! - .rela.text contains PC32 reloc against symbol '_current_tcb'

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
fn cross_module_field_write_u64_builds_successfully() {
    // Issue #1182 AC1: cross_module_field_write_u64.pdx builds without error.
    // Verifies that:
    // - Parser accepts module-qualified field assignment
    // - module_field_refs side-table is populated during elaboration
    // - No U1644 error even though FieldAccessInfo is not populated
    // - No T0540 error (Var RHS is in scope)
    let input = build_emit_data("cross_module_field_write_u64.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_cross_module_field_write_u64.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "cross_module_field_write_u64 should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cross_module_field_write_u64_emits_rip_rel_store() {
    // Issue #1182 AC2: fn(v: u64) -> () { Runqueue._current_tcb = v } emits mov [rip+...], rsi (u64)
    // Expected byte sequence: 48 89 3d (mov [rip+...], rsi) with REX.W prefix + opcode + ModR/M
    // The function receives v in rsi (second parameter per MS calling convention)
    // This verifies emit_module_field_write correctly emits RIP-relative store with u64 width.
    let input = build_emit_data("cross_module_field_write_u64.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_cross_module_field_write_u64_emit.o");
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
        "build failed for cross_module_field_write_u64: {}",
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

    // Assert: must contain 48 89 3d (mov [rip+...], rsi) with REX.W prefix
    // 0x48 = REX.W, 0x89 = opcode, 0x3d = ModR/M (rsi as register field)
    let store_seq = [0x48u8, 0x89, 0x3d];
    let store_pos = text_bytes.windows(3).position(|w| w == store_seq)
        .expect("expected 48 89 3d (mov [rip+...], rsi with REX.W) in .text");

    // Verify that REX.W is indeed there (0x48 must be the prefix)
    assert_eq!(
        text_bytes[store_pos], 0x48,
        "expected REX.W (0x48) prefix for u64 mov [rip+...], rsi at position {}",
        store_pos
    );

    // Assert: must contain C3 (ret) — ideally after the mov
    assert!(
        text_bytes.iter().any(|&b| b == 0xC3),
        "expected C3 (ret) in .text, got: {:02X?}",
        text_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn cross_module_field_write_u64_has_reloc_to_current_tcb() {
    // Issue #1182 AC3: .rela.text contains PC32 relocation against symbol '_current_tcb'
    // This verifies that the relocation record is correctly generated for the RIP-relative reference
    // to the external module symbol.
    let input = build_emit_data("cross_module_field_write_u64.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_cross_module_field_write_u64_reloc.o");
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
        "build failed for cross_module_field_write_u64: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find .text section and iterate its relocations
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");

    // Iterate relocations and check for at least one targeting symbol '_current_tcb'
    let mut found_current_tcb_reloc = false;
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "_current_tcb" {
                found_current_tcb_reloc = true;
                break;
            }
        }
    }

    assert!(
        found_current_tcb_reloc,
        ".rela.text should contain relocation against symbol '_current_tcb'"
    );

    let _ = std::fs::remove_file(&tmp);
}
