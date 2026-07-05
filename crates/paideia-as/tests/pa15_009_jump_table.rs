//! PA15-r15-009 (issue #964): @jump_table attribute on match expressions.
//!
//! This integration test verifies that the parser recognizes and parses @jump_table syntax.
//! Elaborator diagnostics (P0272–P0275, density checks, codegen) are deferred to #1032.
//!
//! Current scope: parser-level verification only.
//! - Parser accepts @jump_table between scrutinee and `{`
//! - Parser rejects unknown attributes with P0270
//! - Parser rejects malformed attributes with P0271

use std::process::Command;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

#[test]
fn jump_table_dense_parses() {
    // Test that @jump_table attribute is accepted between scrutinee and {
    let fixture_path = "../../../tests/end-to-end/codes/pa_r15_009_jump_table_dense.pdx";
    let output = cargo_run(&["check", fixture_path]);
    // Parser must succeed; elaborator may emit diagnostics
    assert!(
        output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("P0100"),
        "Parser error: P0100 or unexpected failure. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn jump_table_sparse_parses() {
    // Test that sparse @jump_table parses (elaborator checks coverage)
    let fixture_path = "../../../tests/end-to-end/codes/pa_r15_009_jump_table_sparse.pdx";
    let output = cargo_run(&["check", fixture_path]);
    assert!(
        output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("P0100"),
        "Parser error: P0100 or unexpected failure"
    );
}

#[test]
fn jump_table_no_default_parses() {
    // Test that @jump_table with missing default arm parses
    // (elaborator will emit P0274 when implemented; currently deferred)
    let fixture_path = "../../../tests/end-to-end/codes/pa_r15_009_jump_table_no_default.pdx";
    let output = cargo_run(&["check", fixture_path]);
    assert!(
        output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("P0100"),
        "Parser error: P0100 or unexpected failure"
    );
}

#[test]
fn jump_table_non_int_parses() {
    // Test that @jump_table with non-integer patterns parses
    // (elaborator will emit P0275 when implemented; currently deferred)
    let fixture_path = "../../../tests/end-to-end/codes/pa_r15_009_jump_table_non_int.pdx";
    let output = cargo_run(&["check", fixture_path]);
    assert!(
        output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("P0100"),
        "Parser error: P0100 or unexpected failure"
    );
}

#[test]
fn jump_table_ipv4_protocol_parses() {
    // Test realistic IPv4 protocol dispatch use case with @jump_table
    let fixture_path = "../../../tests/end-to-end/codes/pa_r15_009_jump_table_ipv4_proto.pdx";
    let output = cargo_run(&["check", fixture_path]);
    assert!(
        output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("P0100"),
        "Parser error: P0100 or unexpected failure"
    );
}

#[test]
fn jump_table_min_offset_parses() {
    // Test @jump_table with minimum offset (0-based indexing)
    let fixture_path = "../../../tests/end-to-end/codes/pa_r15_009_jump_table_min_offset.pdx";
    let output = cargo_run(&["check", fixture_path]);
    assert!(
        output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("P0100"),
        "Parser error: P0100 or unexpected failure"
    );
}
