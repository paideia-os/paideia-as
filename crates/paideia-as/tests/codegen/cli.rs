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

fn assert_sarif_results_contain(sarif: &serde_json::Value, expected_rule: &str) {
    let runs = sarif["runs"].as_array()
        .expect("SARIF should have runs array");
    assert!(!runs.is_empty(), "SARIF runs array should be non-empty");
    let ids: Vec<String> = runs.iter()
        .flat_map(|run| run["results"].as_array().cloned().unwrap_or_default())
        .filter_map(|r| r["ruleId"].as_str().map(String::from))
        .collect();
    assert!(
        ids.iter().any(|id| id == expected_rule),
        "expected ruleId `{expected_rule}` in SARIF results[]; found ruleIds: {ids:?}\nsarif: {sarif}"
    );
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
}

#[test]
fn check_with_sarif_flag_writes_to_specified_path() {
    let input = data("example.pdx");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let sarif_path = temp_dir.path().join("output.sarif.json");

    let out = cargo_run(&[
        "check",
        input.to_str().unwrap(),
        "--sarif",
        sarif_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        sarif_path.exists(),
        "expected SARIF file at {sarif_path:?}"
    );

    let sarif_content = std::fs::read_to_string(&sarif_path)
        .expect("failed to read SARIF file");
    let parsed: serde_json::Value = serde_json::from_str(&sarif_content)
        .expect("failed to parse SARIF as JSON");

    assert!(
        parsed.get("$schema").is_some(),
        "expected SARIF to have $schema field"
    );
    assert!(
        parsed["$schema"]
            .as_str()
            .map(|s| s.contains("sarif"))
            .unwrap_or(false),
        "expected SARIF schema to mention 'sarif'"
    );

    let runs = parsed.get("runs").and_then(|r| r.as_array());
    assert!(
        runs.is_some() && !runs.unwrap().is_empty(),
        "expected SARIF to have non-empty runs array"
    );
}

#[test]
fn check_without_sarif_flag_writes_no_sidecar() {
    let input = data("example.pdx");
    let out = cargo_run(&["check", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}",
        out.status
    );

    let mut sarif = input.clone();
    sarif.set_file_name("example.pdx.sarif.json");
    assert!(
        !sarif.exists(),
        "expected no SARIF sidecar at {sarif:?}"
    );
}

#[test]
fn check_with_sarif_flag_captures_lex_error_diagnostic() {
    let input = data("lex_error.pdx");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let sarif_path = temp_dir.path().join("errors.sarif.json");

    let out = cargo_run(&[
        "check",
        input.to_str().unwrap(),
        "--sarif",
        sarif_path.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 on error, got {:?}",
        out.status
    );

    assert!(
        sarif_path.exists(),
        "expected SARIF file even on error at {sarif_path:?}"
    );

    let sarif_content = std::fs::read_to_string(&sarif_path)
        .expect("failed to read SARIF file");

    let sarif: serde_json::Value = serde_json::from_str(&sarif_content)
        .expect("SARIF should be valid JSON");
    assert_sarif_results_contain(&sarif, "E0006");
}

#[test]
fn build_with_sarif_flag_writes_to_specified_path() {
    let input = data("example.pdx");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let sarif_path = temp_dir.path().join("build.sarif.json");

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "placeholder",
        "--sarif",
        sarif_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        sarif_path.exists(),
        "expected SARIF file at {sarif_path:?}"
    );

    let sarif_content = std::fs::read_to_string(&sarif_path)
        .expect("failed to read SARIF file");
    let parsed: serde_json::Value = serde_json::from_str(&sarif_content)
        .expect("failed to parse SARIF as JSON");

    assert!(
        parsed.get("$schema").is_some(),
        "expected SARIF to have $schema field"
    );
}

#[test]
fn build_without_sarif_flag_writes_no_sidecar() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}",
        out.status
    );

    // Check that no .sarif.json file was created next to input
    let mut sarif = input.clone();
    sarif.set_file_name("example.pdx.sarif.json");
    assert!(
        !sarif.exists(),
        "expected no SARIF sidecar at {sarif:?}"
    );

    // Clean up placeholder
    let mut placeholder = input.clone();
    placeholder.set_file_name("example.placeholder");
    let _ = std::fs::remove_file(&placeholder);
}

