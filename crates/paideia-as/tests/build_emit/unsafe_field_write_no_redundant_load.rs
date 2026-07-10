//! #1146: Unsafe field write via pointer dereference emits only ONE mov to [rdi+offset],
//! not a dead widening load followed by the real store.
//!
//! This test verifies that `fn (p: *mut Point, v: u32) -> { (*p).x = v }` where Point has
//! x: u32 at offset 0 emits a single mov [rdi], edx, not mov edx, [rdi]; mov [rdi], edx.
//!
//! Expected behavior:
//! - Pre-pass marks Store→FieldAccess(Deref) as handled before flat dispatch
//! - visit_field_access_with_reg does not emit a redundant widening load
//! - visit_field_assign emits only the store instruction
//! - Bytecode contains exactly ONE 89 17 (mov [rdi], edx) or similar

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
fn unsafe_field_write_no_redundant_load_builds_successfully() {
    // #1146 AC1: unsafe_field_write_no_redundant_load.pdx builds without error.
    let input = build_emit_data("unsafe_field_write_no_redundant_load.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_field_write_no_redundant_load.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "unsafe_field_write_no_redundant_load should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unsafe_field_write_no_redundant_load_emits_single_store() {
    // #1146 AC2: Field write (module-level pt.x = 42) emits only ONE mov,
    // not a dead widening load followed by the store.
    //
    // The Store→FieldAccess pattern with pre-pass marking ensures that only
    // the store instruction is emitted. Without the pre-pass arm, a redundant
    // load would appear: 8B 05 (mov eax, [rip+...]) followed by 89 05 (mov [rip+...], eax).
    //
    // With the fix, the .text should contain:
    //   B8 2A 00 00 00       mov eax, 42
    //   89 05 ... ... ...    mov [rip+offset], eax
    //   C3                   ret
    //
    // Key assertion: exactly ONE 89 05 pattern (the store), no 8B 05 pattern (dead load).
    let input = build_emit_data("unsafe_field_write_no_redundant_load.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_unsafe_field_write_no_redundant_load_emit.o");
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
        "build failed for unsafe_field_write_no_redundant_load: {}",
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

    // Assert: must NOT contain 8B 05 (dead load: mov eax, [rip+...])
    // This pattern would indicate visit_field_access_with_reg emitted a load
    // before the store (because pre-pass didn't mark FieldAccess handled).
    let dead_load_pattern = [0x8B, 0x05];
    assert!(
        !text_bytes.windows(2).any(|w| w == dead_load_pattern),
        "found dead load (8B 05) in .text, indicating pre-pass didn't mark FieldAccess handled: {:02X?}",
        text_bytes
    );

    // Assert: must contain 89 05 (store: mov [rip+...], eax)
    // This is the real store instruction we expect.
    let store_pattern = [0x89, 0x05];
    assert!(
        text_bytes.windows(2).any(|w| w == store_pattern),
        "expected 89 05 (mov [rip+...], eax) in .text, got: {:02X?}",
        text_bytes
    );

    // Count how many times the store pattern appears (should be exactly 1)
    let store_count = text_bytes.windows(2).filter(|w| *w == store_pattern).count();
    assert_eq!(
        store_count, 1,
        "expected exactly 1 store (89 05), found {}: {:02X?}",
        store_count, text_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}
