//! #1134 Slice 2: Integration test for scanner 2 (cmd_build inline scanner) routing parity.
//!
//! This test verifies that the cmd_build pipeline's inline data scanner (Scanner 2)
//! correctly routes module-level bindings to the appropriate section (Rodata, Data, or Bss)
//! based on their RHS kind and mutability.
//!
//! Key test: The divergence between Scanner 1 (populate_data_table, unit test)
//! and Scanner 2 (cmd_build inline scanner, integration test):
//! - G_BYT_RW (InlineBytes + mut) routes to Data in Scanner 2
//! - G_BYT_RW routed to Rodata in Scanner 1 (bug fixed by this test's existence)
//!
//! This test exercises the full cmd_build pipeline and inspects the resulting ELF
//! to verify section routing at the binary level.

use object::{Object, ObjectSection, ObjectSymbol};
use std::collections::HashMap;
use super::super::common::fixture::build_emit;
use super::super::common::harness::run_build;

/// Expected routing for each binding: (binding_name, section_name, is_writable).
/// is_writable is true for .data, false for .rodata and .bss (which are read-only in the ELF sense,
/// but .bss carries SHF_WRITE semantics during runtime initialization).
const EXPECTED_ROUTING: &[(&str, &str)] = &[
    ("A_LIT_RO", ".rodata"),
    ("A_LIT_RW", ".data"),
    ("B_ARR_RO", ".rodata"),
    ("B_ARR_RW", ".data"),
    ("C_REC_RO", ".rodata"),
    ("C_REC_RW", ".data"),
    ("D_ENUM_RO", ".rodata"),
    ("D_ENUM_RW", ".data"),
    ("E_BSS_RO", ".bss"),
    ("E_BSS_RW", ".bss"),
    // F_STR_* rows skipped due to grammar constraints (see fixture TODOs)
    ("G_BYT_RO", ".rodata"),
    ("G_BYT_RW", ".data"), // Key divergence: Scanner 2 routes mut InlineBytes to Data
];

#[test]
fn scanner_routing_parity_all_bindings() {
    let input = build_emit("scanner_routing_parity.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert!(!bytes.is_empty(), "artifact should be non-empty");

    // Parse ELF and verify all expected bindings are present in correct sections.
    let file = object::File::parse(&*bytes).expect("should parse as valid ELF64");

    // Build a map of section names to Symbol vecs for quick lookup.
    let mut sections_map: HashMap<String, Vec<String>> = HashMap::new();

    eprintln!("=== Symbol to Section Routing ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name.is_empty() {
                continue; // Skip unnamed symbols
            }
            if let Some(section_index) = sym.section_index() {
                if let Ok(section) = file.section_by_index(section_index) {
                    if let Ok(section_name) = section.name() {
                        eprintln!(
                            "  {}: {} (size={}, is_definition={})",
                            name,
                            section_name,
                            sym.size(),
                            sym.is_definition()
                        );
                        sections_map
                            .entry(section_name.to_string())
                            .or_default()
                            .push(name.to_string());
                    }
                }
            }
        }
    }

    // Verify each expected binding is in the correct section.
    for (binding_name, expected_section) in EXPECTED_ROUTING {
        let found = sections_map
            .get(*expected_section)
            .map(|syms| syms.contains(&binding_name.to_string()))
            .unwrap_or(false);

        assert!(
            found,
            "Binding '{}' should be in section '{}', but was not found. Sections present: {:?}",
            binding_name,
            expected_section,
            sections_map.keys().collect::<Vec<_>>()
        );
    }

    eprintln!(
        "✓ All {} bindings routed to correct sections",
        EXPECTED_ROUTING.len()
    );
}