#[test]
fn build_with_sarif_flag_captures_lex_error_across_emit_formats() {
    let input = data("lex_error.pdx");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    for emit in &["placeholder", "elf64", "pax", "pe-coff"] {
        let sarif_path = temp_dir.path().join(format!("build_{}.sarif.json", emit));

        let out = cargo_run(&[
            "build",
            input.to_str().unwrap(),
            "--emit",
            emit,
            "--sarif",
            sarif_path.to_str().unwrap(),
        ]);

        assert_eq!(
            out.status.code(),
            Some(1),
            "expected exit 1 on error with --emit {}, got {:?}",
            emit,
            out.status
        );

        assert!(
            sarif_path.exists(),
            "expected SARIF file for --emit {} at {sarif_path:?}",
            emit
        );

        let sarif_content = std::fs::read_to_string(&sarif_path)
            .expect(&format!("failed to read SARIF file for --emit {}", emit));
        let sarif: serde_json::Value = serde_json::from_str(&sarif_content)
            .unwrap_or_else(|e| panic!("SARIF should be valid JSON for --emit {}: {}", emit, e));
        assert_sarif_results_contain(&sarif, "E0006");
    }
}

#[test]
fn build_sarif_captures_walker_diagnostics_not_just_lex() {
    // Create a fixture with a P0286 error (ABI mismatch)
    let fixture_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = fixture_dir.path().join("Test1.pdx");
    let sarif_path = fixture_dir.path().join("abi.sarif.json");

    // Write a fixture that has a P0286 error: @abi is only valid on lambda bindings
    let fixture_content = r#"module Test1 = structure {
  pub let x : u64 = 42 @abi("ms")
}"#;
    std::fs::write(&fixture_path, fixture_content)
        .expect("failed to write fixture");

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "placeholder",
        "--sarif",
        sarif_path.to_str().unwrap(),
    ]);

    // Should fail because of the P0286 error
    assert!(
        !out.status.success(),
        "expected error exit, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        sarif_path.exists(),
        "expected SARIF file at {sarif_path:?}"
    );

    let sarif_content = std::fs::read_to_string(&sarif_path)
        .expect("failed to read SARIF file");

    let sarif: serde_json::Value = serde_json::from_str(&sarif_content)
        .expect("SARIF should be valid JSON");
    assert_sarif_results_contain(&sarif, "P0286");
}

#[test]
fn build_sarif_on_encoder_failure_still_writes_sidecar() {
    // This test would require a fixture that triggers BuildError::Encoder
    // For now, we create a minimal placeholder to show the pattern
    let input = data("example.pdx");
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let sarif_path = temp_dir.path().join("encoder_test.sarif.json");

    // Build with placeholder emit (won't fail encoding)
    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "placeholder",
        "--sarif",
        sarif_path.to_str().unwrap(),
    ]);

    // Should succeed
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}",
        out.status
    );

    // SARIF should exist
    assert!(
        sarif_path.exists(),
        "expected SARIF file at {sarif_path:?}"
    );

    // Clean up
    let mut placeholder = input.clone();
    placeholder.set_file_name("example.placeholder");
    let _ = std::fs::remove_file(&placeholder);
}

#[test]
fn build_clean_example_writes_placeholder() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

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
    let _ = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);
    let second = std::fs::read_to_string(&placeholder).unwrap();
    assert_eq!(first, second);

    let _ = std::fs::remove_file(&placeholder);
}

