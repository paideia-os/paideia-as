//! Phase 6 m3-005: Byte-sequence assertion test for field-access expression inside unsafe blocks.
//!
//! This test verifies that field access expressions inside unsafe blocks emit correct x86-64 bytes.
//! The parse_deref_operand function in unsafe_walker.rs resolves field offsets via RecordLayoutTable.
//!
//! Test fixtures:
//! - cap_set_rights.pdx: (*p).rights field write inside unsafe block
//!   Expected: mov [rdi + 16], rsi → 48 89 77 10
//! - cap_read_kind.pdx: (*p).kind field read (aspirational, requires struct syntax)
//!
//! Unit tests in unsafe_walker.rs validate:
//! - parse_deref_field_access_with_offset_zero: field at offset 0
//! - parse_deref_field_access_with_offset_16: field at offset 16
//! - parse_deref_field_offset_unresolved_missing_type: U1608 diagnostic on missing type
//! - parse_deref_plain_dereference_zero_offset: plain *p without field access

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

// PAS-DEBT-B1-001 (blocked-on PAS-DEBT-B2-004, closed 2026-09-25): the
// cap_set_rights.pdx fixture now includes the `struct Capability` decl
// and parses cleanly; struct type-definition syntax is accepted at both
// file scope and inside `structure { ... }` (see
// crates/paideia-as-parser/tests/struct_type_def.rs). What remains is
// the field-access lowering that turns `(*p).rights = ...` inside an
// unsafe block into `48 89 77 10` — that walker/emit path is B1-001's
// territory. Keep this test `#[ignore]`d until B1-001 lands; un-ignoring
// belongs to that issue, not to B2-004.
#[test]
#[ignore]
fn field_access_cap_set_rights_deferred_pending_parser_support() {
    // This test would verify that cap_set_rights.pdx builds and emits correct bytes.
    // Ignored while B1-001 (walker field-offset emission) is outstanding.
    let input = build_emit_data("cap_set_rights.pdx");
    let _output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_field_access_cap_set_rights.o",
    ]);

    // When parser support arrives, uncomment and verify:
    // - Successful build
    // - Correct bytecode emission for field access operation
}
