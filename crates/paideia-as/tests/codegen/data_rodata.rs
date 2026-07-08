//! Integration test for data section population (Phase 5 m4-003).
//!
//! Builds a simple .pdx with module-level data (GDT descriptor),
//! emits to ELF, and verifies .rodata section contains expected bytes.

use object::{Object, ObjectSection};
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::{DataEntry, DataSideTable, IrArena, IrKind, IrNodeId, SectionKind};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn data_side_table_can_emit_to_rodata() {
    // Create a simple data side-table with a 16-byte GDT descriptor.
    let mut data_table = DataSideTable::new();

    // GDT descriptor: 16 bytes representing a segment descriptor
    // Format: 8 bytes + 8 bytes (two 64-bit values)
    let gdt_bytes = vec![
        0xFF, 0x00, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];

    assert_eq!(gdt_bytes.len(), 16, "GDT descriptor should be 16 bytes");

    let entry = DataEntry::new_rodata(gdt_bytes.clone(), "gdt_descriptor".to_string(), 8);
    data_table.insert(IrNodeId::new(1).unwrap(), entry);

    // Verify the entry can be retrieved.
    let retrieved = data_table.get(IrNodeId::new(1).unwrap()).unwrap();
    assert_eq!(retrieved.section, SectionKind::Rodata);
    assert_eq!(retrieved.bytes, gdt_bytes);
    assert_eq!(retrieved.symbol_name, "gdt_descriptor");
    assert_eq!(retrieved.align, 8);
}

#[test]
fn emit_walker_populate_data_table_with_gdt() {
    use paideia_as_elaborator::EmitWalker;

    let mut arena = IrArena::new();

    // Build a simple IR: Let with a Literal value representing the GDT descriptor.
    // For simplicity, we'll use two consecutive u64 values that form the 16 bytes.
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);

    // Register a u64 literal value (first 8 bytes of the GDT).
    arena
        .literal_values_mut()
        .insert(lit_id, 0x00CF_9200_0000_00FFu64 as i64);

    // Populate the data table.
    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    // Verify the entry was created and contains the correct bytes.
    let entry = data_table.get(let_id).expect("data entry should exist");
    assert_eq!(entry.section, SectionKind::Rodata);
    assert_eq!(entry.bytes.len(), 8);
    // Verify little-endian packing.
    assert_eq!(entry.bytes[0], 0xFF); // LSB
    assert_eq!(entry.bytes[7], 0x00); // MSB
}

#[test]
fn elf_writer_can_emit_rodata_bytes() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind};

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Create a 16-byte GDT descriptor.
    let gdt_bytes = vec![
        0xFF, 0x00, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];

    // Add the bytes to .rodata with 8-byte alignment.
    let offset = writer.add_rodata_bytes(&gdt_bytes, 8);
    assert_eq!(
        offset, 0,
        "first append to .rodata should start at offset 0"
    );

    // Finalize the ELF and verify the bytes are present.
    let elf_bytes = writer.finalize().expect("finalize should succeed");

    // Parse the ELF and check for the .rodata section.
    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as valid ELF64");

    let mut found = false;
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            let data = section.data().expect(".rodata should have data");
            if data.len() >= 16 {
                assert_eq!(&data[0..16], &gdt_bytes[..]);
                found = true;
            }
        }
    }
    assert!(
        found,
        ".rodata section should contain the GDT descriptor bytes"
    );
}

#[test]
fn elf_writer_rodata_and_data_sections_separate() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind};

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    let rodata_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD];
    let data_bytes = vec![0x11, 0x22, 0x33, 0x44];

    writer.add_rodata_bytes(&rodata_bytes, 4);
    writer.add_data_bytes(&data_bytes, 4);

    let elf_bytes = writer.finalize().expect("finalize should succeed");
    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as valid ELF64");

    let mut found_rodata = false;
    let mut found_data = false;

    for section in elf.sections() {
        let name = section.name().unwrap_or("");
        let data = section.data().expect("section should have data");

        if name == ".rodata" && data.len() >= 4 {
            assert_eq!(&data[0..4], &rodata_bytes[..]);
            found_rodata = true;
        }
        if name == ".data" && data.len() >= 4 {
            assert_eq!(&data[0..4], &data_bytes[..]);
            found_data = true;
        }
    }

    assert!(found_rodata, ".rodata section should be present");
    assert!(found_data, ".data section should be present");
}
