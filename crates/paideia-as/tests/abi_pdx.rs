// =============================================================================
// crates/paideia-as/tests/abi_pdx.rs
// =============================================================================
// Integration test: verify that `src/toolchain/abi/abi.pdx` parses cleanly
// through `paideia-as check` with zero diagnostics.
//
// Purpose:
//   The canonical ABI definition must remain syntactically valid and
//   semantically correct across all changes to paideia-as. This test
//   ensures the definition is never broken by parser or elaborator changes.
//
// Design reference:
//   - design/toolchain/abi.md (ABI specification)
//   - design/02-development-environment.md §8.2 (cross-build smoke test)
//   - design/.plans/phase-2/os-requirements.md §2.1 (T1 requirement)
// =============================================================================

#[test]
fn abi_pdx_parses_cleanly() {
    // Resolve the path to the ABI fixture.
    // The fixture lives at: <workspace>/src/toolchain/abi/abi.pdx
    // From the test crate perspective:
    // CARGO_MANIFEST_DIR = <workspace>/crates/paideia-as
    // So the fixture is at: <workspace>/src/toolchain/abi/abi.pdx
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("parent of crate dir")
        .parent()
        .expect("parent of crates dir");
    let abi_pdx_path = workspace_root.join("src/toolchain/abi/abi.pdx");

    assert!(
        abi_pdx_path.exists(),
        "abi.pdx fixture missing at {}",
        abi_pdx_path.display()
    );

    // Invoke `cargo run -p paideia-as -- check <path>`.
    // The check subcommand type-checks without emitting object files.
    // Zero exit status + empty stderr indicates successful parse and elaboration.
    let mut cmd = std::process::Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-as")
        .arg("--")
        .arg("check")
        .arg(&abi_pdx_path);

    // Disable colored output for cleaner test logs.
    cmd.env("NO_COLOR", "1");

    // Run the command and capture output.
    let output = cmd.output().expect("failed to run paideia-as check");

    // Assert successful exit.
    assert!(
        output.status.success(),
        "abi.pdx must parse cleanly. exit status: {}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // Assert no diagnostics on stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty() || stderr.trim().is_empty(),
        "abi.pdx check produced warnings/errors:\n{}",
        stderr
    );
}