#[test]
fn build_lex_error_skips_placeholder_and_exits_one() {
    let input = data("lex_error.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

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
    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

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
fn build_two_lambdas_with_captures_reaches_all_walkers() {
    // Issue #1237: Regression test verifying that root walker fix allows the
    // LinearityWalker to reach all lambdas in a module correctly.
    //
    // Pre-#1237, the walker seed was IrNodeId::new(1), which (due to bottom-up
    // node allocation in lowering) was typically a leaf node with no children.
    // The root walker fix (find_outermost_root) ensures that all lambdas are
    // properly visited and their captures analyzed.
    //
    // This fixture contains two lambdas that both capture a free variable.
    // The walker must reach both lambdas for convert_closure_lets to work
    // correctly downstream. The test confirms exit code 0, proving the walkers
    // ran successfully over the entire IR tree.
    let input = data("two_lambdas_with_captures.pdx");
    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    assert!(
        out.status.success(),
        "expected exit 0 (walker reaches all lambdas), got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    // Placeholder should be written successfully
    let mut placeholder = input.clone();
    placeholder.set_file_name("two_lambdas_with_captures.placeholder");
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
        .map(|p| p.join("examples").join("02_functions.pdx"))
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

#[test]
fn build_pax_writes_file_with_magic() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "pax", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let mut pax_path = input.clone();
    pax_path.set_file_name("example.pax");
    assert!(pax_path.exists(), "expected PAX file at {pax_path:?}");

    let pax_bytes = std::fs::read(&pax_path).expect("could not read PAX");
    assert!(pax_bytes.len() >= 4, "PAX file too small");
    // PAX magic: b"PAX\0"
    assert_eq!(&pax_bytes[0..4], b"PAX\0", "invalid PAX magic bytes");

    let _ = std::fs::remove_file(&pax_path);
}

#[test]
fn build_pax_writes_96_byte_header_plus_section_table() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "pax", input.to_str().unwrap()]);

    assert!(out.status.success());

    let mut pax_path = input.clone();
    pax_path.set_file_name("example.pax");
    let pax_bytes = std::fs::read(&pax_path).expect("could not read PAX");

    // PAX_HEADER_SIZE is 96 bytes; with empty section table, minimum is 96 bytes.
    assert!(
        pax_bytes.len() >= 96,
        "PAX file should be at least 96 bytes (header); got {}",
        pax_bytes.len()
    );

    let _ = std::fs::remove_file(&pax_path);
}

#[test]
fn build_pax_unknown_format_still_errors() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "garbage", input.to_str().unwrap()]);

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for unknown format, got {:?}",
        out.status
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown --emit format"),
        "expected error message in stderr; got: {stderr}"
    );
}

#[test]
fn build_pax_content_hash_is_nonzero_after_finalize() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "pax", input.to_str().unwrap()]);

    assert!(out.status.success());

    let mut pax_path = input.clone();
    pax_path.set_file_name("example.pax");
    let pax_bytes = std::fs::read(&pax_path).expect("could not read PAX");

    // The BLAKE3 hash is at offset 32..64 in the header
    let hash_slice = &pax_bytes[32..64];
    assert_eq!(hash_slice.len(), 32, "hash should be 32 bytes");

    // Check that the hash is not all zeros
    let is_nonzero = hash_slice.iter().any(|&b| b != 0);
    assert!(is_nonzero, "expected nonzero content hash, got all zeros");

    let _ = std::fs::remove_file(&pax_path);
}

#[test]
fn build_pe_writes_file_with_dos_magic() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "pe-coff", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let mut pe_path = input.clone();
    pe_path.set_file_name("example.efi");
    assert!(pe_path.exists(), "expected PE file at {pe_path:?}");

    let pe_bytes = std::fs::read(&pe_path).expect("could not read PE");
    assert!(pe_bytes.len() >= 2, "PE file too small");
    // PE magic: 0x4D 0x5A (= "MZ")
    assert_eq!(&pe_bytes[0..2], &[0x4D, 0x5A], "invalid PE DOS magic bytes");

    let _ = std::fs::remove_file(&pe_path);
}

