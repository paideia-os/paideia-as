//! UEFI loader smoke tests.
//!
//! Phase-2-m6-008: environment check + structural PE build are ACTIVE.
//! Boot test is #[ignore]'d until m6-009+ ships a meaningful hello.efi.

use paideia_uefi_smoke::{UefiEnv, build_hello_efi};

#[test]
fn env_check_describes_availability() {
    match UefiEnv::probe() {
        Some(_) => println!("OVMF + QEMU present; boot smoke test will run."),
        None => println!("OVMF or QEMU absent; boot smoke test will be skipped."),
    }
    // This test always passes. It's diagnostic, not gating.
}

#[test]
fn hello_efi_builds_structurally_valid_pe() {
    let tmp = std::env::temp_dir().join("paideia-uefi-smoke");
    std::fs::create_dir_all(&tmp).unwrap();
    let efi = tmp.join("hello.efi");
    build_hello_efi(&efi);
    let bytes = std::fs::read(&efi).unwrap();
    assert!(bytes.len() >= 1024, "EFI file should be at least 1 KB");
    assert_eq!(&bytes[0..2], b"MZ", "EFI should start with MZ magic");
    assert_eq!(
        &bytes[64..68],
        b"PE\0\0",
        "PE signature should be at offset 64"
    );
}

#[test]
#[ignore = "boot smoke gated on OVMF + QEMU + a meaningful hello.efi (m6-009+ ships the latter)"]
fn boot_and_print_under_ovmf() {
    let Some(env) = UefiEnv::probe() else {
        eprintln!("Skipped: probe() returned None.");
        return;
    };
    let tmp = std::env::temp_dir().join("paideia-uefi-smoke-boot");
    std::fs::create_dir_all(&tmp).unwrap();
    let efi = tmp.join("hello.efi");
    build_hello_efi(&efi);
    let output = paideia_uefi_smoke::boot_and_capture_serial(&env, &efi).expect("qemu spawn");
    // Phase-2-m6-008 expectation: at minimum, QEMU spawned and the
    // VM made progress past the OVMF screen. Real "hello" assertion
    // happens once a non-empty .efi ships.
    assert!(!output.is_empty(), "expected some serial output from OVMF");
}
