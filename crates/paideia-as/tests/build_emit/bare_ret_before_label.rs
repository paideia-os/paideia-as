//! Integration test for paideia-as#1320 — bare `ret` before a label.
//!
//! Regression test: a zero-operand instruction (`ret`) with no trailing `;`,
//! directly followed by a label declaration, must not have the label's
//! identifier consumed as a stray `ret` operand. Before the fix in
//! crates/paideia-as-parser/src/parse_stmt.rs, this cascaded into a P0100
//! parse error at the label's `:`.
//!
//! Fixture: tests/build-emit/bare_ret_before_label.pdx

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

/// Bare `ret` (no `;`) directly followed by a label declaration compiles
/// successfully, with no P0100 cascade.
#[test]
fn bare_ret_before_label_builds_successfully() {
    let input = build_emit_data("bare_ret_before_label.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_bare_ret_before_label.o");
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
        "build --emit elf64 failed for bare_ret_before_label.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("P0100"),
        "must not emit P0100 cascade: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        std::fs::metadata(&tmp).is_ok(),
        "ELF output file should exist"
    );

    let _ = std::fs::remove_file(&tmp);
}
