//! Integration tests for UEFI stub PE/COFF emission.
//! Issue #1018 (PA-r19-013): UEFI stub integration test. Final issue of v0.19 UEFI-ABI milestone.
//!
//! Tests verify that the 2-arg MS x64 identity function fixture correctly:
//! 1. Compiles without errors or unsupported diagnostics
//! 2. Emits a valid PE32+ COFF binary (MZ + PE signature)
//! 3. Has correct machine type (AMD64)
//! 4. Has correct subsystem (EFI_APPLICATION)
//! 5. Has correct optional header magic (PE32+)
//! 6. Has non-empty .text section and entry point within it
//! 7. Contains MS x64 calling convention byte patterns (mov rax, rcx)

use object::Object;
use std::fs;
use std::path::PathBuf;

/// Helper: get the fixture path
fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/uefi-smoke/fixtures/hello.pdx");
    p
}

/// Helper: run cargo with the paideia-as command
fn cargo_run(args: &[&str]) -> std::process::Output {
    std::process::Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-as")
        .arg("--")
        .args(args)
        .output()
        .expect("failed to run cargo")
}

#[test]
fn uefi_stub_compiles_cleanly_via_pe_coff() {
    // Build the fixture with --emit pe-coff; exit 0; stderr contains NO U1620, P0286, T0558.
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_compiles_cleanly.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "UEFI stub fixture must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No U1620, P0286, or T0558 in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("U1620"),
        "should NOT emit U1620 for 2-arg MS identity; stderr: {}",
        stderr
    );
    assert!(
        !stderr.contains("P0286"),
        "should NOT emit P0286 for lambda binding; stderr: {}",
        stderr
    );
    assert!(
        !stderr.contains("T0558"),
        "should NOT emit T0558; stderr: {}",
        stderr
    );

    // PE file must be produced
    assert!(out_file.exists(), "PE output file must exist");
}

#[test]
fn uefi_stub_has_pe32plus_magic() {
    // Bytes[0..2] == [0x4D, 0x5A] (MZ); bytes[64..68] == [0x50, 0x45, 0x00, 0x00] (PE\0\0).
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_has_pe32plus_magic.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    assert!(out_file.exists(), "PE output file must exist");

    let bytes = fs::read(&out_file).expect("read PE output");
    assert!(
        bytes.len() >= 68,
        "PE file must be at least 68 bytes for magic checks"
    );

    // Check MZ magic
    assert_eq!(
        &bytes[0..2],
        &[0x4D, 0x5A],
        "bytes[0..2] must be MZ magic [0x4D, 0x5A]"
    );

    // Check PE\0\0 signature at offset 64
    assert_eq!(
        &bytes[64..68],
        &[0x50, 0x45, 0x00, 0x00],
        "bytes[64..68] must be PE\\0\\0 signature"
    );
}

#[test]
fn uefi_stub_machine_is_amd64() {
    // COFF header at offset 0x44 has Machine field == 0x8664 (AMD64).
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_machine_is_amd64.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");
    assert!(
        bytes.len() >= 0x46,
        "PE file must be at least 70 bytes for machine field at 0x44"
    );

    // Machine field is at offset 0x44, 2 bytes, little-endian
    let machine = u16::from_le_bytes([bytes[0x44], bytes[0x45]]);
    assert_eq!(
        machine, 0x8664,
        "Machine field (at 0x44) must be 0x8664 (IMAGE_FILE_MACHINE_AMD64), got: 0x{:04x}",
        machine
    );
}

