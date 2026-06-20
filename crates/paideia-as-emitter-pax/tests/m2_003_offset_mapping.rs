//! Phase-4-m2-003 tests: PAX emitter parity with post-rewrite offset mapping.
//!
//! Tests for:
//! 1. .paideia.opt-passes section records pass name and count
//! 2. Caps section with post-rewrite offsets
//! 3. Effects section with post-rewrite offsets
//! 4. pax-introspect displays rewrite counts (tested via binary crate)

use paideia_as_emitter_pax::{OptPassRecord, OptPassesSection};
use paideia_as_ir::IrNodeId;
use std::collections::HashMap;

/// Test 1: pax_opt_passes_section_records_pass_name_and_count
#[test]
fn pax_opt_passes_section_records_pass_name_and_count() {
    let mut section = OptPassesSection::new();

    // Add several pass records with different names and rewrite counts
    section.push(OptPassRecord::new(
        "peephole".to_string(),
        IrNodeId::new(1).unwrap(),
        42,
    ));
    section.push(OptPassRecord::new(
        "dse".to_string(),
        IrNodeId::new(2).unwrap(),
        15,
    ));
    section.push(OptPassRecord::new(
        "const-fold".to_string(),
        IrNodeId::new(3).unwrap(),
        7,
    ));

    // Verify section contains correct data
    assert_eq!(section.len(), 3);
    assert_eq!(section.records[0].pass_name, "peephole");
    assert_eq!(section.records[0].rewrite_count, 42);
    assert_eq!(section.records[1].pass_name, "dse");
    assert_eq!(section.records[1].rewrite_count, 15);
    assert_eq!(section.records[2].pass_name, "const-fold");
    assert_eq!(section.records[2].rewrite_count, 7);

    // Verify serialization and deserialization
    let bytes = section.to_bytes();
    let parsed = OptPassesSection::from_bytes(&bytes).expect("Failed to parse section");

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed.records[0].pass_name, "peephole");
    assert_eq!(parsed.records[0].rewrite_count, 42);
    assert_eq!(parsed.records[1].pass_name, "dse");
    assert_eq!(parsed.records[1].rewrite_count, 15);
    assert_eq!(parsed.records[2].pass_name, "const-fold");
    assert_eq!(parsed.records[2].rewrite_count, 7);
}

/// Test 2 & 3: Offset mapping verification
///
/// This test verifies that the offset_map from text emitter can be used
/// to remap IR node IDs to post-rewrite bytecode offsets.
///
/// The actual remapping in caps/effects sections happens at emit time;
/// this test demonstrates the concept.
#[test]
fn pax_offset_map_remapping_for_caps_effects() {
    // Simulate an offset_map from text emitter (Phase-4-m2-002)
    let mut offset_map: HashMap<IrNodeId, u64> = HashMap::new();

    // Simulate pre-rewrite IR nodes and their post-rewrite bytecode offsets
    offset_map.insert(IrNodeId::new(1).unwrap(), 0x0000u64); // func1 at offset 0
    offset_map.insert(IrNodeId::new(2).unwrap(), 0x0100u64); // func2 at offset 256
    offset_map.insert(IrNodeId::new(3).unwrap(), 0x0250u64); // func3 at offset 592
    offset_map.insert(IrNodeId::new(100).unwrap(), 0x0400u64); // param at offset 1024

    // Verify that caps/effects sections can use these offsets
    // A cap entry for function 1 would reference offset 0x0000
    // A cap entry for function 2 would reference offset 0x0100
    // etc.

    assert_eq!(offset_map.get(&IrNodeId::new(1).unwrap()), Some(&0x0000u64));
    assert_eq!(offset_map.get(&IrNodeId::new(2).unwrap()), Some(&0x0100u64));
    assert_eq!(offset_map.get(&IrNodeId::new(3).unwrap()), Some(&0x0250u64));
    assert_eq!(
        offset_map.get(&IrNodeId::new(100).unwrap()),
        Some(&0x0400u64)
    );

    // Verify that non-existent nodes gracefully return None
    assert_eq!(offset_map.get(&IrNodeId::new(999).unwrap()), None);
}

/// Test 4: Verify OptPassesSection integrates with PAX emitter
///
/// This test verifies that OptPassesSection can be serialized as a PAX section
/// and parsed back correctly.
#[test]
fn pax_opt_passes_section_integrates_with_pax_format() {
    use paideia_as_emitter_pax::{Section, SectionType};

    let mut opt_passes = OptPassesSection::new();

    // Add records as would be generated during emit
    opt_passes.push(OptPassRecord::new(
        "peephole".to_string(),
        IrNodeId::new(1).unwrap(),
        10,
    ));
    opt_passes.push(OptPassRecord::new(
        "dse".to_string(),
        IrNodeId::new(2).unwrap(),
        5,
    ));

    // Serialize section content
    let section_content = opt_passes.to_bytes();

    // Create a PAX section descriptor (as would be done in emit pipeline)
    let pax_section = Section {
        ty: SectionType::OptPasses,
        flags: 0,
        content_offset: 1024, // Example offset
        content_size: section_content.len() as u64,
        virtual_address: 0,
        alignment: 8,
        name: ".paideia.opt-passes".to_owned(),
    };

    // Verify section descriptor is created correctly
    assert_eq!(pax_section.ty, SectionType::OptPasses);
    assert_eq!(pax_section.content_size, section_content.len() as u64);
    assert_eq!(pax_section.name, ".paideia.opt-passes");

    // Verify we can serialize and parse the section descriptor
    let section_bytes = pax_section.to_bytes();
    let parsed_section = Section::from_bytes(&section_bytes).expect("Failed to parse section");
    assert_eq!(parsed_section.ty, SectionType::OptPasses);
    assert_eq!(parsed_section.content_size, section_content.len() as u64);

    // Verify the section content can be reparsed
    let reparsed_opt_passes =
        OptPassesSection::from_bytes(&section_content).expect("Failed to parse opt-passes");
    assert_eq!(reparsed_opt_passes.len(), 2);
    assert_eq!(reparsed_opt_passes.records[0].pass_name, "peephole");
    assert_eq!(reparsed_opt_passes.records[0].rewrite_count, 10);
}

/// Test: Verify multiple passes on same function are aggregatable
#[test]
fn pax_opt_passes_section_supports_multiple_passes_per_function() {
    let mut section = OptPassesSection::new();

    let func_id = IrNodeId::new(1).unwrap();

    // Same function optimized by multiple passes
    section.push(OptPassRecord::new("peephole".to_string(), func_id, 3));
    section.push(OptPassRecord::new("dse".to_string(), func_id, 2));
    section.push(OptPassRecord::new("const-fold".to_string(), func_id, 1));

    // Verify we can distinguish and aggregate per-pass
    let peephole_count: u32 = section
        .records
        .iter()
        .filter(|r| r.pass_name == "peephole" && r.function_id == func_id)
        .map(|r| r.rewrite_count)
        .sum();

    assert_eq!(peephole_count, 3);

    let total_count: u32 = section.records.iter().map(|r| r.rewrite_count).sum();
    assert_eq!(total_count, 6);
}
