//! Issue #1160: Enum variant constructor with primitive u32 payload integration test
//!
//! This test verifies that `Result::Ok(42u32)` enum variant constructor expressions
//! with primitive u32 payloads are correctly parsed, lowered to IR as EnumCons with
//! Literal child, and emit successfully with tight-pack encoding (4 bytes for u32,
//! not 8 bytes as would happen with width-blind encoding).
//!
//! Expected behavior:
//! - Parser accepts enum definition with primitive-typed variant payloads (u32)
//! - Enum variant call `Result::Ok(42u32)` is recognized as EnumCons
//! - AST→IR lowering converts the call to IrKind::EnumCons with Literal child
//! - EnumConsInfo is populated with variant_index and type_id
//! - enum_layout_populator captures primitive width (4) in the widths table
//! - encode_enum_cons routes the Literal through encode_ir_value_sized(width=4)
//! - Emit succeeds with correct discriminant + tight-packed u32 bytes
//! - Total symbol size is 12 bytes (8 discriminant + 4 payload), not 16

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
fn enum_primitive_payload_u32_parses_and_emits() {
    // Issue #1160 AC: enum_primitive_payload_u32.pdx parses, lowers, and emits correctly.
    // Verifies that:
    // - Parser accepts enum declaration with primitive-typed variant payloads (u32)
    // - Enum variant call `Result::Ok(42u32)` is parsed as ExprCall
    // - AST→IR lowering recognizes it as EnumCons with Literal child
    // - Emit produces correct .rodata bytes for Result::Ok(42u32) with tight-pack encoding
    // - Symbol size is 12 bytes (8 discriminant + 4 payload), not 16 (width-blind)
    use std::path::Path;

    let input = build_emit_data("enum_primitive_payload_u32.pdx");
    let output_path = "/tmp/test_enum_primitive_payload_u32.o";
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

    // Build should succeed with tight-pack encoding for primitive payloads.
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

    // Find the symbol 'r' and verify its size
    let symbol_r = object_file
        .symbols()
        .find(|s| s.name().ok() == Some("r"))
        .expect("Symbol 'r' not found in ELF");

    let symbol_size = symbol_r.size() as usize;
    assert_eq!(
        symbol_size, 12,
        "Symbol 'r' size should be 12 bytes (8 discriminant + 4 u32 payload), not {} (would be 16 with width-blind encoding)",
        symbol_size
    );

    // Verify the .rodata contains the expected bytes for Result::Ok(42u32):
    // Issue #1160: tight-pack encoding per declared primitive width.
    // - Discriminant 0 (Ok is variant_index 0) as u64 little-endian: 00 00 00 00 00 00 00 00
    // - Value 42 as u32 little-endian: 2a 00 00 00
    // Total: 12 bytes (discriminant 8 + u32 4)
    let expected_bytes = vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // discriminant 0 (Ok)
        0x2a, 0x00, 0x00, 0x00,                         // value 42 (u32)
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
        ".rodata content does not match expected bytes for Result::Ok(42u32)"
    );
}
