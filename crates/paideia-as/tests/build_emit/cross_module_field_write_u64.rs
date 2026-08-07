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
    // Expected byte sequence: 48 89 3d 00 00 00 00 c3 (exact 8 bytes: mov [rip+disp32], rsi; ret)
    // The function receives v in rsi (second parameter per MS calling convention)
    // This verifies emit_module_field_write correctly emits RIP-relative store with u64 width,
    // with no orphan loads before or after the function symbol (regression guard on 05f2017).
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
    let text = crate::common::elf::text_bytes(&bytes);

    assert!(!text.is_empty(), ".text section should contain bytes");

    // Extract function f's bytes via symbol lookup
    let f_bytes = crate::common::elf::symbol_bytes(&bytes, "f")
        .expect("symbol 'f' should exist in .text");

    // paideia-as#1276 phase 3: 4-byte prologue (55 48 89 E5) + 7-byte store +
    // 4-byte epilogue (48 89 EC 5D) + 1-byte ret = 16 bytes.
    let expected_f = vec![
        0x55u8, 0x48, 0x89, 0xE5,               // push rbp; mov rbp, rsp
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00, // mov [rip+disp32], rsi
        0x48, 0x89, 0xEC, 0x5D,                 // mov rsp, rbp; pop rbp
        0xc3,                                    // ret
    ];
    assert_eq!(
        f_bytes, expected_f,
        "f should emit exactly [prologue; mov [rip+disp32], rsi; epilogue; ret], got: {:02X?}",
        f_bytes
    );

    // Assert .text as a whole does NOT contain orphan RIP-relative reads (48 8b 05)
    // that would indicate the regression at 05f2017
    let orphan_read_seq = [0x48u8, 0x8b, 0x05];
    assert!(
        !text.windows(3).any(|w| w == orphan_read_seq),
        ".text should NOT contain orphan mov rax, [rip+...] reads (regression guard for 05f2017), got: {:02X?}",
        text
    );

    // Assert .text is exactly 16 bytes (function f with prologue+epilogue, no
    // orphan instructions).
    assert_eq!(
        text.len(), 16,
        ".text should be exactly 16 bytes (one function f with prologue+epilogue, no orphan bytes), got {} bytes: {:02X?}",
        text.len(), text
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn cross_module_field_write_u64_has_reloc_to_current_tcb() {
    // Issue #1182 AC3: .rela.text contains exactly one PC32 relocation against symbol '_current_tcb'
    // This verifies that the relocation record is correctly generated for the RIP-relative reference
    // to the external module symbol. The count must be exactly 1 — if 05f2017's orphan load
    // regressed (marked by two relocations), this test catches it.
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

    // Find .text section and count relocations targeting '_current_tcb'
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");

    let mut current_tcb_reloc_count = 0;
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "_current_tcb" {
                current_tcb_reloc_count += 1;
            }
        }
    }

    assert_eq!(
        current_tcb_reloc_count, 1,
        ".rela.text should contain exactly 1 PC32 relocation against '_current_tcb' (regression check: 2 would indicate orphan load from 05f2017), got {}",
        current_tcb_reloc_count
    );

    let _ = std::fs::remove_file(&tmp);
}
