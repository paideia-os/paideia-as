//! Phase 8 m1-004: Integration tests for typed encoder/emitter diagnostics (SARIF output).
//!
//! Verifies that:
//! - B1705 (encoder-error) fires on encoding failures
//! - B1706 (encoder-warn) fires on warnings with --encoder-warn
//! - U1610 (unresolved-label) fires on label fixup failures
//! - B1703 (symbol-layout-invalid) fires on symbol validation failures
//! - B1704 (function-symbol-no-offset) fires on missing function offsets
//!
//! These tests use SARIF output to verify diagnostic codes and locations.

use std::process::Command;
use std::fs;
use std::path::PathBuf;

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

/// Extract results array from SARIF JSON.
/// Returns a serde_json::Value representing the results array, or None if not found.
fn extract_sarif_results(sarif_path: &str) -> Option<Vec<serde_json::Value>> {
    let content = fs::read_to_string(sarif_path).ok()?;
    let sarif_json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Navigate to runs[0].results[]
    sarif_json
        .pointer("/runs/0/results")
        .and_then(|v| v.as_array())
        .map(|arr| arr.clone())
}

#[test]
fn encoder_failure_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1705 fires on encoder failure with --sarif.
    let input = build_emit_data("encoder_strict_unsupported.pdx");
    let sarif_path = "/tmp/test_encoder_failure_diagnostic.sarif.json";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_failure.o",
        "--sarif",
        sarif_path,
    ]);

    // Exit code 2 (error)
    assert_eq!(
        output.status.code(),
        Some(2),
        "encoder failure should exit 2. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // SARIF file exists and contains B1705
    let results = extract_sarif_results(sarif_path)
        .expect("SARIF file should exist and be valid JSON");

    assert!(!results.is_empty(), "SARIF results should not be empty");

    let has_b1705 = results.iter().any(|result| {
        result
            .pointer("/ruleId")
            .and_then(|v| v.as_str())
            .map(|rid| rid.contains("B1705"))
            .unwrap_or(false)
    });

    assert!(has_b1705, "SARIF results should contain B1705 ruleId");
}

#[test]
fn encoder_warn_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1706 fires on encoder warning with --encoder-warn --sarif.
    let input = build_emit_data("encoder_strict_unsupported.pdx");
    let sarif_path = "/tmp/test_encoder_warn_diagnostic.sarif.json";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_warn.o",
        "--encoder-warn",
        "--sarif",
        sarif_path,
    ]);

    // Exit code 0 (success with warning)
    assert_eq!(
        output.status.code(),
        Some(0),
        "encoder with --encoder-warn should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // SARIF file exists and contains B1706 at warning level
    let results = extract_sarif_results(sarif_path)
        .expect("SARIF file should exist and be valid JSON");

    assert!(!results.is_empty(), "SARIF results should not be empty");

    let has_b1706_warning = results.iter().any(|result| {
        let has_rule = result
            .pointer("/ruleId")
            .and_then(|v| v.as_str())
            .map(|rid| rid.contains("B1706"))
            .unwrap_or(false);

        let has_level = result
            .pointer("/level")
            .and_then(|v| v.as_str())
            .map(|level| level == "warning")
            .unwrap_or(false);

        has_rule && has_level
    });

    assert!(
        has_b1706_warning,
        "SARIF results should contain B1706 at warning level"
    );
}

#[test]
#[ignore = "TODO(#1488): U1610 fixup-pass fixture unreachable via user syntax; see paideia-as#1553"]
fn unresolved_label_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify U1610 fires on unresolved labels in fixup pass.
    // Scope re-check (paideia-as#1553): parse_operand_from_ast falls back to
    // SymbolRef for unknown identifiers with `jmp`, so a `.pdx` fixture cannot
    // reach the LabelFixup path that emits U1610. Either add a unit-level driver
    // that synthesizes a LabelRef without insert_label, retire the fixup-pass
    // U1610 as unreachable, or relax operand parsing. Left ignored pending decision.
    panic!("TODO: create fixture with unresolved label reference (see paideia-as#1553)");
}

