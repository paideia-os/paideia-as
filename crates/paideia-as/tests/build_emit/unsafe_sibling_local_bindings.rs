//! #1139: Sibling lambdas sharing a param name must each resolve to their own
//! register, not consume stale local_bindings from the last-visited lambda.
//!
//! This test verifies that emit_pending_unsafe_bodies snapshots/restores
//! local_bindings per unsafe body, ensuring that stmt-position unsafe blocks
//! see their enclosing lambda's params, not whatever state a previous
//! action-bodied lambda left behind.
//!
//! The fixture has two functions:
//! - write_via_ptr: p is arg-0 (RDI), dereferenced in unsafe block -> (*p).x = v
//! - sum_two: uses different param names (a, b), not sharing p
//!
//! Pre-fix: emit_pending_unsafe_bodies ran with stale local_bindings from sum_two
//! (if it were an action body), causing write_via_ptr's unsafe block to misresolve
//! its param registers.
//!
//! Post-fix: snapshot/restore ensures write_via_ptr sees p → RDI when emitting (*p).x = v,
//! generating the correct `mov [rdi], esi` instruction.

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
fn unsafe_sibling_local_bindings_builds_successfully() {
    // #1139 AC1: unsafe_sibling_local_bindings.pdx builds without error.
    let input = build_emit_data("unsafe_sibling_local_bindings.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_sibling_local_bindings.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "unsafe_sibling_local_bindings should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unsafe_sibling_local_bindings_uses_correct_register() {
    // #1139 AC2: write_via_ptr's unsafe block correctly resolves p → RDI,
    // not a stale binding from a sibling lambda's params.
    //
    // The fixture defines two functions:
    // 1. write_via_ptr: fn (p: *Point, v: u32) -> unsafe { (*p).x = v; }
    //    - p is at arg-0 (RDI per SysV ABI)
    //    - v is at arg-1 (RSI per SysV ABI)
    //    - Should emit: mov [rdi], esi (89 37 in bytes)
    //
    // 2. sum_two: fn (a: u64, b: u64) -> u64
    //    - Simple arithmetic, uses different params
    //
    // Pre-fix: if emit_pending_unsafe_bodies used stale local_bindings,
    // write_via_ptr might resolve p → RSI and emit [rsi] instead of [rdi].
    //
    // Post-fix: snapshot/restore ensures write_via_ptr sees p → RDI,
    // generating the correct [rdi] addressing.
    let input = build_emit_data("unsafe_sibling_local_bindings.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_unsafe_sibling_local_bindings_emit.o");
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
        "build failed for unsafe_sibling_local_bindings: {}",
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

    // Assert: must NOT contain 89 36 (mov [rsi], esi) — the wrong register.
    // This pattern would indicate write_via_ptr incorrectly resolved p → RSI.
    let wrong_register_pattern = [0x89, 0x36];
    assert!(
        !text_bytes.windows(2).any(|w| w == wrong_register_pattern),
        "found mov [rsi], esi (89 36) in .text, indicating stale local_bindings \
         from a sibling lambda: {:02X?}",
        text_bytes
    );

    // Assert: must contain 89 37 (mov [rdi], esi) — the correct register.
    // This pattern indicates write_via_ptr correctly resolved p → RDI.
    let correct_pattern = [0x89, 0x37];
    assert!(
        text_bytes.windows(2).any(|w| w == correct_pattern),
        "expected 89 37 (mov [rdi], esi) in .text for write_via_ptr, got: {:02X?}",
        text_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}
