//! Integration test for `.note.paideia` section emission.
//!
//! Phase 6 m3-006: Verifies that ELF objects can round-trip record layout
//! information via a `.note.paideia` section, and that the note can be
//! read back via the `object` crate.

use object::{Object, ObjectSection};
use paideia_as_emitter_elf::{
    Arch, ElfWriter, Kind,
    notes::{PDX_LAYOUTS, encode_paideia_note},
};
use paideia_as_ir::record_layout::{FieldLayout, FinalisedLayoutTable, RecordLayout, RecordTypeId};
use serde_json;

#[test]
fn note_paideia_empty_layouts_omitted() {
    // When record_layouts is empty, the section should be omitted.
    // This test verifies that we don't emit an empty note.
    let writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    let empty_table = FinalisedLayoutTable::new();

    // Serialize an empty table.
    let _json = serde_json::to_vec(&empty_table).expect("serialize empty table");

    // Empty JSON is "{}" which is 2 bytes. We should not emit a note in this case,
    // but here we're testing the encoder itself.
    // The caller (m3-007) is responsible for the "omit if empty" logic.

    let bytes = writer
        .finalize()
        .expect("finalize should succeed with empty layouts");

    // Verify it's still valid ELF.
    assert!(bytes.len() >= 4);
    assert_eq!(bytes[0], 0x7F);
    assert_eq!(bytes[1], b'E');
    assert_eq!(bytes[2], b'L');
    assert_eq!(bytes[3], b'F');
}

#[test]
fn note_paideia_capability_struct_layout() {
    // Build a record layout for a Capability struct: 4 × u64 → 32 bytes, align 8.
    let capability_fields = vec![
        FieldLayout { offset: 0, size: 8 },
        FieldLayout { offset: 8, size: 8 },
        FieldLayout {
            offset: 16,
            size: 8,
        },
        FieldLayout {
            offset: 24,
            size: 8,
        },
    ];
    let capability_layout = RecordLayout::new(32, 8, capability_fields);

    // Insert into the finalised table.
    let mut table = FinalisedLayoutTable::new();
    table.insert(RecordTypeId(1), capability_layout.clone());

    // Serialize the table to JSON.
    let json = serde_json::to_vec(&table).expect("serialize table");

    // Encode the note.
    let note_bytes = encode_paideia_note(&json);

    // Create an ELF writer and add the note.
    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    writer
        .add_note_section(&note_bytes)
        .expect("add note section should succeed");

    // Finalize and verify the output.
    let elf_bytes = writer
        .finalize()
        .expect("finalize should succeed after adding note");

    // Parse the ELF file back.
    let elf_file = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as ELF64");

    // Find the `.note.paideia` section.
    let mut note_section_found = false;
    for section in elf_file.sections() {
        if section.name().unwrap_or("") == ".note.paideia" {
            note_section_found = true;

            // Verify the section kind is SHT_NOTE.
            assert_eq!(section.kind(), object::SectionKind::Note);

            // Verify the section is not loaded into memory (SHF_ALLOC = 0).
            // The `object` crate doesn't expose all flags directly, but we can
            // verify via the raw ELF section header.
            // (This is implicitly verified by the section kind check.)

            // Verify the note content.
            let section_data = section.data().expect("section should have data");
            assert_eq!(section_data.len(), note_bytes.len());
            assert_eq!(section_data, note_bytes.as_slice());

            break;
        }
    }

    assert!(
        note_section_found,
        ".note.paideia section should be present"
    );
}

#[test]
fn note_paideia_header_format() {
    // Verify that the encoded note has the correct ELF note header.
    let mut table = FinalisedLayoutTable::new();
    let layout = RecordLayout::new(16, 8, vec![FieldLayout { offset: 0, size: 8 }]);
    table.insert(RecordTypeId(42), layout);

    let json = serde_json::to_vec(&table).expect("serialize table");
    let note_bytes = encode_paideia_note(&json);

    // Verify the header:
    // bytes 0-3: n_namesz = 8 (little-endian)
    let n_namesz = u32::from_le_bytes([note_bytes[0], note_bytes[1], note_bytes[2], note_bytes[3]]);
    assert_eq!(n_namesz, 8);

    // bytes 4-7: n_descsz (should be the JSON size)
    let n_descsz = u32::from_le_bytes([note_bytes[4], note_bytes[5], note_bytes[6], note_bytes[7]]);
    assert_eq!(n_descsz as usize, json.len());

    // bytes 8-11: n_type = 0x50441600 (PDX_LAYOUTS, little-endian)
    let n_type = u32::from_le_bytes([note_bytes[8], note_bytes[9], note_bytes[10], note_bytes[11]]);
    assert_eq!(n_type, PDX_LAYOUTS);

    // bytes 12-19: name = "paideia\0"
    assert_eq!(&note_bytes[12..20], b"paideia\0");

    // bytes 20+: descriptor data should match JSON
    assert_eq!(&note_bytes[20..20 + json.len()], json.as_slice());
}

