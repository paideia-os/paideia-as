//! Phase 7 m4-003 (#1048/#1049): Enum variant constructor integration test
//!
//! This test verifies that `Result::Ok(42u64)` enum variant constructor expressions
//! are correctly parsed, lowered to IR as EnumCons nodes, and emit successfully.
//!
//! Expected behavior:
//! - Parser accepts enum definition with tuple variants
//! - Enum variant call `Result::Ok(42u64)` is recognized as an enum constructor
//! - AST→IR lowering converts the call to IrKind::EnumCons
//! - EnumConsInfo side-table is populated with type_id and variant_index
//! - If #1050 (enum_layouts population) is complete: emit succeeds with correct layout
//! - If #1050 is not yet done: emit may fail at layout stage (expected)

use std::path::PathBuf;
use std::process::Command;
use object::{Object, ObjectSection, ObjectSymbol};

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

#[test]
fn enum_cons_parses_and_lowers() {
    // Phase 7 m4-003 AC: enum_cons_result_ok.pdx parses, lowers, and emits correctly.
    // Verifies that:
    // - Parser accepts enum declaration with tuple variants
    // - Enum variant call `Result::Ok(42u64)` is parsed as ExprCall
    // - AST→IR lowering recognizes it as EnumCons with correct children (payload only, no callee)
    // - Emit produces correct .rodata bytes for Result::Ok(42u64)
    use std::path::Path;

    let input = build_emit_data("enum_cons_result_ok.pdx");
    let output_path = "/tmp/test_enum_cons_result_ok.o";
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // PA-r17-007 (#1050): After #1050 is implemented, the build should now succeed.
    // Verify the ELF output contains correct layout data for Result::Ok(42u64).
    assert!(
        output.status.success(),
        "Build failed unexpectedly. stdout:\n{}\n\nstderr:\n{}",
        stdout,
        stderr
    );

    assert!(
        Path::new(output_path).exists(),
        "Output ELF file not created at {}",
        output_path
    );

    // Parse the ELF file using the object crate
    let file_data = std::fs::read(output_path)
        .expect("Failed to read ELF file");
    let object_file = object::File::parse(&*file_data)
        .expect("Failed to parse ELF file");

    // Find the .rodata section
    let rodata_section = object_file
        .sections()
        .find(|s| s.name().ok() == Some(".rodata"))
        .expect("No .rodata section found in ELF");

    let rodata_data = rodata_section.data().expect("Failed to read .rodata data");

    // Verify the symbol 'r' exists
    let symbol_found = object_file
        .symbols()
        .any(|s| s.name().ok() == Some("r"));
    assert!(symbol_found, "Symbol 'r' not found in ELF");

    // Verify the .rodata contains the expected bytes for Result::Ok(42u64):
    // - Discriminant 0 (Ok is variant_index 0) as u64 little-endian: 00 00 00 00 00 00 00 00
    // - Payload 42 as u64 little-endian: 2a 00 00 00 00 00 00 00
    // Total: 16 bytes
    let expected_bytes = vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // discriminant 0 (Ok)
        0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // payload 42
    ];

    assert!(
        rodata_data.len() >= expected_bytes.len(),
        ".rodata section too small: {} bytes, expected at least {} bytes",
        rodata_data.len(),
        expected_bytes.len()
    );

    // Check that the bytes match the expected pattern
    assert_eq!(
        &rodata_data[..expected_bytes.len()],
        expected_bytes.as_slice(),
        ".rodata content does not match expected bytes for Result::Ok(42u64)"
    );
}
