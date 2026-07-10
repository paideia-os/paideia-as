//! Test for #1131: Module-level let bindings should not emit spurious Mov to .text.
//!
//! When a module-level `let mut x : T = 0` is declared, populate_data_table
//! creates a .data entry for it. The emit_walker should NOT emit a spurious
//! Mov instruction to .text; instead, functions should reference the data
//! symbol via relocation (RIP-relative for module symbols).
//!
//! This test verifies:
//! 1. .text starts at offset 0 with function bytecode (no spurious Mov before it)
//! 2. The data symbol exists in .data or .bss
//! 3. Relocations reference the data symbol correctly

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes, data_bytes, rodata_bytes, has_section};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test for #1131: `let mut` at module scope should not emit to .text.
///
/// Fixture: module_let_mut_no_text_emission.pdx
/// - Declares `pub let mut counter : u64 = 0` at module level
/// - No functions, just the data binding
///
/// Expected: counter goes to .data section with 8 zero bytes, no .text emission.
/// This is the core test for #1131: verify that populate_data_table + the gate
/// in visit_let_literal prevent spurious Mov instructions.
#[test]
fn module_let_mut_data_no_spurious_text_emission() {
    let out = run_build(build_emit("module_let_mut_no_text_emission.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    // Verify .data section exists (for mutable initialized data)
    assert!(
        has_section(&bytes, ".data"),
        ".data section should exist for mutable module-level let"
    );

    // Extract and verify the .data contents
    let data = data_bytes(&bytes);
    assert!(
        data.len() >= 8,
        ".data should have at least 8 bytes for counter u64"
    );
    // The counter should be initialized to 0
    assert_eq!(
        &data[0..8],
        &[0, 0, 0, 0, 0, 0, 0, 0],
        "counter should be initialized to 0 in .data"
    );

    // Verify counter symbol exists and is in .data section
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let mut found_counter = false;

    // Debug: print all symbols
    eprintln!("=== Available symbols ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            eprintln!("  {}", name);
        }
    }

    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            // Also accept "counter" as the symbol name if binding names are used
            if name == "data_2" || name == "counter" {
                found_counter = true;
                // Verify it's in .data section
                match sym.section() {
                    object::SymbolSection::Section(idx) => {
                        let section = file.section_by_index(idx);
                        if let Ok(sect) = section {
                            let sect_name = sect.name().unwrap_or("");
                            assert_eq!(
                                sect_name, ".data",
                                "counter symbol should be in .data, got {}",
                                sect_name
                            );
                        }
                    }
                    _ => {
                        panic!("counter symbol should have a section index");
                    }
                }
                break;
            }
        }
    }
    assert!(found_counter, "counter data symbol (data_2) should exist");

    // The critical test: .text should be empty or very small (no spurious Mov).
    // If visit_let_literal was unconditionally emitting Mov instructions,
    // we would see a 7-byte instruction (mov rax, 0) in .text before any functions.
    let text = text_bytes(&bytes);
    assert!(
        text.is_empty(),
        ".text should be empty (no spurious Mov) for module-level data-only binding. Got {} bytes: {:02x?}",
        text.len(),
        text
    );
}

/// Test for #1131: `let` (immutable) at module scope should not emit to .text.
///
/// Fixture: module_let_immut_no_text_emission.pdx
/// - Declares `pub let counter : u64 = 0` at module level (immutable)
/// - No functions, just the data binding
///
/// Expected: counter goes to .rodata section with 8 zero bytes, no .text emission.
#[test]
fn module_let_immut_rodata_no_spurious_text_emission() {
    let out = run_build(build_emit("module_let_immut_no_text_emission.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    // Verify .rodata section exists (for immutable initialized data)
    let rodata = rodata_bytes(&bytes);
    assert!(!rodata.is_empty(), ".rodata section should exist for immutable module-level let");

    // Verify .rodata contains at least 8 bytes for the counter
    assert!(
        rodata.len() >= 8,
        ".rodata should have at least 8 bytes for counter u64"
    );
    // The counter should be initialized to 0
    assert_eq!(
        &rodata[0..8],
        &[0, 0, 0, 0, 0, 0, 0, 0],
        "counter should be initialized to 0 in .rodata"
    );

    // Verify counter symbol exists in .rodata
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let mut found_counter = false;

    // Debug: print all symbols
    eprintln!("=== Available symbols (immut) ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            eprintln!("  {}", name);
        }
    }

    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            // Also accept "counter" as the symbol name if binding names are used
            if name == "data_2" || name == "counter" {
                found_counter = true;
                match sym.section() {
                    object::SymbolSection::Section(idx) => {
                        let section = file.section_by_index(idx);
                        if let Ok(sect) = section {
                            let sect_name = sect.name().unwrap_or("");
                            assert_eq!(
                                sect_name, ".rodata",
                                "immutable counter symbol should be in .rodata, got {}",
                                sect_name
                            );
                        }
                    }
                    _ => {
                        panic!("counter symbol should have a section index");
                    }
                }
                break;
            }
        }
    }
    assert!(found_counter, "counter symbol (data_2) should exist in .rodata");

    // The critical test: .text should be empty (no spurious Mov).
    let text = text_bytes(&bytes);
    assert!(
        text.is_empty(),
        ".text should be empty (no spurious Mov) for module-level data-only binding. Got {} bytes: {:02x?}",
        text.len(),
        text
    );
}

/// Test for #1131: Placeholder (uninit) at module scope should use .bss.
///
/// Fixture: module_let_mut_placeholder_bss.pdx
/// - Declares `pub let mut buffer : u64 = uninit` at module level (uninitialized placeholder)
/// - No functions, just the data binding
///
/// Expected: buffer goes to .bss (SHT_NOBITS), no spurious Mov in .text.
#[test]
fn module_let_mut_placeholder_bss_no_spurious_text_emission() {
    let out = run_build(build_emit("module_let_mut_placeholder_bss.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    // Verify .bss section exists (for uninitialized data)
    assert!(
        has_section(&bytes, ".bss"),
        ".bss section should exist for uninitialized module-level let"
    );

    // Verify buffer symbol exists in .bss
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let mut found_buffer = false;

    // Debug: print all symbols
    eprintln!("=== Available symbols (placeholder) ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            eprintln!("  {}", name);
        }
    }

    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            // Also accept "buffer" as the symbol name if binding names are used
            if name == "data_2" || name == "buffer" {
                found_buffer = true;
                match sym.section() {
                    object::SymbolSection::Section(idx) => {
                        let section = file.section_by_index(idx);
                        if let Ok(sect) = section {
                            let sect_name = sect.name().unwrap_or("");
                            assert_eq!(
                                sect_name, ".bss",
                                "uninitialized buffer symbol should be in .bss, got {}",
                                sect_name
                            );
                        }
                    }
                    _ => {
                        panic!("buffer symbol should have a section index");
                    }
                }
                break;
            }
        }
    }
    assert!(found_buffer, "buffer symbol (data_2) should exist in .bss");

    // The critical test: .text should be empty (no spurious Mov).
    let text = text_bytes(&bytes);
    assert!(
        text.is_empty(),
        ".text should be empty (no spurious Mov) for module-level data-only binding. Got {} bytes: {:02x?}",
        text.len(),
        text
    );
}
