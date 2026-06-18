//! End-to-end CLI tests for `paideia-as check`.
//!
//! These build the binary via `cargo run` and assert on exit code,
//! stderr/stdout, and the SARIF sidecar file. The tests run against
//! fixtures in `tests/data/`.

use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/data");
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
fn check_clean_example_exits_zero() {
    let input = data("example.pdx");
    let out = cargo_run(&["check", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // SARIF sidecar should have been written.
    let mut sarif = input.clone();
    sarif.set_file_name("example.pdx.sarif.json");
    assert!(sarif.exists(), "expected SARIF sidecar at {sarif:?}");

    // Clean up the sidecar so the test is idempotent.
    let _ = std::fs::remove_file(&sarif);
}

#[test]
fn check_lex_error_emits_e0006_and_exits_one() {
    let input = data("lex_error.pdx");
    let out = cargo_run(&["check", input.to_str().unwrap()]);

    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, got {:?}\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("E0006"),
        "expected E0006 in stderr; got:\n{stderr}"
    );

    let mut sarif = input.clone();
    sarif.set_file_name("lex_error.pdx.sarif.json");
    assert!(sarif.exists(), "expected SARIF sidecar at {sarif:?}");

    let _ = std::fs::remove_file(&sarif);
}

#[test]
fn check_dump_ir_prints_arena_header() {
    let input = data("example.pdx");
    let out = cargo_run(&["check", "--dump-ir", input.to_str().unwrap()]);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(ir-arena nodes="),
        "expected IR-arena header in stdout; got:\n{stdout}"
    );

    let mut sarif = input.clone();
    sarif.set_file_name("example.pdx.sarif.json");
    let _ = std::fs::remove_file(&sarif);
}
