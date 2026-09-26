//! Phase 6 m3-005: Byte-sequence assertion test for field-access expression inside unsafe blocks.
//!
//! Verifies cap_set_rights.pdx builds and emits the expected
//! `48 89 77 10` (mov [rdi + 16], rsi) sequence for its unsafe-block
//! field-store shape.
//!
//! Fixture: `tests/build-emit/cap_set_rights.pdx`
//!   `struct Capability { kind, target, rights, generation: u64 }` +
//!   `fn_set_rights` whose unsafe block writes `mov [rdi + 16], rsi`.
//!   Expected .text: `48 89 77 10`.
//!
//! History:
//! - Blocked on PAS-DEBT-B2-004 (parser struct type-def) until
//!   v0.36.43, when B2-004 landed struct decl parsing at both file
//!   scope and inside `structure { ... }` (see
//!   crates/paideia-as-parser/tests/struct_type_def.rs). The fixture
//!   here uses direct assembly (`mov [rdi + 16], rsi`), not the
//!   semantic `(*p).rights = v` shape, so no field-access walker
//!   work was required beyond what already exists.
//! - PAS-DEBT-B1-001 (v0.36.44): un-ignored and turned into a real
//!   byte-sequence assertion instead of a vacuous cargo_run().

use object::{Object, ObjectSection, ObjectSymbol};
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
fn field_access_cap_set_rights_emits_mov_qword_rdi_plus_16_rsi() {
    // PAS-DEBT-B1-001: cap_set_rights.pdx builds and emits the
    // 4-byte SIB-less displacement store `48 89 77 10`
    // (mov [rdi + 16], rsi) as the sole instruction of `fn_set_rights`.
    let input = build_emit_data("cap_set_rights.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_cap_set_rights_emit.o");
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
        "build --emit elf64 failed for cap_set_rights.pdx: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // .text must contain exactly the 4-byte store instruction.
    let text_bytes = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .and_then(|s| s.data().ok().map(<[u8]>::to_vec))
        .expect(".text section must exist");

    let expected: [u8; 4] = [0x48, 0x89, 0x77, 0x10];
    assert_eq!(
        text_bytes, expected,
        ".text mismatch: expected {:02X?} (mov [rdi+16], rsi), got {:02X?}",
        expected, text_bytes
    );

    // `fn_set_rights` symbol must be present (STT_FUNC).
    let found = file
        .symbols()
        .any(|s| s.name().unwrap_or("") == "fn_set_rights");
    assert!(
        found,
        "expected symbol `fn_set_rights` in ELF symbol table"
    );

    let _ = std::fs::remove_file(&tmp);
}
