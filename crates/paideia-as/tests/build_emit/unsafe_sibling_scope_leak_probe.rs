//! #1139: Fix for scope leak in sibling unsafe lambdas.
//!
//! This test verifies that when two sibling lambdas both have Unsafe bodies
//! and share a parameter name (both have `v`), each lambda's unsafe block
//! correctly resolves `v` to its own parameter register, not to a sibling's.
//!
//! The fixture defines two functions with Unsafe bodies:
//! - f: fn (v: u64) -> unsafe { mov rax, v; ret }
//!   - v is at arg-0 (RDI per SysV ABI)
//!   - Should emit: mov rax, rdi (48 89 f8 in bytes)
//!
//! - g: fn (a: u64, v: u64) -> unsafe { mov rax, v; ret }
//!   - v is at arg-1 (RSI per SysV ABI, since a is at arg-0)
//!   - Should emit: mov rax, rsi (48 89 f0 in bytes)
//!
//! Bug (pre-fix): Both functions resolve `v` to RSI because the flat
//! local_bindings map (populated during the whole-arena walk) has only
//! one entry for `v`, pointing to whichever was last (g's RSI).
//!
//! Fix: per_lambda_bindings snapshot + instr_to_lambda mapping allow
//! resolve_var_operands to look up each instruction's lambda and use
//! that lambda's binding snapshot, so f's `v` → RDI and g's `v` → RSI.

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
fn unsafe_sibling_scope_leak_probe_builds_successfully() {
    // #1139 AC1: unsafe_sibling_scope_leak_probe.pdx builds without error.
    let input = build_emit_data("unsafe_sibling_scope_leak_probe.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_sibling_scope_leak_probe.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "unsafe_sibling_scope_leak_probe should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unsafe_sibling_scope_leak_probe_f_uses_rdi() {
    // #1139 AC2: f's unsafe block correctly resolves v → RDI (arg-0),
    // not to RSI (g's arg-1).
    //
    // f: fn (v: u64) -> unsafe { mov rax, v; ret }
    //   - v is at arg-0 (RDI per SysV ABI)
    //   - Should emit: 48 89 f8 (mov rax, rdi)
    //
    // Pre-fix: both f and g resolve v → RSI (g's second param),
    // so f emits 48 89 f0 (mov rax, rsi) — WRONG.
    //
    // Post-fix: f correctly resolves v → RDI via per_lambda_bindings,
    // emitting 48 89 f8 (mov rax, rdi) — CORRECT.

    let input = build_emit_data("unsafe_sibling_scope_leak_probe.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_unsafe_sibling_scope_leak_probe_emit.o");
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
        "build failed for unsafe_sibling_scope_leak_probe: {}",
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

    // Assert: f's .text at offset 0 (first function) starts with 48 89 f8 (mov rax, rdi) — CORRECT.
    let correct_f_pattern = [0x48, 0x89, 0xf8];
    assert!(
        text_bytes.len() >= 3 && &text_bytes[0..3] == correct_f_pattern,
        "expected f to start with 48 89 f8 (mov rax, rdi) at offset 0, got: {:02X?}",
        &text_bytes[0..text_bytes.len().min(8)]
    );

    let _ = std::fs::remove_file(&tmp);
}