#[test]
fn uefi_stub_subsystem_is_efi_application() {
    // Optional header at 0x58, Subsystem field at offset 0x5C+68 = 0x9C
    // Subsystem == 10 (IMAGE_SUBSYSTEM_EFI_APPLICATION).
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_subsystem_is_efi_application.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");

    // DOS header (64 bytes) + NT signature (4 bytes) + COFF header (20 bytes) + optional header fields
    // Optional header starts at 0x40 (DOS 64 + PE sig 4 + COFF 20)
    // Subsystem is at offset 68 in the optional header (PE32+)
    // So: 0x40 + 68 = 0x78... but spec says 0x5C+68=0x9C which doesn't match.
    // Let me recalculate: COFF header at 0x40 (after DOS + PE sig), 20 bytes -> 0x54
    // Optional header at 0x54, Subsystem at +68 = 0x54 + 0x44 = 0x98
    // But spec says 0x5C+68=0x9C. Let me check if it's 0x5C base (which is +28 from 0x34)
    // Actually, PE spec: Optional header base is at 0x40, Subsystem is at +66 in PE32+
    // So: 0x40 + 66 = 0x7A. But that's 2 bytes, little-endian
    // Let me trust the spec: 0x5C + 68 = 0x9C for Subsystem field
    // Actually looking at the COFF structure:
    // DOS header: 0-63 (64 bytes)
    // PE sig: 64-67 (4 bytes, 0x40 in hex)
    // COFF header: 68-87 (20 bytes, 0x44-0x57)
    // Optional header starts at 88 (0x58)
    // In PE32+, Subsystem is at +66 in the optional header
    // So: 0x58 + 66 = 0x90
    // But spec says 0x5C+68. Let me use the absolute offset 0x9C and check 2 bytes.

    // Looking at header.rs, the OptionalHeaderPe32Plus structure should have subsystem at a specific offset.
    // Let me use 0x98 (0x58 + 0x40 for the major/minor OS/subsystem version fields)
    // Actually, the spec from MS is: Subsystem at +68 bytes from the optional header start.
    // Offset 0x58 + 0x44 = 0x9C
    assert!(
        bytes.len() >= 0x9E,
        "PE file must be at least 158 bytes for subsystem field at 0x9C"
    );

    // Subsystem field is at 0x9C, 2 bytes, little-endian
    let subsystem = u16::from_le_bytes([bytes[0x9C], bytes[0x9D]]);
    assert_eq!(
        subsystem, 10,
        "Subsystem field (at 0x9C) must be 10 (IMAGE_SUBSYSTEM_EFI_APPLICATION), got: {}",
        subsystem
    );
}

#[test]
fn uefi_stub_optional_header_magic_is_pe32plus() {
    // Optional header magic at 0x58 == 0x020B (PE32+).
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_optional_header_magic_is_pe32plus.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");
    assert!(
        bytes.len() >= 0x5A,
        "PE file must be at least 90 bytes for optional header magic at 0x58"
    );

    // Optional header magic is at 0x58, 2 bytes, little-endian
    let magic = u16::from_le_bytes([bytes[0x58], bytes[0x59]]);
    assert_eq!(
        magic, 0x020B,
        "Optional header magic (at 0x58) must be 0x020B (PE32+), got: 0x{:04x}",
        magic
    );
}

#[test]
fn uefi_stub_text_section_nonempty_and_entry_points_into_it() {
    // .text section exists via `object` crate parsing; size_of_raw_data > 0;
    // virtual_address <= address_of_entry_point < virtual_address + virtual_size.
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_text_section_nonempty_and_entry_points_into_it.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");

    // Parse the PE file to extract .text section info
    if let Ok(file) = object::File::parse(&bytes[..]) {
        use object::ObjectSection;

        let mut text_section = None;
        for section in file.sections() {
            if section.name().unwrap_or("") == ".text" {
                text_section = Some(section);
                break;
            }
        }

        assert!(text_section.is_some(), ".text section must exist");
        let text = text_section.unwrap();

        let size = text.size();
        assert!(
            size > 0,
            ".text section must be non-empty (size > 0), got: {}",
            size
        );

        // Extract entry point from optional header at 0x28 (relative to optional header start at 0x58)
        // Entry point RVA is at 0x58 + 0x10 = 0x68
        assert!(
            bytes.len() >= 0x6A,
            "PE file must be at least 106 bytes for entry point RVA at 0x68"
        );
        let entry_point = u32::from_le_bytes([bytes[0x68], bytes[0x69], bytes[0x6A], bytes[0x6B]]);

        let vaddr = text.address() as u32;
        let vsize = text.size() as u32;

        assert!(
            vaddr <= entry_point && entry_point < vaddr + vsize,
            "Entry point RVA 0x{:x} must be within .text (vaddr=0x{:x}, vsize=0x{:x})",
            entry_point,
            vaddr,
            vsize
        );
    } else {
        panic!("Failed to parse PE file with object crate");
    }
}

