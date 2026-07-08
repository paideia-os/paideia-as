//! Issue #1091: Enum variant constructor with RecordCons payload integration test
//!
//! This test verifies that `Result::Ok(Point { x: 5u32, y: 10u32 })` enum variant
//! constructor expressions with record literal payloads are correctly parsed, lowered
//! to IR as EnumCons with nested RecordCons, and emit successfully to correct bytes.
//!
//! Expected behavior:
//! - Parser accepts enum definition with struct-typed variant payloads
//! - Enum variant call `Result::Ok(Point { x: 5u32, y: 10u32 })` is recognized as EnumCons
//! - AST→IR lowering converts the call to IrKind::EnumCons with RecordCons child
//! - EnumConsInfo and RecordLayout side-tables are populated correctly
//! - Emit succeeds with correct discriminant + record field bytes

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
fn enum_record_payload_parses_and_emits() {
    // Issue #1091 AC: enum_record_payload.pdx parses, lowers, and emits correctly.
    // Verifies that:
    // - Parser accepts enum declaration with struct-typed variant payloads
    // - Enum variant call `Result::Ok(Point { x: 5u32, y: 10u32 })` is parsed as ExprCall
    // - AST→IR lowering recognizes it as EnumCons with RecordCons child
    // - Emit produces correct .rodata bytes for Result::Ok(Point { x: 5u32, y: 10u32 })
    use std::path::Path;

    let input = build_emit_data("enum_record_payload.pdx");
    let output_path = "/tmp/test_enum_record_payload.o";
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

    // Issue #1091: After #1091 is implemented, the build should now succeed.
    // Verify the ELF output contains correct bytes for Result::Ok(Point { x: 5u32, y: 10u32 }).
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

    // Verify the .rodata contains the expected bytes for Result::Ok(Point { x: 5u32, y: 10u32 }):
    // Note: the encoder doesn't have type information, so it packs all Literal values as u64 (8 bytes each)
    // - Discriminant 0 (Ok is variant_index 0) as u64 little-endian: 00 00 00 00 00 00 00 00
    // - Point.x = 5 as u64 little-endian: 05 00 00 00 00 00 00 00
    // - Point.y = 10 as u64 little-endian: 0a 00 00 00 00 00 00 00
    // Total: 24 bytes (discriminant 8 + x 8 + y 8)
    let expected_bytes = vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // discriminant 0 (Ok)
        0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Point.x = 5
        0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Point.y = 10
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
        ".rodata content does not match expected bytes for Result::Ok(Point {{ x: 5u32, y: 10u32 }})"
    );
}
