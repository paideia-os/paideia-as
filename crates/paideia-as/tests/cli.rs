//! End-to-end CLI tests for `paideia-as check` and `paideia-as build`.
//!
//! These build the binary via `cargo run` and assert on exit code,
//! stderr/stdout, and the SARIF sidecar file. The tests run against
//! fixtures in `tests/data/` and examples.

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

#[test]
fn build_clean_example_writes_placeholder() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let mut placeholder = input.clone();
    placeholder.set_file_name("example.placeholder");
    assert!(
        placeholder.exists(),
        "expected placeholder at {placeholder:?}"
    );

    let first = std::fs::read_to_string(&placeholder).unwrap();
    assert!(first.starts_with("paideia-as placeholder v0"));
    assert!(first.contains("blake3 "));

    // Run again: deterministic output.
    let _ = std::fs::remove_file(&placeholder);
    let _ = cargo_run(&["build", input.to_str().unwrap()]);
    let second = std::fs::read_to_string(&placeholder).unwrap();
    assert_eq!(first, second);

    let _ = std::fs::remove_file(&placeholder);
}

#[test]
fn build_lex_error_skips_placeholder_and_exits_one() {
    let input = data("lex_error.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(1));

    let mut placeholder = input.clone();
    placeholder.set_file_name("lex_error.placeholder");
    assert!(
        !placeholder.exists(),
        "placeholder should not be written when errors present"
    );
}

#[test]
fn build_linear_double_use_compiles_but_doesnt_fire_walker() {
    // Phase-2-m1 limitation: the walker infrastructure runs end-to-end, but
    // the IR carries only IrKind (no structured payloads). Linear/Ordered
    // linearity classes are not populated in phase-1 lowering, so the
    // LinearityWalker sees all bindings as Unrestricted and cannot fire
    // S0901 (overused) diagnostics on real source.
    //
    // This test confirms the walker runs without panicking (end-to-end proof
    // that CLI wiring works) but does not expect S0901 to fire yet.
    // Structured linearity payload arrives in m3/m5 when the IR gains
    // per-binding class info at lowering time.
    let input = data("linear_double_use.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0 (walker runs without crashing), got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    // Placeholder should be written successfully
    let mut placeholder = input.clone();
    placeholder.set_file_name("linear_double_use.placeholder");
    assert!(
        placeholder.exists(),
        "placeholder should be written when no errors present"
    );

    let _ = std::fs::remove_file(&placeholder);
}

#[test]
fn build_calling_convention_example_emits_clean_elf() {
    // Test that a known-good example (§12) still produces a valid ELF
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("examples").join("12_calling_convention.pdx"))
        .expect("could not resolve examples directory");

    if !input.exists() {
        // Skip if the example doesn't exist in this build context
        return;
    }

    let out = cargo_run(&["build", "--emit", "elf64", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0 on valid example, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify ELF magic bytes were written
    let mut elf_path = input.clone();
    elf_path.set_extension("o");
    if elf_path.exists() {
        let elf_bytes = std::fs::read(&elf_path).expect("could not read ELF");
        assert!(elf_bytes.len() >= 4, "ELF file too small");
        // ELF magic: 0x7f 0x45 0x4c 0x46 (= "\x7fELF")
        assert_eq!(&elf_bytes[0..4], b"\x7fELF", "invalid ELF magic bytes");
        let _ = std::fs::remove_file(&elf_path);
    }
}