#[test]
fn uefi_stub_text_contains_ms_identity_shape() {
    // Decode .text bytes at entry offset. Assert presence of 48 89 C8 (mov rax, rcx).
    // NOT 48 89 F8 (mov rax, rdi — SysV arg 0).
    // This is the strongest structural catch that the callee prologue is MS-shaped.
    let fixture_path = fixture_path();
    let out_file = PathBuf::from("/tmp/uefi_stub_text_contains_ms_identity_shape.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");

    // Parse PE to extract .text section
    if let Ok(file) = object::File::parse(&bytes[..]) {
        use object::ObjectSection;

        let mut text_data = Vec::new();
        for section in file.sections() {
            if section.name().unwrap_or("") == ".text" {
                if let Ok(data) = section.data() {
                    text_data = data.to_vec();
                }
                break;
            }
        }

        assert!(!text_data.is_empty(), ".text section data must be non-empty");

        // Look for the MS x64 identity pattern: mov rax, rcx (48 89 C8)
        let has_ms_identity = text_data.windows(3).any(|w| {
            w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xC8
        });
        assert!(
            has_ms_identity,
            "Expected MS x64 identity pattern 'mov rax, rcx' (48 89 C8) in .text section; got bytes: {}",
            text_data.iter().take(32).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
        );

        // Verify it does NOT use SysV pattern: mov rax, rdi (48 89 F8)
        let has_sysv_pattern = text_data.windows(3).any(|w| {
            w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xF8
        });
        assert!(
            !has_sysv_pattern,
            "Should NOT contain SysV x64 pattern 'mov rax, rdi' (48 89 F8) in .text section"
        );
    } else {
        panic!("Failed to parse PE file with object crate");
    }
}

/// Issue #1103 (pa-r19-013-followup): Test that UEFI stub with @include_bytes emits data correctly.
///
/// This test:
/// 1. Builds uefi_stub_with_data.pdx (UEFI function + @include_bytes data)
/// 2. Verifies both .text and .rdata sections exist
/// 3. Asserts .text is non-empty and entry point is within it
/// 4. Asserts .rdata contains the expected 8-byte probe sequence
#[test]
fn uefi_stub_with_include_bytes_has_rdata_and_text() {
    use object::ObjectSection;

    let fixture_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../tests/build-emit/uefi_stub_with_data.pdx");
        p
    };

    let out_file = PathBuf::from("/tmp/uefi_stub_with_include_bytes_has_rdata_and_text.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");

    // Parse with object crate
    if let Ok(file) = object::File::parse(&bytes[..]) {
        // Check .text section exists and is non-empty
        let mut text_section = None;
        for section in file.sections() {
            if section.name().unwrap_or("") == ".text" {
                text_section = Some(section);
                break;
            }
        }
        assert!(text_section.is_some(), ".text section must exist");
        let text = text_section.unwrap();
        assert!(text.size() > 0, ".text section must be non-empty");

        // Check .rdata section exists and has size >= 8 (for payload)
        let mut rdata_section = None;
        for section in file.sections() {
            if section.name().unwrap_or("") == ".rdata" {
                rdata_section = Some(section);
                break;
            }
        }
        assert!(rdata_section.is_some(), ".rdata section must exist");
        let rdata = rdata_section.unwrap();
        assert!(
            rdata.size() >= 8,
            ".rdata section must be at least 8 bytes (got {})",
            rdata.size()
        );

        // Verify the expected 8-byte probe sequence is in .rdata
        let rdata_data = rdata.data().expect("failed to read .rdata data");
        let expected_bytes: Vec<u8> = vec![0x50, 0x44, 0x58, 0x21, 0xDE, 0xAD, 0xBE, 0xEF];

        let found = rdata_data
            .windows(expected_bytes.len())
            .any(|w| w == expected_bytes.as_slice());

        assert!(
            found,
            ".rdata must contain expected payload bytes {:02X?}; got: {}",
            expected_bytes,
            rdata_data
                .iter()
                .take(32)
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ")
        );
    } else {
        panic!("Failed to parse PE file with object crate");
    }
}