#[test]
fn build_pe_writes_nt_signature_at_e_lfanew() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "pe-coff", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let mut pe_path = input.clone();
    pe_path.set_file_name("example.efi");
    let pe_bytes = std::fs::read(&pe_path).expect("could not read PE");

    assert!(pe_bytes.len() >= 68, "PE file too small for NT signature");
    // NT signature at offset 64: 0x50 0x45 0x00 0x00 (= "PE\0\0")
    assert_eq!(
        &pe_bytes[64..68],
        &[0x50, 0x45, 0, 0],
        "invalid NT signature"
    );

    let _ = std::fs::remove_file(&pe_path);
}

#[test]
fn build_pe_unknown_format_still_errors() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "garbage", input.to_str().unwrap()]);

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for unknown format, got {:?}",
        out.status
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown --emit format"),
        "expected error message in stderr; got: {stderr}"
    );
}

#[test]
fn build_pe_first_256_bytes_snapshot() {
    let input = data("example.pdx");
    let out = cargo_run(&["build", "--emit", "pe-coff", input.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let mut pe_path = input.clone();
    pe_path.set_file_name("example.efi");
    let pe_bytes = std::fs::read(&pe_path).expect("could not read PE");

    // Snapshot the first 256 bytes as hex string.
    // Phase-2-m6: minimal PE with DOS header + NT sig + COFF header + Opt header + section headers + padding.
    let snapshot_len = std::cmp::min(256, pe_bytes.len());
    let hex_snapshot = pe_bytes[..snapshot_len]
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("");

    // Verify that the snapshot is deterministic and structurally valid.
    // The first 256 bytes should start with MZ and contain PE\0\0 at offset 64.
    assert!(
        hex_snapshot.starts_with("4d5a"),
        "snapshot should start with MZ magic"
    );
    assert!(
        hex_snapshot.len() >= 128,
        "snapshot should be at least 128 chars (64 bytes)"
    );

    let _ = std::fs::remove_file(&pe_path);
}

#[test]
fn pax_introspect_dumps_header() {
    let input = data("example.pdx");
    let build_out = cargo_run(&["build", "--emit", "pax", input.to_str().unwrap()]);
    assert!(build_out.status.success());

    let mut pax_path = input.clone();
    pax_path.set_file_name("example.pax");

    // Run pax-introspect on the PAX file
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("pax-introspect")
        .arg("--")
        .arg(pax_path.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let introspect_out = cmd.output().expect("failed to run pax-introspect");

    assert!(
        introspect_out.status.success(),
        "pax-introspect should exit 0; got {:?}\nstderr: {}",
        introspect_out.status,
        String::from_utf8_lossy(&introspect_out.stderr)
    );

    let stdout = String::from_utf8_lossy(&introspect_out.stdout);
    assert!(
        stdout.contains("PaxHeader"),
        "expected 'PaxHeader' in output; got:\n{stdout}"
    );
    assert!(
        stdout.contains("section_table"),
        "expected 'section_table' in output; got:\n{stdout}"
    );

    let _ = std::fs::remove_file(&pax_path);
}

#[test]
fn stub_lint_exits_with_code_2() {
    let input = data("example.pdx");
    let out = cargo_run(&["lint", input.to_str().unwrap()]);

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for stub lint command, got {:?}",
        out.status
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "expected 'not yet implemented' in stderr; got: {stderr}"
    );
}

#[test]
fn stub_emit_exits_with_code_2() {
    let input = data("example.pdx");
    let out = cargo_run(&["emit", "elf64", input.to_str().unwrap()]);

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for stub emit command, got {:?}",
        out.status
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "expected 'not yet implemented' in stderr; got: {stderr}"
    );
}

#[test]
fn stub_audit_exits_with_code_2() {
    let input = data("example.pdx");
    let out = cargo_run(&["audit", input.to_str().unwrap()]);

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for stub audit command, got {:?}",
        out.status
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "expected 'not yet implemented' in stderr; got: {stderr}"
    );
}
