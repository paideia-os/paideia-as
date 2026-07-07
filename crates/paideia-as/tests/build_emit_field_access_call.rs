//! Phase 6 m3-002 (#1073): Field access in call position AST→IR lowering
//!
//! This test verifies that `(record.field)()` expressions lower to App(FieldAccess(...), args)
//! IR with populated FieldAccessInfo side-table entries.
//!
//! Expected behavior:
//! - Parser produces ExprFieldAccess AST node for `record.field`
//! - Lowering emits IrKind::FieldAccess with exactly one child (receiver)
//! - populate_field_access_info builds binding→struct_type map and populates side-table
//! - Emit phase processes FieldAccess as indirect call via rip-relative address
//! - Bytecode contains FF 15 (indirect call via [rip+...])
//!
//! Test fixtures:
//! - vops_field_access_call.pdx: struct Vops with u64 field, call vops.read as function

use object::{Object, ObjectSection};
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
fn field_access_call_builds_successfully() {
    // Phase 6 m3-002 AC1: vops_field_access_call.pdx builds without error.
    // Verifies that:
    // - Parser accepts struct definition and field access syntax
    // - Lowering creates FieldAccess IR nodes
    // - populate_field_access_info builds binding map and populates side-table
    // - No compilation errors or T0552 diagnostics for known struct/field
    let input = build_emit_data("vops_field_access_call.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_field_access_call.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "field access call should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn field_access_call_emits_indirect_call_bytes() {
    // Phase 6 m3-002 AC2: (vops.read)() emits FF 15 bytes (indirect call via rip-relative).
    // This verifies the emit phase correctly lowers FieldAccess to indirect call.
    //
    // Expected bytes: FF 15 ... (call indirect via [rip+offset])
    // NOT: 4C 8B 1D ... FF 23 (the #1039 local-bound path with mov r11, ... + call r11)
    let input = build_emit_data("vops_field_access_call.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_field_access_call_emit.o");
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
        "build failed for vops_field_access_call: {}",
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

    // Assert: must contain FF 15 bytes (indirect call via [rip+...])
    assert!(
        text_bytes.windows(2).any(|w| w == [0xFF, 0x15]),
        "expected FF 15 (indirect call) in .text, got: {:02X?}",
        text_bytes
    );

    // Assert: must NOT contain the R11-stash path (mov r11, [rip+...])
    // The R11-stash sequence starts with 4C 8B 1D (mov r11, [rip+...])
    assert!(
        !text_bytes.windows(3).any(|w| w == [0x4C, 0x8B, 0x1D]),
        "unexpected mov r11, [rip+...] in .text — field access should use direct FF 15, not R11-stash path"
    );

    let _ = std::fs::remove_file(&tmp);
}