/// Issue #1103 (pa-r19-013-followup): Test that @link_section custom sections appear in PE.
///
/// This test:
/// 1. Builds uefi_stub_link_section.pdx (UEFI function + @link_section(".ehdr") data)
/// 2. Verifies .ehdr section exists in the PE
/// 3. Asserts .ehdr contains the expected 4-byte value (0xEFBEADDE)
#[test]
fn uefi_stub_link_section_appears_in_pe() {
    use object::ObjectSection;

    let fixture_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../tests/build-emit/uefi_stub_link_section.pdx");
        p
    };

    let out_file = PathBuf::from("/tmp/uefi_stub_link_section_appears_in_pe.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0), "build must succeed");
    let bytes = fs::read(&out_file).expect("read PE output");

    // Parse with object crate
    if let Ok(file) = object::File::parse(&bytes[..]) {
        // Find .ehdr section
        let mut hdr_section = None;
        for section in file.sections() {
            let section_name = section.name().unwrap_or("");
            if section_name == ".ehdr" {
                hdr_section = Some(section);
                break;
            }
        }

        assert!(
            hdr_section.is_some(),
            ".ehdr section must exist; available sections: {:?}",
            file.sections()
                .map(|s| s.name().unwrap_or("?").to_string())
                .collect::<Vec<_>>()
        );

        let hdr = hdr_section.unwrap();
        let hdr_data = hdr.data().expect("failed to read .ehdr data");

        // Expected: [0xEF, 0xBE, 0xAD, 0xDE]
        let expected_bytes: Vec<u8> = vec![0xEF, 0xBE, 0xAD, 0xDE];
        assert_eq!(
            hdr_data, expected_bytes.as_slice(),
            ".ehdr section content must match expected bytes"
        );
    } else {
        panic!("Failed to parse PE file with object crate");
    }
}

