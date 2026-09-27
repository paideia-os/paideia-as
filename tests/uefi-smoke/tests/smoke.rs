//! UEFI loader smoke tests.
//!
//! Phase-2-m6-008 shipped the structural PE build (`build_hello_efi`).
//! Phase-2-m6-009 / pa-r19-013 (issue #1018) shipped the paideia-as
//! end-to-end compile of `tests/uefi-smoke/fixtures/hello.pdx` into a
//! bootable PE32+ (`build_hello_efi_via_paideia_as`).
//!
//! PAS-DEBT-B7-006 (paideia-as#1534) reactivated both boot smokes: they
//! run when all runtime deps are present (QEMU + OVMF + mkfs.vfat +
//! mcopy) and skip cleanly with a diagnostic `eprintln!` otherwise.
//! `UefiEnv::probe()` handles the QEMU+OVMF skip; the mkfs.vfat/mcopy
//! skip is enforced upfront by `require_tool` in each test to avoid a
//! late panic inside `boot_and_capture_serial`.
//!
//! The current assertion (`!output.is_empty()`) is satisfied by any
//! OVMF boot that reaches serial-out — even the 2-arg identity `.efi`
//! shipped by pa-r19-013 suffices, because OVMF prints its own boot
//! banner before dispatching the image. Tightening the assertion to
//! "hello.efi actually printed `Hello`" waits on a boot-services `Print`
//! primitive (unfiled; see design/toolchain/bootstrap.md §m6-010+).

use paideia_uefi_smoke::{UefiEnv, build_hello_efi, build_hello_efi_via_paideia_as};
use std::path::PathBuf;
use std::process::Command;

/// Return `true` iff `tool` responds to `--version` on `PATH`.
///
/// Mirrors the shape used by
/// `crates/paideia-as/tests/build_emit/pa10_007_data_symbol_names.rs`
/// (`require_tool`) so the skip pattern is uniform across the tree.
fn require_tool(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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
fn boot_and_print_under_ovmf() {
    // PAS-DEBT-B7-006 (#1534): un-ignored. Uses the structural-PE
    // `build_hello_efi` (m6-008). Skips cleanly when any runtime dep is
    // missing (QEMU, OVMF, mkfs.vfat, mcopy).
    let Some(env) = UefiEnv::probe() else {
        eprintln!("skipping boot_and_print_under_ovmf: OVMF or QEMU absent");
        return;
    };
    if !require_tool("mkfs.vfat") {
        eprintln!("skipping boot_and_print_under_ovmf: mkfs.vfat not in $PATH");
        return;
    }
    if !require_tool("mcopy") {
        eprintln!("skipping boot_and_print_under_ovmf: mcopy not in $PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("paideia-uefi-smoke-boot");
    std::fs::create_dir_all(&tmp).unwrap();
    let efi = tmp.join("hello.efi");
    build_hello_efi(&efi);
    let output = paideia_uefi_smoke::boot_and_capture_serial(&env, &efi).expect("qemu spawn");
    // Assertion floor: OVMF reached serial-out. Tightening to
    // "hello.efi printed Hello" waits on a Boot-Services `Print`
    // primitive (see design/toolchain/bootstrap.md §m6-010+).
    assert!(!output.is_empty(), "expected some serial output from OVMF");
}

#[test]
fn boot_and_print_paideia_compiled() {
    // PAS-DEBT-B7-006 (#1534): un-ignored. pa-r19-013 (#1018) shipped
    // the paideia-as → PE/COFF pipeline used by
    // `build_hello_efi_via_paideia_as`. Same runtime-dep skip pattern
    // as `boot_and_print_under_ovmf`.
    let Some(env) = UefiEnv::probe() else {
        eprintln!("skipping boot_and_print_paideia_compiled: OVMF or QEMU absent");
        return;
    };
    if !require_tool("mkfs.vfat") {
        eprintln!("skipping boot_and_print_paideia_compiled: mkfs.vfat not in $PATH");
        return;
    }
    if !require_tool("mcopy") {
        eprintln!("skipping boot_and_print_paideia_compiled: mcopy not in $PATH");
        return;
    }

    let tmp = std::env::temp_dir().join("paideia-uefi-smoke-pa-compiled");
    std::fs::create_dir_all(&tmp).unwrap();

    let fixture_path = PathBuf::from("tests/uefi-smoke/fixtures/hello.pdx");
    let efi = tmp.join("hello_paideia.efi");

    // Compile via paideia-as
    build_hello_efi_via_paideia_as(&fixture_path, &efi).expect("paideia-as build failed");

    // Boot and capture output
    let output = paideia_uefi_smoke::boot_and_capture_serial(&env, &efi).expect("qemu spawn");

    // Expect QEMU to have produced some output (see note above).
    assert!(!output.is_empty(), "expected some serial output from OVMF");
}
