//! Issue #1413: `paideia-as build` must exit nonzero when any diagnostic with
//! Severity::Error was emitted, even if a downstream emit stage did not
//! propagate the failure as an `Err`.
//!
//! Repro: the fixture omits the `capabilities:` field from an `unsafe { }`
//! block. The parser reports `error[P0154]: expected \`@{\` or \`{\` for
//! capabilities field`. Before the fix, `paideia-as build --emit elf64` on
//! this input exited 0, silently fooling `tools/build.sh`'s grep gate and
//! letting parse-error .pdx files land in downstream repos (paideia-os/line#3).
//!
//! After the fix, the cmd_build seam explicitly checks the diagnostic sink
//! and returns `ExitCode::from(1)` when any Severity::Error was recorded —
//! regardless of the emit format or which internal stage produced it.
//! (Exit code 1 matches the existing per-emit-path convention: 1 = diagnostic
//! error, 2 = I/O or CLI-argument error.)

use std::process::Command;

fn build_emit_data(name: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

/// P0154 (missing capabilities field) must force a nonzero exit code even on
/// the elf64 emit path — which previously masked the error as exit 0.
#[test]
fn p0154_missing_capabilities_field_exits_nonzero_elf64() {
    let input = build_emit_data("parse_error_p0154_exit_nonzero.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_p0154_exit_nonzero.o",
    ]);

    let code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Exit must be nonzero. The seam-level guard in cmd_build returns 1
    // (matching the existing finish_* convention: 1 = diagnostic error,
    // 2 = I/O or CLI-argument error).
    assert_ne!(
        code,
        Some(0),
        "P0154 parse-error input must NOT exit 0 (regression #1413). \
         stderr: {}",
        stderr
    );
    assert_eq!(
        code,
        Some(1),
        "P0154 parse-error input should exit 1 (diagnostic error). \
         stderr: {}",
        stderr
    );

    // The diagnostic should still have been rendered to stderr so users see
    // the underlying failure, not just a bare nonzero exit.
    assert!(
        stderr.contains("P0154"),
        "stderr should render the P0154 diagnostic. stderr: {}",
        stderr
    );
}

/// The same guard must fire for the placeholder emit path too — this is the
/// original default `--emit placeholder` behaviour that some tests still use.
#[test]
fn p0154_missing_capabilities_field_exits_nonzero_placeholder() {
    let input = build_emit_data("parse_error_p0154_exit_nonzero.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "placeholder",
        "-o",
        "/tmp/test_p0154_exit_nonzero.placeholder",
    ]);

    let code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_ne!(
        code,
        Some(0),
        "P0154 parse-error input must NOT exit 0 on placeholder emit \
         (regression #1413). stderr: {}",
        stderr
    );
}
