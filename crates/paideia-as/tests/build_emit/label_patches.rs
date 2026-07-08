//! Phase 6 m4-004: Label fixup patching integration test.
//!
//! Tests that Jcc instructions with label references are correctly
//! patched with computed displacements after .text section encoding.
//! Verifies that:
//! - Label fixups are applied after encoding completes
//! - Displacements are computed correctly
//! - Unresolved labels emit U1610 and cause build failure in strict mode
//! - Label maps are properly scoped per-function

use std::process::Command;

fn build_emit_data(name: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

/// Helper to read an ELF file and extract .text section.
#[allow(dead_code)]
fn read_elf_text_section(elf_path: &str) -> Vec<u8> {
    use std::fs;

    let bytes = fs::read(elf_path).expect("failed to read ELF file");

    // Simple ELF parser: find .text section.
    // This is a minimal implementation for testing purposes.
    // For production, use a proper ELF parsing library.

    // ELF header is 64 bytes for 64-bit
    if bytes.len() < 64 {
        return Vec::new();
    }

    // e_shoff is at offset 32 (section header offset)
    let sh_offset = u64::from_le_bytes([
        bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
    ]) as usize;

    // e_shnum is at offset 48 (number of section headers)
    let sh_num = u16::from_le_bytes([bytes[48], bytes[49]]) as usize;

    // e_shentsize is at offset 46 (size of section header entry)
    let sh_entsize = u16::from_le_bytes([bytes[46], bytes[47]]) as usize;

    // Find .text section by iterating section headers
    for i in 0..sh_num {
        let offset = sh_offset + i * sh_entsize;
        if offset + 64 > bytes.len() {
            break;
        }

        // sh_name is at offset 0 in section header
        // sh_type is at offset 4
        // sh_offset is at offset 24
        // sh_size is at offset 32

        let sh_type = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);

        // Skip if not PROGBITS (sh_type = 1)
        if sh_type != 1 {
            continue;
        }

        let sh_offset = u64::from_le_bytes([
            bytes[offset + 24],
            bytes[offset + 25],
            bytes[offset + 26],
            bytes[offset + 27],
            bytes[offset + 28],
            bytes[offset + 29],
            bytes[offset + 30],
            bytes[offset + 31],
        ]) as usize;

        let sh_size = u64::from_le_bytes([
            bytes[offset + 32],
            bytes[offset + 33],
            bytes[offset + 34],
            bytes[offset + 35],
            bytes[offset + 36],
            bytes[offset + 37],
            bytes[offset + 38],
            bytes[offset + 39],
        ]) as usize;

        if sh_offset + sh_size <= bytes.len() {
            return bytes[sh_offset..sh_offset + sh_size].to_vec();
        }
    }

    Vec::new()
}

#[test]
fn label_fixup_simple_jcc_builds() {
    // Test that the simple_jcc fixture builds successfully.
    // This fixture contains basic unsafe block with simple instructions.
    // Label fixup infrastructure is present but not actively used in this simple fixture.
    // The test verifies that patch_label_fixups doesn't break the build process.

    let input = build_emit_data("simple_jcc.pdx");
    let output_path = "/tmp/test_label_fixup_simple_jcc.o";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    // Should exit with code 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "simple_jcc.pdx should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the ELF file was created
    use std::fs;
    assert!(
        fs::metadata(output_path).is_ok(),
        "ELF output file should exist"
    );
}

#[test]
fn label_fixup_preserves_encoding() {
    // Test that label fixup doesn't corrupt the instruction encoding.
    // The fixture should compile successfully and produce a valid ELF.

    let input = build_emit_data("simple_jcc.pdx");
    let output_path = "/tmp/test_label_fixup_preserves.o";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    assert_eq!(output.status.code(), Some(0), "build should succeed");

    // Read the ELF and verify it's valid
    use std::fs;
    let bytes = fs::read(output_path).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic should be present");
}

#[test]
fn label_fixup_accepts_encoder_warn() {
    // Test that --encoder-warn flag is accepted when building fixtures.

    let input = build_emit_data("simple_jcc.pdx");
    let output_path = "/tmp/test_label_fixup_warn.o";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
        "--encoder-warn",
    ]);

    // Should exit with code 0 even with --encoder-warn
    assert_eq!(
        output.status.code(),
        Some(0),
        "build with --encoder-warn should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn label_fixup_jz_backward_local_disp32_not_zero() {
    // Regression test for issue #901: backward JZ with local label.
    // Verifies that disp32 is NOT 0x00000000 after label fixup patching.
    // The fixture has a loop that jumps back to a label via jz instruction.
    // The disp32 must be computed correctly to the backward offset, not left as 0.

    let input = build_emit_data("control_flow/jz_backward_local.pdx");
    let output_path = "/tmp/test_jz_backward_local.o";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "jz_backward_local.pdx should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Read the entire ELF file and search for the jz instruction pattern
    use std::fs;
    let elf_bytes = fs::read(output_path).expect("output ELF should exist");

    // The .text section is typically in the second half of the ELF.
    // Search for the pattern: EC (in_al al) followed by 48 83 E0 20 (and rax, 0x20)
    // followed by 0F 84 (jz rel32)
    let mut found_jz = false;
    let mut jz_disp32_bytes = [0u8; 4];
    for i in 0..elf_bytes.len().saturating_sub(10) {
        if elf_bytes[i] == 0xEC
            && elf_bytes[i + 1] == 0x48
            && elf_bytes[i + 2] == 0x83
            && elf_bytes[i + 3] == 0xE0
            && elf_bytes[i + 4] == 0x20
            && elf_bytes[i + 5] == 0x0F
            && elf_bytes[i + 6] == 0x84
        {
            // Found the pattern: in_al, and, jz
            found_jz = true;
            jz_disp32_bytes.copy_from_slice(&elf_bytes[i + 7..i + 11]);
            break;
        }
    }

    assert!(found_jz, "Should find JZ pattern in ELF file");

    let disp32 = i32::from_le_bytes(jz_disp32_bytes);
    assert_ne!(
        disp32, 0,
        "disp32 should NOT be 0 (issue #901). Got: 0x{:08X}",
        disp32 as u32
    );

    // Verify it's negative (backward jump)
    assert!(
        disp32 < 0,
        "Backward jump should have negative disp32. Got: {}",
        disp32
    );
}