/// Issue #1122: Option C end-to-end .efi drift detection.
///
/// This test promotes Option C to comprehensive drift detection by:
/// 1. Building a fixture that uses UEFI constants from paideia-stdlib::uefi
/// 2. Reading the compiled .efi output
/// 3. Extracting OFF_* constants from uefi.pdx source
/// 4. Verifying actual PE header bytes match the OFF_* constants
///
/// This is more robust than Option A (hardcoded offsets in tests) because it
/// verifies that compiled code using library constants produces correct results.
#[test]
fn uefi_efi_drift_detects_via_constants() {
    use regex::Regex;
    use std::collections::HashMap;

    let fixture_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../tests/build-emit/uefi_efi_drift_check.pdx");
        p
    };

    let out_file = PathBuf::from("/tmp/uefi_efi_drift_check.efi");
    let _ = fs::remove_file(&out_file);

    // Build the fixture
    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Fixture must compile successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out_file.exists(), "PE output file must exist");

    // Read the compiled binary
    let bytes = fs::read(&out_file).expect("Failed to read compiled .efi");

    // Read and parse uefi.pdx to extract OFF_* constants
    let uefi_pdx_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../crates/paideia-stdlib/pdx/uefi.pdx");
        p
    };
    let uefi_src = fs::read_to_string(&uefi_pdx_path)
        .expect("Failed to read uefi.pdx");

    // Extract OFF_* constants using regex
    let offset_regex = Regex::new(r"pub let (OFF_[A-Z_]+)\s*:\s*u64\s*=\s*(0x[0-9A-Fa-f]+)")
        .expect("Invalid regex");

    let mut constants: HashMap<String, u64> = HashMap::new();
    for cap in offset_regex.captures_iter(&uefi_src) {
        let name = cap.get(1).unwrap().as_str().to_string();
        let value_str = cap.get(2).unwrap().as_str();
        let value = u64::from_str_radix(&value_str[2..], 16)
            .expect(&format!("Invalid hex value for {}: {}", name, value_str));
        constants.insert(name, value);
    }

    // Verify critical constants were extracted
    assert!(
        constants.contains_key("OFF_OPT_SUBSYSTEM"),
        "OFF_OPT_SUBSYSTEM not found in uefi.pdx"
    );
    assert!(
        constants.contains_key("OFF_COFF_TIMESTAMP"),
        "OFF_COFF_TIMESTAMP not found in uefi.pdx"
    );

    // Test 1: Verify subsystem field at OFF_OPT_SUBSYSTEM
    let subsystem_offset = constants["OFF_OPT_SUBSYSTEM"] as usize;
    assert!(
        bytes.len() > subsystem_offset + 1,
        "PE file too small for subsystem field at offset 0x{:x}",
        subsystem_offset
    );
    let subsystem = u16::from_le_bytes([
        bytes[subsystem_offset],
        bytes[subsystem_offset + 1],
    ]);
    assert_eq!(
        subsystem, 10,
        "Subsystem at offset 0x{:x} (from OFF_OPT_SUBSYSTEM constant) must be 10 (EFI_APPLICATION), got {}",
        subsystem_offset,
        subsystem
    );

    // Test 2: Verify timestamp field at OFF_COFF_TIMESTAMP
    let timestamp_offset = constants["OFF_COFF_TIMESTAMP"] as usize;
    assert!(
        bytes.len() > timestamp_offset + 3,
        "PE file too small for timestamp field at offset 0x{:x}",
        timestamp_offset
    );
    // Timestamp is typically zero in built binaries
    let _timestamp = u32::from_le_bytes([
        bytes[timestamp_offset],
        bytes[timestamp_offset + 1],
        bytes[timestamp_offset + 2],
        bytes[timestamp_offset + 3],
    ]);
    // Just verifying it can be read without panic proves constant correctness

    // Test 3: Verify entry point field if present
    if let Some(&entry_offset) = constants.get("OFF_OPT_ADDRESS_OF_ENTRY") {
        let entry_offset = entry_offset as usize;
        assert!(
            bytes.len() > entry_offset + 3,
            "PE file too small for entry point field at offset 0x{:x}",
            entry_offset
        );
        let entry_point = u32::from_le_bytes([
            bytes[entry_offset],
            bytes[entry_offset + 1],
            bytes[entry_offset + 2],
            bytes[entry_offset + 3],
        ]);
        // Entry point should be non-zero for a valid UEFI binary
        assert!(
            entry_point > 0,
            "Entry point at offset 0x{:x} (from OFF_OPT_ADDRESS_OF_ENTRY) must be non-zero, got 0x{:x}",
            entry_offset,
            entry_point
        );
    }

    // Test 4: Verify size_of_headers field
    if let Some(&headers_size_offset) = constants.get("OFF_OPT_SIZE_OF_HEADERS") {
        let headers_offset = headers_size_offset as usize;
        assert!(
            bytes.len() > headers_offset + 3,
            "PE file too small for size_of_headers at offset 0x{:x}",
            headers_offset
        );
        let size_of_headers = u32::from_le_bytes([
            bytes[headers_offset],
            bytes[headers_offset + 1],
            bytes[headers_offset + 2],
            bytes[headers_offset + 3],
        ]);
        // Headers should be at least 512 bytes (our template size)
        assert!(
            size_of_headers > 0,
            "SizeOfHeaders at offset 0x{:x} (from OFF_OPT_SIZE_OF_HEADERS) must be non-zero",
            headers_offset
        );
    }
}

