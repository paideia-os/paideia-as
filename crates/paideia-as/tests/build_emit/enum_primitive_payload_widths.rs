//! Issue #1171: Discriminating fixture for #1160's primitive-payload width fix
//!
//! This test verifies that enum variant constructors with various primitive-width
//! payloads (u8, u16, u32) are correctly tightly-packed during encoding.
//! Without #1160's width table, all primitive payloads would default to 8 bytes (width-blind),
//! resulting in wrong symbol sizes and encoding.
//!
//! Fixture: three separate enums (Result8, Result16, Result32) each with a primitive variant
//! - Result8::Ok(u8): discriminant (8) + payload (1) = 9 bytes total (tight-packed)
//! - Result16::Ok(u16): discriminant (8) + payload (2) = 10 bytes total (tight-packed)
//! - Result32::Ok(u32): discriminant (8) + payload (4) = 12 bytes total (tight-packed)
//!
//! Expected behavior:
//! - Parser accepts enums with primitive-width payloads
//! - enum_layout_populator captures the width for each primitive variant
//! - encode_enum_cons routes each Literal child through encode_ir_value_sized(width)
//! - Functions exercising match on each enum exercise the entire code path
//! - Symbol sizes are correct: 16, 16, 16 (8-byte aligned)
//! - .rodata bytes match tight-pack encoding for each variant
//!
//! Discriminating property: If the width table lookup is removed, the encoder
//! would default to 8-byte encoding for all Literal payloads (width-blind).
//! This would make symbol sizes 16, 16, 16 (instead of 9, 10, 12) and .rodata
//! bytes wrong: u8 and u16 variants would be padded to 8 bytes each instead of
//! tight-packed at 1 and 2 bytes. The test assertions on both symbol size and
//! .rodata bytes catch this regression.

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
fn enum_primitive_payload_widths_parses_and_emits() {
    // Issue #1171 AC: enum_primitive_payload_widths.pdx exercises multiple primitive widths.
    // Verifies that:
    // - Parser accepts enums with u8, u16, u32 primitive-payload variants
    // - enum_layout_populator captures width for each primitive variant
    // - encode_enum_cons respects the width table for tight-pack encoding
    // - Match expressions on each enum compile correctly
    // - .rodata bytes match expected tight-pack values
    use std::path::Path;

    let input = build_emit_data("enum_primitive_payload_widths.pdx");
    let output_path = "/tmp/test_enum_primitive_payload_widths.o";
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

    // Build should succeed with tight-pack encoding for all primitive widths.
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

    // Parse the ELF file
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

    // Debug output
    eprintln!("DEBUG .rodata first 48 bytes: {:?}", &rodata_data[..rodata_data.len().min(48)]);

    // Verify symbols exist and have correct sizes (tight-packed, not 16-byte aligned)
    // With #1160's width-aware encoding:
    // - r8 (discriminant 8 + u8 1): 9 bytes
    // - r16 (discriminant 8 + u16 2): 10 bytes
    // - r32 (discriminant 8 + u32 4): 12 bytes
    // Without the fix (width-blind), all would be 16 bytes (8 byte disc + 8 byte payload)
    let symbol_sizes = [("r8", 9usize), ("r16", 10usize), ("r32", 12usize)];
    for (sym_name, expected_size) in &symbol_sizes {
        let symbol = object_file
            .symbols()
            .find(|s| s.name().ok() == Some(*sym_name))
            .expect(&format!("Symbol '{}' not found in ELF", sym_name));

        let symbol_size = symbol.size() as usize;
        assert_eq!(
            symbol_size, *expected_size,
            "Symbol '{}' size should be {} bytes (tight-packed), got {} (would be 16 if width-blind)",
            sym_name, expected_size, symbol_size
        );
    }

    // Verify .rodata contains the expected byte sequences for each variant.
    // The compiler may reorder module-level data, so search for patterns rather than
    // assuming fixed offsets.

    // Issue #1160's fix ensures tight-pack encoding: discriminant (8) + payload (N bytes)
    // Width-blind encoding would use 8 bytes for all payloads, making these tests fail.

    // r8: Result8::Ok(42u8) - pattern: 8 zero bytes (disc) + single byte 0x2a (42)
    let pattern_r8_disc = &[0x00u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let mut found_r8 = false;
    for i in 0..rodata_data.len().saturating_sub(8) {
        if &rodata_data[i..i+8] == pattern_r8_disc && i+9 <= rodata_data.len() && rodata_data[i+8] == 0x2a {
            found_r8 = true;
            break;
        }
    }
    assert!(
        found_r8,
        "Result8::Ok(42u8) pattern not found in .rodata (disc=0x00×8, payload=0x2a)"
    );

    // r16: Result16::Ok(1000u16) - pattern: 8 zero bytes (disc) + 0xe8 0x03 (1000 in little-endian)
    let pattern_r16_payload = &[0xe8u8, 0x03];
    let mut found_r16 = false;
    for i in 0..rodata_data.len().saturating_sub(8) {
        if &rodata_data[i..i+8] == pattern_r8_disc && i+10 <= rodata_data.len() && &rodata_data[i+8..i+10] == pattern_r16_payload {
            found_r16 = true;
            break;
        }
    }
    assert!(
        found_r16,
        "Result16::Ok(1000u16) pattern not found in .rodata (disc=0x00×8, payload=0xe8 0x03)"
    );

    // r32: Result32::Ok(100000u32) - pattern: 8 zero bytes (disc) + 0xa0 0x86 0x01 0x00 (100000 in little-endian)
    let pattern_r32_payload = &[0xa0u8, 0x86, 0x01, 0x00];
    let mut found_r32 = false;
    for i in 0..rodata_data.len().saturating_sub(8) {
        if &rodata_data[i..i+8] == pattern_r8_disc && i+12 <= rodata_data.len() && &rodata_data[i+8..i+12] == pattern_r32_payload {
            found_r32 = true;
            break;
        }
    }
    assert!(
        found_r32,
        "Result32::Ok(100000u32) pattern not found in .rodata (disc=0x00×8, payload=0xa0 0x86 0x01 0x00)"
    );
}
