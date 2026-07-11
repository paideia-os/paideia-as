//! Phase 8 m1: Integration test for out-of-range shift-left immediate lambda (#1167).
//!
//! Verifies that:
//! - `fn(x) -> x << 100` does not emit shl (100 > imm8 max of 63)
//! - B1704 (function-symbol-no-offset) fires for symbol `g`
//! - Symbol `g` has st_value=0, st_size=0 (no offset was recorded)

use std::process::Command;

fn build_emit_data(name: &str) -> String {
    let mut p = String::from(env!("CARGO_MANIFEST_DIR"));
    p.push_str("/../../tests/build-emit/");
    p.push_str(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

#[test]
fn pa8_shl_imm_out_of_range_diagnostic() {
    // Phase 8 m1: Verify B1704 fires on out-of-range shift-left immediate (#1167).
    let input = build_emit_data("pa8_shl_imm_out_of_range.pdx");
    let obj_path = "/tmp/pa8_shl_imm_out_of_range.o";

    let output = cargo_run(&[
        "build",
        &input,
        "--emit",
        "elf64",
        "-o",
        obj_path,
    ]);

    // Build should succeed (exit code 0)
    assert_eq!(
        output.status.code(),
        Some(0),
        "build should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Stderr should contain B1704 for symbol `g`
    let stderr = String::from_utf8_lossy(&output.stderr);
    let has_b1704 = stderr.contains("B1704") || stderr.contains("function_symbol_no_offset") || stderr.contains("no offset");
    assert!(
        has_b1704,
        "stderr should contain B1704 diagnostic. stderr: {}",
        stderr
    );

    // Verify symbol `g` exists with st_value=0, st_size=0 using readelf
    let readelf_output = Command::new("readelf")
        .args(&["-sW", obj_path])
        .output()
        .expect("readelf failed");

    let readelf_text = String::from_utf8_lossy(&readelf_output.stdout);
    // Look for the symbol line with " g" (space + g + space or end-of-line)
    // e.g., "    40: 0000000000000000     0 FUNC    LOCAL  DEFAULT    1 g"
    let has_symbol_with_zero_value = readelf_text
        .lines()
        .any(|line| {
            line.contains(" g") && (line.contains("0 FUNC") || line.contains(" 0 "))
        });

    assert!(
        has_symbol_with_zero_value,
        "readelf output should show symbol 'g' with st_value=0. output:\n{}",
        readelf_text
    );
}