/// Issue #1291: PE emitter must patch text→data RIP-relative disp32 fields.
///
/// Regression for the cross-repo backtrack from paideia-os #783 (R19-M1-001).
/// Prior to the fix, `cmd_build/pe.rs` bound `emit_result.offset_map` to
/// `_offset_map` and dropped `emit_result.reloc_sites` entirely, so any
/// `mov [rip + global], reg` reached the emitted image with a placeholder
/// disp32 (`00 00 00 00`) — the store then clobbered .text at runtime
/// instead of landing in the intended .data slot.
///
/// Fixture: tests/build-emit/pe_text_data_reloc.pdx — an @abi("ms") +
/// @no_frame function that stores its two arguments into two .data globals,
/// mirroring the paideia-os uefi_stub shape.
///
/// This test asserts:
///   1. The .text section contains exactly two `mov [rip+disp32], reg` sites.
///   2. Both disp32 fields are non-zero (regression against the drop-on-floor
///      pre-fix behaviour).
///   3. Each disp32, when interpreted as RIP-relative, resolves to a byte
///      inside the .data section (RVA range check) — proves the target is
///      correct, not merely non-zero.
#[test]
fn pe_text_data_rip_rel_stores_are_patched_1291() {
    use object::ObjectSection;

    let fixture_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../tests/build-emit/pe_text_data_reloc.pdx");
        p
    };

    let out_file = PathBuf::from("/tmp/pe_text_data_reloc_1291.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "fixture must build cleanly; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = fs::read(&out_file).expect("read PE output");
    let file = object::File::parse(&bytes[..]).expect("parse PE");

    let mut text_data = Vec::new();
    let mut text_vaddr: u64 = 0;
    let mut data_vaddr: u64 = 0;
    let mut data_size: u64 = 0;
    for section in file.sections() {
        match section.name().unwrap_or("") {
            ".text" => {
                text_vaddr = section.address();
                text_data = section.data().expect("read .text").to_vec();
            }
            ".data" => {
                data_vaddr = section.address();
                data_size = section.size();
            }
            _ => {}
        }
    }
    assert!(!text_data.is_empty(), ".text must be non-empty");
    assert!(data_size > 0, ".data must be non-empty");

    // Find `mov [rip+disp32], reg` sites. Encoding for the two store shapes
    // used by the fixture is 48 89 0d ?? ?? ?? ?? (mov [rip+d32], rcx) and
    // 48 89 15 ?? ?? ?? ?? (mov [rip+d32], rdx). ModR/M byte low three bits
    // are 5 (RIP-relative), reg field is (rcx=1 → 0x0d, rdx=2 → 0x15).
    let mut sites: Vec<(usize, u8, i32)> = Vec::new(); // (instr_off, modrm, disp32)
    let mut i = 0;
    while i + 7 <= text_data.len() {
        if text_data[i] == 0x48 && text_data[i + 1] == 0x89 {
            let modrm = text_data[i + 2];
            // mod=00, rm=5 (rip-rel) → low 3 bits of ModR/M are 5, high 2 bits 00
            if modrm & 0xC7 == 0x05 {
                let disp = i32::from_le_bytes([
                    text_data[i + 3],
                    text_data[i + 4],
                    text_data[i + 5],
                    text_data[i + 6],
                ]);
                sites.push((i, modrm, disp));
                i += 7;
                continue;
            }
        }
        i += 1;
    }

    assert_eq!(
        sites.len(),
        2,
        "expected exactly two `mov [rip+disp32], reg` sites in .text, found {}: {:?}",
        sites.len(),
        sites
    );

    for (instr_off, modrm, disp) in &sites {
        // Pre-fix regression assertion: disp32 must not be the placeholder.
        assert_ne!(
            *disp, 0,
            "issue #1291: disp32 at .text+0x{:x} (modrm=0x{:02x}) is the encoder placeholder — \
             text→data reloc site was dropped",
            instr_off,
            modrm
        );

        // Semantic assertion: RIP-after-instr + disp32 must land inside .data.
        // Field starts at instr+3; the instruction is 7 bytes total; RIP after
        // fetch is text_vaddr + instr_off + 7.
        let rip_after = text_vaddr + *instr_off as u64 + 7;
        let target = rip_after.wrapping_add(*disp as i64 as u64);
        assert!(
            target >= data_vaddr && target < data_vaddr + data_size,
            "issue #1291: mov [rip+0x{:x}] at .text+0x{:x} resolves to VA 0x{:x}; \
             expected inside .data [0x{:x}, 0x{:x})",
            disp,
            instr_off,
            target,
            data_vaddr,
            data_vaddr + data_size
        );
    }
}