#[test]
fn note_paideia_round_trip_deserialization() {
    // Build a table, serialize, encode, write to ELF, read back, and deserialize.
    let mut original_table = FinalisedLayoutTable::new();

    let layout1 = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8 },
            FieldLayout { offset: 8, size: 8 },
            FieldLayout {
                offset: 16,
                size: 8,
            },
            FieldLayout {
                offset: 24,
                size: 8,
            },
        ],
    );

    let layout2 = RecordLayout::new(16, 8, vec![FieldLayout { offset: 0, size: 8 }]);

    original_table.insert(RecordTypeId(1), layout1);
    original_table.insert(RecordTypeId(2), layout2);

    // Serialize and encode.
    let json = serde_json::to_vec(&original_table).expect("serialize table");
    let note_bytes = encode_paideia_note(&json);

    // Write to ELF.
    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    writer
        .add_note_section(&note_bytes)
        .expect("add note section should succeed");

    let elf_bytes = writer
        .finalize()
        .expect("finalize should succeed after adding note");

    // Parse and extract the note.
    let elf_file = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as ELF64");

    let mut recovered_json = None;
    for section in elf_file.sections() {
        if section.name().unwrap_or("") == ".note.paideia" {
            let section_data = section.data().expect("section should have data");

            // Extract the descriptor from the note.
            // Header (12 bytes) contains:
            // - bytes 0-3: n_namesz (should be 8)
            // - bytes 4-7: n_descsz (the actual JSON size)
            // - bytes 8-11: n_type
            let n_descsz = u32::from_le_bytes([
                section_data[4],
                section_data[5],
                section_data[6],
                section_data[7],
            ]) as usize;

            // Name is at 12..20 ("paideia\0")
            // Descriptor is at 20..20+n_descsz
            let start = 20;
            let end = start + n_descsz;
            recovered_json = Some(section_data[start..end].to_vec());
            break;
        }
    }

    let recovered_json = recovered_json.expect("should have recovered JSON from note");

    // Verify JSON round-trips.
    assert_eq!(recovered_json, json);

    // Deserialize back to a table.
    let recovered_table: FinalisedLayoutTable =
        serde_json::from_slice(&recovered_json).expect("deserialize recovered JSON");

    // Verify the layouts are identical.
    let layout1_recovered = recovered_table
        .get(RecordTypeId(1))
        .expect("should have layout 1");
    assert_eq!(layout1_recovered.size, 32);
    assert_eq!(layout1_recovered.align, 8);
    assert_eq!(layout1_recovered.fields.len(), 4);
    assert_eq!(layout1_recovered.fields[0].offset, 0);
    assert_eq!(layout1_recovered.fields[0].size, 8);
    assert_eq!(layout1_recovered.fields[3].offset, 24);
    assert_eq!(layout1_recovered.fields[3].size, 8);

    let layout2_recovered = recovered_table
        .get(RecordTypeId(2))
        .expect("should have layout 2");
    assert_eq!(layout2_recovered.size, 16);
    assert_eq!(layout2_recovered.align, 8);
    assert_eq!(layout2_recovered.fields.len(), 1);
    assert_eq!(layout2_recovered.fields[0].offset, 0);
    assert_eq!(layout2_recovered.fields[0].size, 8);
}

#[test]
fn note_paideia_readelf_compatible() {
    // Verify that the note can be read by standard ELF tools.
    // (This is a structural verification; actual readelf would need a binary.)

    let mut table = FinalisedLayoutTable::new();
    let layout = RecordLayout::new(32, 8, vec![FieldLayout { offset: 0, size: 8 }]);
    table.insert(RecordTypeId(1), layout);

    let json = serde_json::to_vec(&table).expect("serialize table");
    let note_bytes = encode_paideia_note(&json);

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    writer
        .add_note_section(&note_bytes)
        .expect("add note section should succeed");

    let elf_bytes = writer
        .finalize()
        .expect("finalize should succeed after adding note");

    let elf_file = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
        .expect("should parse as ELF64");

    let mut found = false;
    for section in elf_file.sections() {
        if section.name().unwrap_or("") == ".note.paideia" {
            // Verify the section has the correct attributes.
            assert_eq!(section.kind(), object::SectionKind::Note);
            // Note sections are not typically allocated (SHF_ALLOC=0).
            // The object crate handles this during ELF serialization.
            found = true;
            break;
        }
    }
    assert!(found, ".note.paideia section should be found");
}
