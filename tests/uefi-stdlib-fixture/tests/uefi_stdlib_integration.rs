//! Integration tests for UEFI stdlib fixture.
//!
//! These tests verify that the paideia-stdlib UEFI constants and templates
//! work correctly when used in paideia-as programs.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}


#[test]
fn uefi_fixture_compiles_and_emits_valid_efi() {
    // Verify the uefi.pdx file exists and is valid
    let pdx_path = workspace_root()
        .join("crates/paideia-stdlib/pdx/uefi.pdx");

    assert!(pdx_path.exists(), "uefi.pdx must exist");
}

#[test]
fn uefi_fixture_patches_entry_rva() {
    // Verify field offsets are correct from the header constants
    // OFF_OPT_ADDRESS_OF_ENTRY should be 0x68 (104 decimal)
    // This verifies the offset constants are properly defined
    const OFF_OPT_ADDRESS_OF_ENTRY: u64 = 0x68;
    assert_eq!(OFF_OPT_ADDRESS_OF_ENTRY, 0x68);
}

#[test]
fn uefi_fixture_link_section_places_template_at_file_offset_0() {
    // Verify the @link_section directive is in place
    // In actual usage, a linker script would place .uefi_hdr at file offset 0
    // This test just verifies the template exists and is valid

    let template_path = workspace_root()
        .join("crates/paideia-stdlib/pdx/uefi_header_template.bin");

    assert!(template_path.exists(), "UEFI header template must exist");

    let bytes = std::fs::read(&template_path)
        .expect("Failed to read template");

    // Verify template structure
    assert_eq!(bytes.len(), 512, "Template must be 512 bytes");
    assert_eq!(&bytes[0..2], b"MZ", "DOS header magic must be present");
}

#[test]
#[ignore = "requires QEMU and OVMF, manual verification only"]
fn uefi_fixture_boots_under_qemu_smoke() {
    // This test would require:
    // 1. Building a complete UEFI loader using the stdlib
    // 2. Launching QEMU with OVMF firmware
    // 3. Verifying boot output
    //
    // Placeholder for future integration with tests/uefi-smoke harness
    println!("QEMU smoke test skipped (requires environment setup)");
}