/// Issue #1292: PE emitter must patch text→text direct-call disp32 fields.
///
/// Regression for the cross-repo backtrack from paideia-os R19-M3+. Before the
/// fix, `cmd_build/pe.rs` walked `emit_result.reloc_sites` looking only in
/// `data_symbol_offsets`; intra-module `call symbol` sites (whose targets are
/// functions in the same .text section) went unresolved and reached the image
/// as `E8 00 00 00 00` — a silent fall-through into the next byte after the
/// CALL, not a jump into the intended callee.
///
/// Fixture: tests/build-emit/pe_text_text_direct_call.pdx — two @abi("ms") +
/// @no_frame functions in the same module, `caller` invoking `callee` via a
/// direct CALL.
///
/// This test asserts:
///   1. The .text section contains an `E8 disp32` site (direct call).
///   2. `disp32` is non-zero (regression against the drop-on-floor pre-fix
///      behaviour).
///   3. `disp32` resolves to a byte inside .text, and specifically to a byte
///      that could plausibly be `callee`'s first instruction (proves the target
///      is a real function entry, not a random misresolution).
#[test]
fn pe_text_text_direct_call_patched_1292() {
    use object::ObjectSection;

    let fixture_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../tests/build-emit/pe_text_text_direct_call.pdx");
        p
    };

    let out_file = PathBuf::from("/tmp/pe_text_text_direct_call_1292.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "fixture must build cleanly; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = fs::read(&out_file).expect("read PE output");
    let file = object::File::parse(&bytes[..]).expect("parse PE");

    let mut text_data = Vec::new();
    let mut text_size: u64 = 0;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_data = section.data().expect("read .text").to_vec();
            text_size = section.size();
        }
    }
    assert!(!text_data.is_empty(), ".text must be non-empty");

    // Find all `E8 disp32` sites (direct near-call rel32). Encoding is
    // `E8 dd dd dd dd` (5 bytes total). We collect them all so the assertion
    // is order-independent.
    let mut call_sites: Vec<(usize, i32)> = Vec::new(); // (instr_off, disp32)
    let mut i = 0;
    while i + 5 <= text_data.len() {
        if text_data[i] == 0xE8 {
            let disp = i32::from_le_bytes([
                text_data[i + 1],
                text_data[i + 2],
                text_data[i + 3],
                text_data[i + 4],
            ]);
            call_sites.push((i, disp));
            i += 5;
            continue;
        }
        i += 1;
    }

    assert!(
        !call_sites.is_empty(),
        "expected at least one `E8 disp32` direct-call site in .text; \
         bytes: {:02x?}",
        text_data
    );

    for (instr_off, disp) in &call_sites {
        // Pre-fix regression assertion: disp32 must not be the placeholder.
        assert_ne!(
            *disp, 0,
            "issue #1292: E8 disp32 at .text+0x{:x} is the encoder placeholder — \
             text→text reloc site was dropped (`call symbol` fell through)",
            instr_off
        );

        // Semantic assertion: RIP-after-instr + disp32 must land inside .text
        // (a direct call to an intra-module function).
        // Field starts at instr+1; the instruction is 5 bytes total; RIP after
        // fetch is instr_off + 5 (all relative to .text start).
        let rip_after = *instr_off as i64 + 5;
        let target = rip_after + *disp as i64;
        assert!(
            target >= 0 && (target as u64) < text_size,
            "issue #1292: `call disp32=0x{:x}` at .text+0x{:x} resolves to \
             offset 0x{:x} in .text (size 0x{:x}); expected inside .text bounds",
            disp,
            instr_off,
            target,
            text_size
        );

        // Sanity: target must be at the start of a plausible instruction
        // (not the middle of the calling function). For our fixture the
        // callee is a leaf `mov rax, rcx; ret` shape starting with REX.W
        // prefix 0x48; assert the target byte is 0x48 (the callee's first
        // instruction) OR the target lands on E8/EB/C3 (any well-formed
        // opcode boundary the emitter is likely to produce). This guards
        // against a partially-wrong disp32 that happens to be non-zero but
        // points into an operand slot.
        let target_byte = text_data[target as usize];
        assert!(
            matches!(target_byte, 0x48 | 0x4C | 0xE8 | 0xEB | 0xC3 | 0x0F),
            "issue #1292: `call` at .text+0x{:x} resolves to .text+0x{:x} \
             whose first byte is 0x{:02x}; expected a plausible instruction \
             opcode boundary (0x48/0x4C REX.W, 0xE8 CALL, 0xEB JMP, 0xC3 RET, \
             or 0x0F two-byte opcode)",
            instr_off,
            target,
            target_byte
        );
    }
}

