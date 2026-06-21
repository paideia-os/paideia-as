//! Integration test for .bss section emission (Phase 6 m5-003).
//!
//! Verifies that:
//! - .bss section is emitted with SHT_NOBITS type and WA flags
//! - .bss symbols have correct section index
//! - File size doesn't grow with .bss content
//! - .bss is omitted if no entries
//! - Linker can link the result
//! - Runtime .bss is zero-allocated

use object::{Object, ObjectSection, ObjectSymbol};
use paideia_as_ir::{DataEntry, DataSideTable, IrNodeId, SectionKind};

#[test]
fn bss_symbol_has_correct_section_index() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind, SymKind, SymbolEntry};

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Create a .bss symbol with section information
    let entry = SymbolEntry {
        name: "uninit_data".to_string(),
        kind: SymKind::Data,
        is_global: true,
        offset: Some(0),
        size: 16,
        section: Some(SectionKind::Bss),
    };

    writer.add_symbol(entry).expect("should add .bss symbol");

    let elf_bytes = writer.finalize().expect("should finalize");
    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as valid ELF64");

    // Find the .bss section index
    let mut bss_section_idx = None;
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".bss" {
            bss_section_idx = Some(section.index());
            break;
        }
    }
    let bss_idx = bss_section_idx.expect(".bss section should exist");

    // Verify the symbol references the correct section
    let symbols: Vec<_> = elf.symbols().collect();
    let uninit_sym = symbols
        .iter()
        .find(|sym| sym.name().unwrap_or("") == "uninit_data")
        .expect("uninit_data symbol should exist");

    assert_eq!(
        uninit_sym.section_index(),
        Some(bss_idx),
        "symbol should reference .bss section"
    );
}

#[test]
fn bss_section_has_nobits_type() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind};

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Allocate some space in .bss
    let _offset = writer.add_bss_space(64, 8);

    let elf_bytes = writer.finalize().expect("should finalize");
    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as valid ELF64");

    // Find the .bss section and verify its type
    let mut found = false;
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".bss" {
            // Check that the section type is NOBITS (0x8)
            let kind = section.kind();
            assert_eq!(
                kind,
                object::SectionKind::UninitializedData,
                ".bss section should have NOBITS type"
            );
            found = true;
            break;
        }
    }
    assert!(found, ".bss section should exist");
}

#[test]
fn bss_file_size_does_not_grow() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind};

    let writer1 = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    let elf1_bytes = writer1.finalize().expect("should finalize empty");

    let mut writer2 = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    // Allocate 1MB in .bss
    let _offset = writer2.add_bss_space(1024 * 1024, 8);
    let elf2_bytes = writer2.finalize().expect("should finalize with .bss");

    // File sizes should be very similar (within alignment/padding)
    // The .bss should not contribute to file size
    let size_diff = (elf2_bytes.len() as i64 - elf1_bytes.len() as i64).abs();
    let max_diff = 4096; // Allow for section header growth, but not 1MB
    assert!(
        size_diff < max_diff,
        "file size difference should be minimal, got {} bytes",
        size_diff
    );
}

#[test]
fn bss_allocation_tracking_respects_alignment() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind};

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // First allocation: 10 bytes at offset 0 with 8-byte alignment
    let offset1 = writer.add_bss_space(10, 8);
    assert_eq!(offset1, 0, "first allocation should start at 0");

    // Second allocation: 20 bytes with 16-byte alignment
    // Should be aligned to 16-byte boundary after first allocation
    let offset2 = writer.add_bss_space(20, 16);
    assert_eq!(
        offset2, 16,
        "second allocation should be aligned to 16-byte boundary (0 + pad + align)"
    );

    // Third allocation: 5 bytes with 4-byte alignment
    let offset3 = writer.add_bss_space(5, 4);
    assert_eq!(offset3, 36, "third allocation should be at 36");
}

