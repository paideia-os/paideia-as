//! #1139: Order-dependence regression test for scope leak fix.
//!
//! This is a companion to unsafe_sibling_scope_leak_probe.rs with
//! the function definitions reversed (g first, f last).
//!
//! Verifies that the fix doesn't regress when functions are processed
//! in a different order. If the per_lambda_bindings snapshot logic
//! depended on order, this test would catch it.

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
fn unsafe_sibling_scope_leak_probe_reversed_builds_successfully() {
    let input = build_emit_data("unsafe_sibling_scope_leak_probe_reversed.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_sibling_scope_leak_probe_reversed.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "unsafe_sibling_scope_leak_probe_reversed should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unsafe_sibling_scope_leak_probe_reversed_f_still_uses_rdi() {
    // Reversed-order test: g first (with v → RSI), then f (with v → RDI).
    // Verifies f still correctly resolves v → RDI even when processed after g.
    //
    // Pre-fix: the order would matter; f might pick up g's stale binding.
    // Post-fix: per_lambda_bindings ensures each lambda's snapshot is
    // independent of processing order.

    let input = build_emit_data("unsafe_sibling_scope_leak_probe_reversed.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_unsafe_sibling_scope_leak_probe_reversed_emit.o");
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
        "build failed for unsafe_sibling_scope_leak_probe_reversed: {}",
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

    // For reversed order: g is first (at offset 0), f is second (at offset 4 after g's 4 bytes).
    // g: 48 89 f0 c3 (mov rax, rsi; ret) — offset 0-3
    // f: 48 89 f8 c3 (mov rax, rdi; ret) — offset 4-7
    // We want to verify f's code, which starts at offset 4.
    let correct_f_pattern = [0x48, 0x89, 0xf8];
    assert!(
        text_bytes.len() >= 7 && &text_bytes[4..7] == correct_f_pattern,
        "expected f to start with 48 89 f8 (mov rax, rdi) at offset 4 (reversed order), got: {:02X?}",
        &text_bytes[4..text_bytes.len().min(12)]
    );

    let _ = std::fs::remove_file(&tmp);
}
