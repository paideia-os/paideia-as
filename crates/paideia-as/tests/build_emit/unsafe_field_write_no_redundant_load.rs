//! #1146: Unsafe field write via pointer dereference emits only ONE mov to
//! [rdi+offset], not a dead widening load followed by the real store.
//!
//! This test verifies that `fn (p: *Point, v: u32) -> unsafe { block: { (*p).x = v; } } }`
//! where Point has x: u32 at offset 0 emits a single `mov [rdi], esi`, not
//! `mov eax, [rdi]; mov [rdi], esi`.
//!
//! #1146 follow-up (adversarial re-verification of be48580): the original
//! version of this test built a fixture using the module-level, non-Deref
//! shape (`pt.x = 42`, lowering via a RIP-relative store) and asserted on
//! `8B 05` / `89 05` byte patterns. That shape never exercises the Deref
//! receiver the #1146 pre-pass arm targets, so it could not have caught a
//! regression in the actual fix. This version builds the real bug shape —
//! `(*p).field = value` with `p` a dereferenced function parameter — and
//! asserts on the `[rdi+disp]` base-register ModR/M encoding the bug
//! report describes.
//!
//! Expected behavior:
//! - Pre-pass marks Store→FieldAccess(Deref) as handled before flat dispatch
//! - visit_field_access_with_reg does not emit a redundant widening load
//! - visit_field_assign emits only the store instruction, using the
//!   pointer's and value's actual assigned registers (not hardcoded RDI/RDX)
//! - Bytecode contains exactly one `mov [rdi], esi` (encoded `89 37`) and no
//!   preceding `mov eax, [rdi]`-shaped widening load

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
    // #1146 AC2: Field write via pointer deref, `(*p).x = v`, emits only ONE
    // mov, not a dead widening load followed by the store.
    //
    // The Store->FieldAccess(Deref) pattern with pre-pass marking ensures
    // that only the store instruction is emitted. Without the pre-pass arm,
    // a redundant load would appear before the store: `8B 07` (mov eax,
    // [rdi]) followed by `89 37` (mov [rdi], esi).
    //
    // With the fix, the .text should contain exactly:
    //   89 37                mov [rdi], esi
    //
    // (base=RDI holds `p`, the function's first parameter; source=ESI holds
    // `v`, the second parameter, per the SysV ABI — resolved from
    // local_bindings, not hardcoded.)
    //
    // Key assertion: exactly ONE `89 37` pattern (the store), no `8B 07`
    // pattern (dead load) anywhere in .text.
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

    // Assert: must NOT contain 8B 07 (dead load: mov eax, [rdi]) anywhere.
    // This pattern would indicate visit_field_access_with_reg emitted a
    // widening load before the store (i.e. the pre-pass didn't mark the
    // FieldAccess node handled in time).
    let dead_load_pattern = [0x8B, 0x07];
    assert!(
        !text_bytes.windows(2).any(|w| w == dead_load_pattern),
        "found dead load (8B 07, mov eax,[rdi]) in .text, indicating the \
         Store->FieldAccess(Deref) pre-pass didn't mark FieldAccess handled: {:02X?}",
        text_bytes
    );

    // Assert: must contain 89 37 (store: mov [rdi], esi) — the real store,
    // using the resolved base (RDI = p) and value (ESI = v) registers.
    let store_pattern = [0x89, 0x37];
    assert!(
        text_bytes.windows(2).any(|w| w == store_pattern),
        "expected 89 37 (mov [rdi], esi) in .text, got: {:02X?}",
        text_bytes
    );

    // Count how many times the store pattern appears (should be exactly 1).
    let store_count = text_bytes.windows(2).filter(|w| *w == store_pattern).count();
    assert_eq!(
        store_count, 1,
        "expected exactly 1 store (89 37), found {}: {:02X?}",
        store_count, text_bytes
    );

    // .text should be exactly 2 bytes: the single store instruction, no
    // preceding load and no trailing padding.
    assert_eq!(
        text_bytes.len(),
        2,
        "expected .text to contain exactly the 2-byte store instruction, got {:02X?}",
        text_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}