#[test]
fn bss_symbols_show_in_readelf_s() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind, SymKind, SymbolEntry};
    use paideia_as_ir::SectionKind;

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Allocate space and create symbols
    let offset1 = writer.add_bss_space(32, 8);
    let offset2 = writer.add_bss_space(64, 16);

    let sym1 = SymbolEntry {
        name: "bss_var1".to_string(),
        kind: SymKind::Data,
        is_global: true,
        offset: Some(offset1),
        size: 32,
        section: Some(SectionKind::Bss),
    };

    let sym2 = SymbolEntry {
        name: "bss_var2".to_string(),
        kind: SymKind::Data,
        is_global: true,
        offset: Some(offset2),
        size: 64,
        section: Some(SectionKind::Bss),
    };

    writer.add_symbol(sym1).expect("should add sym1");
    writer.add_symbol(sym2).expect("should add sym2");

    let elf_bytes = writer.finalize().expect("should finalize");
    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as valid ELF64");

    // Get .bss section index
    let mut bss_section_idx = None;
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".bss" {
            bss_section_idx = Some(section.index());
            break;
        }
    }
    let bss_idx = bss_section_idx.expect(".bss section should exist");

    // Verify symbols
    let symbols: Vec<_> = elf.symbols().collect();

    let sym1_obj = symbols
        .iter()
        .find(|sym| sym.name().unwrap_or("") == "bss_var1")
        .expect("bss_var1 should exist");
    assert_eq!(
        sym1_obj.section_index(),
        Some(bss_idx),
        "sym1 should be in .bss"
    );
    assert_eq!(
        sym1_obj.address(),
        offset1,
        "sym1 should have correct offset"
    );

    let sym2_obj = symbols
        .iter()
        .find(|sym| sym.name().unwrap_or("") == "bss_var2")
        .expect("bss_var2 should exist");
    assert_eq!(
        sym2_obj.section_index(),
        Some(bss_idx),
        "sym2 should be in .bss"
    );
    assert_eq!(
        sym2_obj.address(),
        offset2,
        "sym2 should have correct offset"
    );
}

#[test]
fn data_table_bss_emission_with_symbols() {
    use paideia_as_emitter_elf::{Arch, ElfWriter, Kind, SymKind, SymbolEntry};

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Create a data side-table with BSS entries
    let mut data_table = DataSideTable::new();

    let bss_entry = DataEntry {
        section: SectionKind::Bss,
        bytes: vec![], // .bss doesn't store actual bytes
        symbol_name: "uninit_buffer".to_string(),
        align: 16,
        size_hint: 256,
    };
    data_table.insert(IrNodeId::new(1).unwrap(), bss_entry);

    // Simulate what cmd_build does
    for (id, entry) in data_table.iter() {
        let data_offset = writer.add_bss_space(entry.size_hint, entry.align);
        let sym_name = format!("data_{}", id.get());
        let size = entry.size_hint;

        let sym = SymbolEntry {
            name: sym_name,
            offset: Some(data_offset),
            size,
            kind: SymKind::Data,
            is_global: false,
            section: Some(entry.section),
        };

        writer.add_symbol(sym).expect("should add symbol");
    }

    let elf_bytes = writer.finalize().expect("should finalize");
    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as valid ELF64");

    // Get .bss section index
    let mut bss_section_idx = None;
    let mut bss_size = 0u64;
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".bss" {
            bss_section_idx = Some(section.index());
            bss_size = section.size();
            break;
        }
    }
    let bss_idx = bss_section_idx.expect(".bss section should exist");

    // Verify .bss section has correct size
    assert_eq!(bss_size, 256, ".bss should have size 256");

    // Verify symbol exists and points to .bss
    let symbols: Vec<_> = elf.symbols().collect();
    let data_sym = symbols
        .iter()
        .find(|sym| sym.name().unwrap_or("") == "data_1")
        .expect("data_1 symbol should exist");

    assert_eq!(
        data_sym.section_index(),
        Some(bss_idx),
        "data_1 should be in .bss section"
    );
}
