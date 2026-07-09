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