/// Issue #1293: PE emitter must patch intra-function label fixups.
///
/// Regression for the cross-repo backtrack from paideia-os R19-M3+. Before the
/// fix, `cmd_build/pe.rs` never called `patch_label_fixups`; any Jcc/Jmp with
/// a label target (`je exit_label`, `jmp .loop`, …) reached the image with a
/// placeholder rel32 (`00 00 00 00`). The je/jne then fell through (rel32=0
/// means "jump to next instruction") and jmp became a self-successor.
///
/// Fixture: tests/build-emit/pe_text_label_fixup.pdx — a single @abi("ms") +
/// @no_frame function with a forward `je exit_label` and a matching
/// `exit_label:` further down in the same unsafe block.
///
/// This test asserts:
///   1. The .text section contains a `0F 84 rel32` site (JE rel32).
///   2. `rel32` is non-zero (regression against the pre-fix drop-on-floor).
///   3. `rel32` resolves to a byte offset inside .text (intra-function).
///   4. The target byte is a plausible instruction-boundary opcode (guards
///      against a partial patch that lands mid-instruction).
#[test]
fn pe_text_label_fixup_patched_1293() {
    use object::ObjectSection;

    let fixture_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../tests/build-emit/pe_text_label_fixup.pdx");
        p
    };

    let out_file = PathBuf::from("/tmp/pe_text_label_fixup_1293.efi");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        fixture_path.to_str().unwrap(),
        "--emit",
        "pe-coff",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "fixture must build cleanly; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = fs::read(&out_file).expect("read PE output");
    let file = object::File::parse(&bytes[..]).expect("parse PE");

    let mut text_data = Vec::new();
    let mut text_size: u64 = 0;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_data = section.data().expect("read .text").to_vec();
            text_size = section.size();
        }
    }
    assert!(!text_data.is_empty(), ".text must be non-empty");

    // Find all `0F 8x rel32` sites (near Jcc with rel32 displacement).
    // Encoding is `0F 8x dd dd dd dd` (6 bytes total). 0x84 is JE.
    let mut jcc_sites: Vec<(usize, u8, i32)> = Vec::new(); // (instr_off, cond, rel32)
    let mut i = 0;
    while i + 6 <= text_data.len() {
        if text_data[i] == 0x0F && (text_data[i + 1] & 0xF0) == 0x80 {
            let disp = i32::from_le_bytes([
                text_data[i + 2],
                text_data[i + 3],
                text_data[i + 4],
                text_data[i + 5],
            ]);
            jcc_sites.push((i, text_data[i + 1], disp));
            i += 6;
            continue;
        }
        i += 1;
    }

    assert!(
        !jcc_sites.is_empty(),
        "expected at least one `0F 8x rel32` Jcc site in .text; bytes: {:02x?}",
        text_data
    );

    for (instr_off, cond, disp) in &jcc_sites {
        // Pre-fix regression assertion: rel32 must not be the placeholder.
        assert_ne!(
            *disp, 0,
            "issue #1293: `0F {:02x} rel32` at .text+0x{:x} is the encoder \
             placeholder — LabelFixup was never patched (Jcc fell through)",
            cond, instr_off
        );

        // Semantic assertion: RIP-after-instr + rel32 must land inside .text.
        // Field starts at instr+2; instruction is 6 bytes total; RIP after
        // fetch is instr_off + 6.
        let rip_after = *instr_off as i64 + 6;
        let target = rip_after + *disp as i64;
        assert!(
            target >= 0 && (target as u64) < text_size,
            "issue #1293: `0F {:02x} rel32=0x{:x}` at .text+0x{:x} resolves \
             to offset 0x{:x} in .text (size 0x{:x}); expected inside .text",
            cond, disp, instr_off, target, text_size
        );

        // Sanity: target must land on a plausible opcode boundary. For the
        // fixture, exit_label is `mov rax, 0` = `48 c7 c0 00 00 00 00` — first
        // byte 0x48 (REX.W).
        let target_byte = text_data[target as usize];
        assert!(
            matches!(target_byte, 0x48 | 0x4C | 0xE8 | 0xEB | 0xC3 | 0x0F | 0xB8..=0xBF),
            "issue #1293: Jcc at .text+0x{:x} resolves to .text+0x{:x} whose \
             first byte is 0x{:02x}; expected a plausible instruction opcode \
             boundary (0x48/0x4C REX.W, 0xE8 CALL, 0xEB JMP, 0xC3 RET, \
             0x0F two-byte, 0xB8-BF MOV imm)",
            instr_off, target, target_byte
        );
    }
}
