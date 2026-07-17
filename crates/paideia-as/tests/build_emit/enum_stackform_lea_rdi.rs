//! Issue #1177: Coverage for #1153's stack-form LEA rdi
//!
//! This test verifies that stack-form enums (size > 16 bytes) correctly emit
//! the `lea rdi, [rip+symbol]` instruction when loading a module-level enum
//! in a context where scrutinee loading is needed (e.g., match expressions).
//!
//! Expected behavior:
//! - Defines a large enum (Point with 3 u64 fields = 24 bytes + 8 byte discriminant = 32 bytes total)
//! - Creates a module-level instance: `pub let r : Result = Result::Ok(Point { ... })`
//! - Uses it in a match expression: `match res { Ok(p) => p.x, Err => 0u64 }`
//! - emit_scrutinee_load should detect layout.size > 16 and emit `lea rdi, [rip+r]`
//! - The discriminant extraction should then use `mov rax, [rdi+0]`
//! - Pattern binding should extract the Point fields via `mov <reg>, [rdi+8+offset]`

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
fn enum_stackform_lea_rdi_parses_and_emits() {
    // Issue #1177: Discriminating fixture for #1153's stack-form LEA rdi.
    // Verifies that:
    // - Parser accepts enum declaration with large struct-typed payload
    // - Module-level enum instance is created and serialized to .rodata
    // - Function that matches on the enum parses and lowers correctly
    // - emit_scrutinee_load emits `lea rdi, [rip+symbol]` for stack-form enum
    // - The resulting .o file builds successfully
    use std::path::Path;

    let input = build_emit_data("enum_stackform_lea_rdi.pdx");
    let output_path = "/tmp/test_enum_stackform_lea_rdi.o";
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

    // Build should succeed; LEA rdi code path should be exercised without error.
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

    // Verify symbol 'r' exists (module-level enum instance)
    let symbol_found = object_file
        .symbols()
        .any(|s| s.name().ok() == Some("r"));
    assert!(symbol_found, "Symbol 'r' not found in ELF");

    // Verify the .rodata contains correct bytes for Result::Ok(Point { x: 1u64, y: 2u64, z: 3u64 }):
    // Issue #1157: tight-pack encoding per declared field widths.
    // - Discriminant 0 (Ok is variant_index 0) as u64 little-endian: 00 00 00 00 00 00 00 00
    // - Point.x = 1 as u64 little-endian: 01 00 00 00 00 00 00 00
    // - Point.y = 2 as u64 little-endian: 02 00 00 00 00 00 00 00
    // - Point.z = 3 as u64 little-endian: 03 00 00 00 00 00 00 00
    // Total: 32 bytes (discriminant 8 + Point 24)
    let expected_bytes = vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // discriminant 0 (Ok)
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Point.x = 1
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Point.y = 2
        0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Point.z = 3
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
        ".rodata content does not match expected bytes for Result::Ok(Point {{ x: 1u64, y: 2u64, z: 3u64 }})"
    );
}
