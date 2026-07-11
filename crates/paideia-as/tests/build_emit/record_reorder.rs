//! Issue #1092: Record literal field canonicalization integration test
//!
//! This test verifies that `Point { y: 20u32, x: 10u32 }` record literals are
//! canonicalized to declared field order (x, y) rather than literal order (y, x),
//! and that the emitted .rodata bytes reflect the declared order.
//!
//! Expected behavior:
//! - Parser accepts record literal with fields in any order
//! - AST→IR lowering canonicalizes field order to match record declaration
//! - RecordCons children are reordered from literal order (y, x) to declared order (x, y)
//! - Emit produces correct .rodata bytes in declared order: x=10 followed by y=20

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
fn record_reorder_parses_and_emits_in_declared_order() {
    // Issue #1092 AC: pa_r17_1092_record_reorder.pdx parses, lowers, and emits correctly.
    // Verifies that:
    // - Parser accepts record literal with fields in any order (y, x)
    // - AST→IR lowering canonicalizes fields to declared order (x, y)
    // - RecordCons IR children are reordered from literal to declared order
    // - Emit produces correct .rodata bytes in declared order: x=10 then y=20
    use std::path::Path;

    let input = build_emit_data("pa_r17_1092_record_reorder.pdx");
    let output_path = "/tmp/test_record_reorder.o";
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

    // Verify the build succeeded
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

    // Verify the .rodata contains the expected bytes for Point { y: 20u32, x: 10u32 }
    // reordered to declared order Point { x: 10u32, y: 20u32 }:
    // Issue #1157: tight-pack encoding respects declared field widths (u32 = 4 bytes each)
    // - Point.x = 10 as u32 little-endian: 0a 00 00 00
    // - Point.y = 20 as u32 little-endian: 14 00 00 00
    // Total: 8 bytes (x 4 + y 4), and they should be in declared order
    let expected_bytes = vec![
        0x0a, 0x00, 0x00, 0x00, // Point.x = 10 (declared first, u32)
        0x14, 0x00, 0x00, 0x00, // Point.y = 20 (declared second, u32)
    ];

    assert!(
        rodata_data.len() >= expected_bytes.len(),
        ".rodata section too small: {} bytes, expected at least {} bytes",
        rodata_data.len(),
        expected_bytes.len()
    );

    // Check that the bytes match the expected pattern (declared order, not literal order)
    assert_eq!(
        &rodata_data[..expected_bytes.len()],
        expected_bytes.as_slice(),
        ".rodata content does not match expected bytes for Point {{ y: 20u32, x: 10u32 }} \
         canonicalized to declared order Point {{ x: 10u32, y: 20u32 }}"
    );
}