#[test]
fn scanner_routing_g_byt_rw_symbol_name_and_section() {
    // Key representative test: G_BYT_RW (InlineBytes + mut) should:
    // 1. Have symbol name "G_BYT_RW" (proves Scanner 2 with real names is final winner)
    // 2. Be in .data section (divergence from Scanner 1)

    let input = build_emit("scanner_routing_parity.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("should parse as valid ELF64");

    // Find G_BYT_RW symbol
    let mut g_byt_rw_sym = None;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "G_BYT_RW" {
                g_byt_rw_sym = Some(sym);
                break;
            }
        }
    }

    let sym = g_byt_rw_sym.expect("Symbol 'G_BYT_RW' must exist (proves real naming from Scanner 2)");

    // Verify it's a defined symbol (not undefined)
    assert!(
        sym.is_definition(),
        "G_BYT_RW must be a defined symbol, not undefined"
    );

    // Verify it's in .data section (the divergence from Scanner 1)
    let section_index = sym
        .section_index()
        .expect("G_BYT_RW symbol must have a section index");
    let section = file
        .section_by_index(section_index)
        .expect("G_BYT_RW's section index should resolve");
    let section_name = section.name().unwrap_or("");

    assert_eq!(
        section_name, ".data",
        "G_BYT_RW (InlineBytes + mut) must be in .data section (Scanner 2 divergence)"
    );

    eprintln!(
        "✓ G_BYT_RW symbol name and section verified: {} in {}",
        "G_BYT_RW", ".data"
    );
}

#[test]
fn scanner_routing_rodata_bindings() {
    // Verify all immutable (RO) bindings are in .rodata
    let input = build_emit("scanner_routing_parity.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("should parse as valid ELF64");

    let rodata_bindings = ["A_LIT_RO", "B_ARR_RO", "C_REC_RO", "D_ENUM_RO", "G_BYT_RO"];

    for binding_name in &rodata_bindings {
        let mut found_in_rodata = false;
        for sym in file.symbols() {
            if let Ok(name) = sym.name() {
                if name == *binding_name {
                    if let Some(section_index) = sym.section_index() {
                        if let Ok(section) = file.section_by_index(section_index) {
                            if let Ok(section_name) = section.name() {
                                if section_name == ".rodata" {
                                    found_in_rodata = true;
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
        assert!(
            found_in_rodata,
            "Binding '{}' should be in .rodata (immutable RHS)",
            binding_name
        );
    }

    eprintln!("✓ All immutable (RO) bindings verified in .rodata");
}

#[test]
fn scanner_routing_data_bindings() {
    // Verify all mutable non-placeholder bindings are in .data
    let input = build_emit("scanner_routing_parity.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("should parse as valid ELF64");

    let data_bindings = [
        "A_LIT_RW", "B_ARR_RW", "C_REC_RW", "D_ENUM_RW", "G_BYT_RW",
    ];

    for binding_name in &data_bindings {
        let mut found_in_data = false;
        for sym in file.symbols() {
            if let Ok(name) = sym.name() {
                if name == *binding_name {
                    if let Some(section_index) = sym.section_index() {
                        if let Ok(section) = file.section_by_index(section_index) {
                            if let Ok(section_name) = section.name() {
                                if section_name == ".data" {
                                    found_in_data = true;
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
        assert!(
            found_in_data,
            "Binding '{}' should be in .data (mutable non-placeholder)",
            binding_name
        );
    }

    eprintln!("✓ All mutable non-placeholder bindings verified in .data");
}

#[test]
fn scanner_routing_bss_bindings() {
    // Verify all Placeholder (uninit) bindings are in .bss
    let input = build_emit("scanner_routing_parity.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("should parse as valid ELF64");

    let bss_bindings = ["E_BSS_RO", "E_BSS_RW"];

    for binding_name in &bss_bindings {
        let mut found_in_bss = false;
        for sym in file.symbols() {
            if let Ok(name) = sym.name() {
                if name == *binding_name {
                    if let Some(section_index) = sym.section_index() {
                        if let Ok(section) = file.section_by_index(section_index) {
                            if let Ok(section_name) = section.name() {
                                if section_name == ".bss" {
                                    found_in_bss = true;
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
        assert!(
            found_in_bss,
            "Binding '{}' should be in .bss (Placeholder/uninit)",
            binding_name
        );
    }

    eprintln!("✓ All Placeholder (uninit) bindings verified in .bss");
}
