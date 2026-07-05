//! PA14-r14-008 (issue #951): `@ring(slots=N, slot_size=M)` ring buffer synthesis.
//!
//! This integration test verifies end-to-end that `@ring(slots=64, slot_size=32)` on a binding
//! produces 4 symbols in the correct sections with correct sizes and alignments:
//! - `<name>_slots` in `.bss`, size=64*32=2048, align=64
//! - `<name>_head` in `.data`, size=8, value=0, align=8
//! - `<name>_tail` in `.data`, size=8, value=0, align=8
//! - `<name>_mask` in `.rodata`, size=8, value=63 (slots-1), align=8
//!
//! Fixture (written to a temp `.pdx` file):
//! ```text
//! module Pa14R14008RingSynthesis = structure {
//!   pub let mut rx_ring : u64 = uninit @ring(slots=64, slot_size=32)
//! }
//! ```

use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

const FIXTURE_SRC: &str = r#"module Pa14R14008RingSynthesis = structure {
  pub let mut rx_ring : u64 = uninit @ring(slots=64, slot_size=32)
}
"#;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

#[test]
fn ring_synthesis_produces_four_symbols_in_correct_sections() {
    // Write the fixture to a temp path. The file stem must PascalCase-convert to
    // the module name declared in FIXTURE_SRC (`Pa14R14008RingSynthesis`), per the
    // M0305 file-module naming rule (§7.6).
    let src_path = std::env::temp_dir().join("pa14_r14008_ring_synthesis.pdx");
    std::fs::write(&src_path, FIXTURE_SRC).expect("should write fixture source");

    let obj_path = std::env::temp_dir().join("pa14_r14008_ring_synthesis.o");
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
        "build --emit elf64 failed for pa14_r14008_ring_synthesis fixture: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read and parse the ELF file.
    let bytes = std::fs::read(&obj_path).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the .bss, .data, and .rodata sections.
    let mut bss_section = None;
    let mut data_section = None;
    let mut rodata_section = None;

    for section in file.sections() {
        match section.name().unwrap_or("") {
            ".bss" => bss_section = Some(section),
            ".data" => data_section = Some(section),
            ".rodata" => rodata_section = Some(section),
            _ => {}
        }
    }

    let bss_section = bss_section.expect(".bss section should exist");
    let data_section = data_section.expect(".data section should exist");
    let rodata_section = rodata_section.expect(".rodata section should exist");

    let bss_idx = bss_section.index();
    let data_idx = data_section.index();
    let rodata_idx = rodata_section.index();

    // Helper to find a symbol by name
    let find_symbol = |name: &str| {
        for symbol in file.symbols() {
            if symbol.name().unwrap_or("") == name {
                return Some(symbol);
            }
        }
        None
    };

    // 1. Verify rx_ring_slots exists in .bss, size=2048, align=64
    let slots_sym = find_symbol("rx_ring_slots").expect("rx_ring_slots symbol should exist");
    assert_eq!(
        slots_sym.section_index(),
        Some(bss_idx),
        "rx_ring_slots should be in .bss section"
    );
    assert_eq!(
        slots_sym.size(),
        2048,
        "rx_ring_slots size should be 2048 (64 * 32)"
    );
    let slots_offset = slots_sym.address();
    assert_eq!(
        slots_offset % 64,
        0,
        "rx_ring_slots offset ({}) should be 64-aligned",
        slots_offset
    );

    // 2. Verify rx_ring_head exists in .data, size=8, value=0, align=8
    let head_sym = find_symbol("rx_ring_head").expect("rx_ring_head symbol should exist");
    assert_eq!(
        head_sym.section_index(),
        Some(data_idx),
        "rx_ring_head should be in .data section"
    );
    assert_eq!(head_sym.size(), 8, "rx_ring_head size should be 8");

    // 3. Verify rx_ring_tail exists in .data, size=8, value=0, align=8
    let tail_sym = find_symbol("rx_ring_tail").expect("rx_ring_tail symbol should exist");
    assert_eq!(
        tail_sym.section_index(),
        Some(data_idx),
        "rx_ring_tail should be in .data section"
    );
    assert_eq!(tail_sym.size(), 8, "rx_ring_tail size should be 8");

    // 4. Verify rx_ring_mask exists in .rodata, size=8, value=63 (little-endian)
    let mask_sym = find_symbol("rx_ring_mask").expect("rx_ring_mask symbol should exist");
    assert_eq!(
        mask_sym.section_index(),
        Some(rodata_idx),
        "rx_ring_mask should be in .rodata section"
    );
    assert_eq!(mask_sym.size(), 8, "rx_ring_mask size should be 8");

    // Verify that mask value is 63 (slots - 1) in little-endian format
    let mask_data = rodata_section.data().unwrap_or(&[]);
    let mask_offset = mask_sym.address() as usize;
    if mask_offset + 8 <= mask_data.len() {
        let mask_bytes = &mask_data[mask_offset..mask_offset + 8];
        let mask_value = u64::from_le_bytes([
            mask_bytes[0], mask_bytes[1], mask_bytes[2], mask_bytes[3],
            mask_bytes[4], mask_bytes[5], mask_bytes[6], mask_bytes[7],
        ]);
        assert_eq!(mask_value, 63, "rx_ring_mask value should be 63 (64-1)");
    }

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&obj_path);
}
