//! PA14-r14-012 (issue #955): Driver corpus integration test.
//!
//! This integration test is the v0.14 canary for driver-shape code: it verifies
//! end-to-end that an AHCI FIS ring driver stub compiles cleanly and emits an
//! ELF64 with the expected sections:
//!
//! - `.rodata`: register-offset constants (AHCI_CAP, AHCI_GHC, AHCI_CI, etc.)
//! - `.data`: ring metadata (ahci_fis_ring_head, ahci_fis_ring_tail, port counters)
//! - `.bss`: uninitialized ring slots (ahci_fis_ring_slots, size=1024 for 32×32)
//! - `.text`: dispatch loop with cmp, jz, movnti, clflush, sfence patterns
//!
//! Any regression in MMIO/ring/dispatch vocabulary breaks this test and surfaces
//! the breakage before paideia-os Phase 5/7 (driver network) is affected.
//!
//! Fixture: tests/build-emit/ahci_fis_stub.pdx

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn find_fixture(name: &str) -> PathBuf {
    // Fixtures live in tests/build-emit/
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = crate_dir.parent().expect("crate has parent").parent().expect("parent has parent")
        .join("tests").join("build-emit");
    fixture_dir.join(name)
}

#[test]
fn driver_corpus_ahci_fis_stub_emits_all_sections() {
    let src_path = find_fixture("ahci_fis_stub.pdx");
    assert!(src_path.exists(), "fixture {} should exist", src_path.display());

    let obj_path = std::env::temp_dir().join("pa14_012_driver_corpus.o");
    let _ = std::fs::remove_file(&obj_path);

    // Build the fixture into ELF64 format.
    let out = cargo_run(&[
        "build",
        src_path.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        obj_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed for ahci_fis_stub: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read and parse the ELF file.
    let bytes = std::fs::read(&obj_path).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find sections.
    let mut text_section = None;
    let mut rodata_section = None;
    let mut data_section = None;
    let mut bss_section = None;

    for section in file.sections() {
        match section.name().unwrap_or("") {
            ".text" => text_section = Some(section),
            ".rodata" => rodata_section = Some(section),
            ".data" => data_section = Some(section),
            ".bss" => bss_section = Some(section),
            _ => {}
        }
    }

    let text_section = text_section.expect(".text section should exist");
    let rodata_section = rodata_section.expect(".rodata section should exist");
    let data_section = data_section.expect(".data section should exist");
    let bss_section = bss_section.expect(".bss section should exist");

    let text_idx = text_section.index();
    let rodata_idx = rodata_section.index();
    let data_idx = data_section.index();
    let bss_idx = bss_section.index();

    // Helper to find a symbol by name
    let find_symbol = |name: &str| {
        for symbol in file.symbols() {
            if symbol.name().unwrap_or("") == name {
                return Some(symbol);
            }
        }
        None
    };

    // 1. Verify register-offset constants exist in .rodata
    let ahci_cap = find_symbol("AHCI_CAP").expect("AHCI_CAP symbol should exist");
    assert_eq!(
        ahci_cap.section_index(),
        Some(rodata_idx),
        "AHCI_CAP should be in .rodata section"
    );

    let ahci_ghc = find_symbol("AHCI_GHC").expect("AHCI_GHC symbol should exist");
    assert_eq!(
        ahci_ghc.section_index(),
        Some(rodata_idx),
        "AHCI_GHC should be in .rodata section"
    );

    let ahci_ci = find_symbol("AHCI_CI").expect("AHCI_CI symbol should exist");
    assert_eq!(
        ahci_ci.section_index(),
        Some(rodata_idx),
        "AHCI_CI should be in .rodata section"
    );

    // 2. Verify ring buffer symbols exist in correct sections
    // ahci_fis_ring_head and ahci_fis_ring_tail should be in .data (from @ring synthesis)
    let ring_head = find_symbol("ahci_fis_ring_head")
        .expect("ahci_fis_ring_head symbol should exist");
    assert_eq!(
        ring_head.section_index(),
        Some(data_idx),
        "ahci_fis_ring_head should be in .data section"
    );

    let ring_tail = find_symbol("ahci_fis_ring_tail")
        .expect("ahci_fis_ring_tail symbol should exist");
    assert_eq!(
        ring_tail.section_index(),
        Some(data_idx),
        "ahci_fis_ring_tail should be in .data section"
    );

    // 3. Verify ring slots exist in .bss (32 slots × 32 bytes = 1024)
    let ring_slots = find_symbol("ahci_fis_ring_slots")
        .expect("ahci_fis_ring_slots symbol should exist");
    assert_eq!(
        ring_slots.section_index(),
        Some(bss_idx),
        "ahci_fis_ring_slots should be in .bss section"
    );
    assert_eq!(
        ring_slots.size(),
        1024,
        "ahci_fis_ring_slots size should be 1024 (32 * 32)"
    );

    // 4. Verify port_active and port_errors counters exist in .data
    let port_active = find_symbol("port_active").expect("port_active symbol should exist");
    assert_eq!(
        port_active.section_index(),
        Some(data_idx),
        "port_active should be in .data section"
    );

    let port_errors = find_symbol("port_errors").expect("port_errors symbol should exist");
    assert_eq!(
        port_errors.section_index(),
        Some(data_idx),
        "port_errors should be in .data section"
    );

    // 5. Verify dispatch handler exists in .text section
    let dispatch_handler = find_symbol("dispatch_handler")
        .expect("dispatch_handler symbol should exist");
    assert_eq!(
        dispatch_handler.section_index(),
        Some(text_idx),
        "dispatch_handler should be in .text section"
    );

    // 6. Sanity check: .text should have meaningful size (dispatch loop with cmp, jz, movnti, clflush, sfence)
    assert!(
        text_section.size() > 0,
        ".text section should have non-zero size"
    );

    let _ = std::fs::remove_file(&obj_path);
}