#[test]
fn symbol_layout_invalid_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004 / PAS-DEBT-B1-006 (paideia-as#1489):
    // Verify B1703 fires on symbol layout validation failures.
    //
    // The fixture `duplicate_symbol.pdx` carries two `pub let` bindings
    // that share the same `@fingerprint("dup_tag")` attribute. The
    // elaborator stages two `.rodata` DataEntries under the identical
    // synthetic symbol `fp_dup_tag`; `writer.finalize()`'s
    // `validate_symbol_layout` catches the duplicate name and returns
    // `EmitterError::SymbolLayoutInvalid`, which `build_elf_object`
    // maps to a typed B1703 diagnostic and returns `BuildError::Failed`
    // (exit code 2 via `finish_build_error`).
    let input = build_emit_data("duplicate_symbol.pdx");
    let sarif_path = "/tmp/test_symbol_layout_invalid_diagnostic.sarif.json";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_symbol_layout_invalid.o",
        "--sarif",
        sarif_path,
    ]);

    // Exit code 2 (BuildError::Failed → finish_build_error).
    assert_eq!(
        output.status.code(),
        Some(2),
        "symbol layout failure should exit 2. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // SARIF file exists and contains B1703.
    let results = extract_sarif_results(sarif_path)
        .expect("SARIF file should exist and be valid JSON");

    assert!(!results.is_empty(), "SARIF results should not be empty");

    let has_b1703 = results.iter().any(|result| {
        result
            .pointer("/ruleId")
            .and_then(|v| v.as_str())
            .map(|rid| rid.contains("B1703"))
            .unwrap_or(false)
    });

    assert!(has_b1703, "SARIF results should contain B1703 ruleId");
}

#[test]
fn lambda_no_offset_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1704 fires on missing function symbol offsets
    // and is surfaced through the typed SARIF pipeline.
    //
    // Fixture rationale (paideia-as#1537 / PAS-DEBT-B1-007): the (Var,
    // Literal) `+` fast-path in `emit_visit_lambda.rs` (lines ~1011-1015)
    // rejects any immediate outside the disp8 range (-128..=127) by
    // *skipping emission entirely* — no `record_lambda_entry` fires, so
    // the lambda ends up with a Function symbol in `arena.symbols()` but
    // no `lambda_first_instr` entry. The `elf.rs` symbol-emission loop
    // (lines ~294-315) then hits the `None` branch of
    // `function_offsets.get(&symbol.ir_node.get())` and emits B1704 as a
    // warning (see `cmd_build/diagnostics.rs::function_symbol_no_offset`).
    //
    // Reuses the existing `pa8_add_imm_out_of_range.pdx` fixture — same
    // shape as `pa8_add_imm_out_of_range.rs`, but this test exercises the
    // typed SARIF surface rather than the stderr text.
    let input = build_emit_data("pa8_add_imm_out_of_range.pdx");
    let sarif_path = "/tmp/test_lambda_no_offset_diagnostic.sarif.json";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_lambda_no_offset.o",
        "--sarif",
        sarif_path,
    ]);

    // Exit code 0 — B1704 is a warning (build succeeds with st_value=0/st_size=0
    // placeholder for the un-emitted function symbol).
    assert_eq!(
        output.status.code(),
        Some(0),
        "build should exit 0 (B1704 is a warning). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // SARIF file exists and contains B1704 at warning level.
    let results = extract_sarif_results(sarif_path)
        .expect("SARIF file should exist and be valid JSON");

    assert!(!results.is_empty(), "SARIF results should not be empty");

    let has_b1704_warning = results.iter().any(|result| {
        let has_rule = result
            .pointer("/ruleId")
            .and_then(|v| v.as_str())
            .map(|rid| rid.contains("B1704"))
            .unwrap_or(false);

        let has_level = result
            .pointer("/level")
            .and_then(|v| v.as_str())
            .map(|level| level == "warning")
            .unwrap_or(false);

        has_rule && has_level
    });

    assert!(
        has_b1704_warning,
        "SARIF results should contain B1704 at warning level. results: {:?}",
        results
    );
}

#[test]
fn encoder_warn_diagnostic_appears_in_stderr_when_no_sarif() {
    // Phase 8 m1-004: Verify B1706 appears in stderr when --encoder-warn is used without --sarif.
    let input = build_emit_data("encoder_strict_unsupported.pdx");

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_warn_stderr.o",
        "--encoder-warn",
    ]);

    // Exit code 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "encoder with --encoder-warn should exit 0"
    );

    // Stderr contains warning[B1706]: (may have ANSI color codes)
    let stderr = String::from_utf8_lossy(&output.stderr);
    let has_b1706 = stderr.contains("B1706");
    assert!(
        has_b1706,
        "stderr should contain 'B1706'. stderr: {}",
        stderr
    );
}
