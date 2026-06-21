//! ELF64 object file writer for paideia-as.

use crate::relocs::RelocEntry;
use crate::relocs::RelocKind;
use crate::sections::PAIDEIA_SECTIONS;
use crate::symtab::{SymbolEntry, SymbolIndex};
use object::{
    Architecture, BinaryFormat, Endianness, RelocationEncoding, RelocationFlags, RelocationKind,
    SectionKind, SymbolScope,
    write::{
        Object, Relocation, SectionId, StandardSection, StandardSegment, Symbol, SymbolFlags,
        SymbolId, SymbolSection,
    },
};
use static_assertions::const_assert_eq;
use std::collections::HashMap;
use std::mem::size_of;

// Verify that ELF64 file header is 64 bytes per ELF specification.
const_assert_eq!(
    size_of::<object::elf::FileHeader64<object::Endianness>>(),
    64
);

/// Architecture selector for [`ElfWriter`].
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Arch {
    /// x86-64 (amd64) architecture.
    X86_64,
}

/// Output kind for ELF objects.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Kind {
    /// Relocatable object file (`.o`-style output).
    Relocatable,
}

/// Writer for ELF64 object files emitted by paideia-as.
///
/// Constructs ELF64 relocatable objects with standard ELF sections and
/// PaideiaOS-specific custom sections per `custom-assembler.md` §12.1.
pub struct ElfWriter {
    /// The underlying object file being constructed.
    obj: Object<'static>,
    /// Standard section identifiers by name, in declaration order.
    sections: Vec<(String, SectionId)>,
    /// Symbol table entries accumulated during construction.
    /// Mapped by symbol name for deduplication and symbol ID lookup.
    symbols: HashMap<String, (SymbolEntry, SymbolId)>,
}

impl ElfWriter {
    /// Construct a writer for the given architecture and output kind.
    ///
    /// Initializes the ELF object with:
    /// - Standard sections: `.text`, `.rodata`, `.data`, `.bss` (via `object`'s standard helpers)
    /// - PaideiaOS custom sections: `.paideia.caps`, `.paideia.effects`, `.paideia.sig`
    pub fn new(arch: Arch, _kind: Kind) -> Self {
        let architecture = match arch {
            Arch::X86_64 => Architecture::X86_64,
        };
        let mut obj = Object::new(BinaryFormat::Elf, architecture, Endianness::Little);
        let mut sections = Vec::new();

        // Allocate standard sections using the `object` crate's standard helpers.
        // These are recognized by the ELF spec and handled specially by the object crate.
        for (name, standard) in &[
            (".text", StandardSection::Text),
            (".rodata", StandardSection::ReadOnlyData),
            (".data", StandardSection::Data),
            (".bss", StandardSection::UninitializedData),
        ] {
            let sid = obj.section_id(*standard);
            sections.push((name.to_string(), sid));
        }

        // Allocate PaideiaOS-specific custom sections.
        // These are registered as custom sections with SectionKind::Other
        // and belong to the data segment.
        for name in PAIDEIA_SECTIONS {
            let sid = obj.add_section(
                obj.segment_name(StandardSegment::Data).to_vec(),
                name.as_bytes().to_vec(),
                SectionKind::Other,
            );
            sections.push((name.to_string(), sid));
        }

        Self {
            obj,
            sections,
            symbols: HashMap::new(),
        }
    }

    /// Returns a slice of section tuples (name, id) in declaration order.
    ///
    /// This includes both standard and PaideiaOS-specific sections allocated
    /// during construction.
    pub fn sections(&self) -> &[(String, SectionId)] {
        &self.sections
    }

    /// Get the `.text` section ID.
    ///
    /// Phase-5-m4-004: Used to add relocations to .text.
    pub fn text_section_id(&mut self) -> SectionId {
        self.obj.section_id(StandardSection::Text)
    }

    /// Get the `.rodata` section ID.
    ///
    /// Phase-5-m4-003: Used to add data symbols and relocations.
    pub fn rodata_section_id(&mut self) -> SectionId {
        self.obj.section_id(StandardSection::ReadOnlyData)
    }

    /// Get the `.bss` section ID.
    ///
    /// Phase-6-m5-003: Used for symbol bindings to .bss.
    pub fn bss_section_id(&mut self) -> SectionId {
        self.obj.section_id(StandardSection::UninitializedData)
    }

    /// Append `bytes` to the `.text` section. Returns the offset at
    /// which the append starts. Phase-1 helper used by the CLI to
    /// land function bodies; later refinements will accept a
    /// per-function bytes payload + automatic symbol binding.
    pub fn add_text_bytes(&mut self, bytes: &[u8]) -> u64 {
        let text_section = self.text_section_id();
        self.obj.append_section_data(text_section, bytes, 1)
    }

    /// Append `bytes` to the `.rodata` section with the specified alignment.
    /// Returns the offset at which the append starts.
    /// Phase-1 helper used for read-only data (constants, GDT descriptors, etc).
    pub fn add_rodata_bytes(&mut self, bytes: &[u8], align: u8) -> u64 {
        let rodata_section = self.rodata_section_id();
        self.obj
            .append_section_data(rodata_section, bytes, align as u64)
    }

    /// Append `bytes` to the `.data` section with the specified alignment.
    /// Returns the offset at which the append starts.
    /// Phase-1 helper used for initialized mutable data (Phase 6+).
    pub fn add_data_bytes(&mut self, bytes: &[u8], align: u8) -> u64 {
        let data_section = self.obj.section_id(StandardSection::Data);
        self.obj
            .append_section_data(data_section, bytes, align as u64)
    }

    /// Allocate space in the `.bss` section with the specified alignment and size.
    /// Returns the offset at which the allocation starts.
    /// Phase 6 m5-002: used for uninitialized mutable data (let mut x : T = uninit).
    /// Phase 6 m5-003: uses Section::append_bss() which doesn't write file data.
    /// The object crate correctly marks this as SHT_NOBITS and records the size.
    pub fn add_bss_space(&mut self, size: u64, align: u8) -> u64 {
        let bss_section = self.obj.section_id(StandardSection::UninitializedData);
        // Use the Section::append_bss() method which allocates space without writing to file.
        // This ensures .bss is properly marked as SHT_NOBITS with no file payload growth.
        let section = self.obj.section_mut(bss_section);
        section.append_bss(size, align as u64)
    }

    /// Add a symbol to the symbol table.
    ///
    /// Accepts a [`SymbolEntry`] and registers it with the ELF object.
    /// Symbols must be added before relocations that reference them.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `object` crate operation fails
    /// (e.g., invalid symbol configuration).
    pub fn add_symbol(&mut self, entry: SymbolEntry) -> Result<(), Box<dyn std::error::Error>> {
        let sym_name = entry.name.clone();

        // Determine the section for the symbol.
        // Phase 6 m5-003: if entry.section is set, use the corresponding section ID.
        let symbol_section = if let Some(section_kind) = entry.section {
            match section_kind {
                paideia_as_ir::SectionKind::Rodata => {
                    SymbolSection::Section(self.obj.section_id(StandardSection::ReadOnlyData))
                }
                paideia_as_ir::SectionKind::Data => {
                    SymbolSection::Section(self.obj.section_id(StandardSection::Data))
                }
                paideia_as_ir::SectionKind::Bss => {
                    SymbolSection::Section(self.obj.section_id(StandardSection::UninitializedData))
                }
            }
        } else if entry.offset.is_some() {
            // For defined symbols without explicit section, use Undefined.
            // This maintains backward compatibility.
            SymbolSection::Undefined
        } else {
            SymbolSection::Undefined
        };

        let sym_id = self.obj.add_symbol(Symbol {
            name: sym_name.clone().into_bytes(),
            value: entry.offset.unwrap_or(0),
            size: entry.size,
            kind: entry.kind.to_object_kind(),
            scope: if entry.is_global {
                SymbolScope::Dynamic
            } else {
                SymbolScope::Compilation
            },
            weak: false,
            section: symbol_section,
            flags: SymbolFlags::None,
        });

        self.symbols.insert(sym_name, (entry, sym_id));
        Ok(())
    }

    /// Add an undefined symbol to the symbol table if not already present.
    ///
    /// When emitting relocation entries, if a target symbol is not found in the symbol table,
    /// this method is called to add an undefined external reference. The symbol will be
    /// marked as globally visible with type NOTYPE (unknown kind), and the linker will
    /// resolve the actual definition from other object files.
    ///
    /// Returns the symbol index (`SymbolIndex`/`SymbolId`) for use in relocation entries.
    /// If the symbol already exists in the table, returns the existing symbol's ID.
    ///
    /// # Example
    ///
    /// When processing a relocation to `gdt_load` that is not defined in the current
    /// object file, this method creates an undefined symbol entry so the relocation
    /// can reference it. The linker then resolves the actual address when combining
    /// with other object files.
    pub fn add_undefined_symbol(&mut self, name: &str) -> SymbolIndex {
        // Check if symbol already exists.
        if let Some((_, existing_id)) = self.symbols.get(name) {
            return *existing_id;
        }

        // Create an undefined symbol entry.
        let entry = SymbolEntry::undefined(name);
        let sym_id = self.obj.add_symbol(Symbol {
            name: name.as_bytes().to_vec(),
            value: 0,
            size: 0,
            kind: entry.kind.to_object_kind(),
            scope: SymbolScope::Dynamic,
            weak: false,
            section: SymbolSection::Undefined,
            flags: SymbolFlags::None,
        });

        // Cache the symbol for future lookups.
        self.symbols.insert(name.to_string(), (entry, sym_id));

        sym_id
    }

    /// Add a relocation to a section.
    ///
    /// Registers a relocation request for the given section. If the target symbol
    /// is not found in the symbol table, it is automatically added as an undefined
    /// external reference via [`add_undefined_symbol`](Self::add_undefined_symbol).
    ///
    /// Maps paideia-as relocation kinds to `object` crate kinds:
    /// - [`RelocKind::PC32`] → [`RelocationKind::Relative`] (32-bit PC-relative)
    /// - [`RelocKind::Abs64`] → [`RelocationKind::Absolute`] (64-bit absolute)
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `object` crate operation fails.
    pub fn add_relocation(
        &mut self,
        section: SectionId,
        entry: RelocEntry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Look up the target symbol in our symbol table.
        // If not found, add it as an undefined symbol.
        let sym_id = if let Some((_, id)) = self.symbols.get(&entry.target) {
            *id
        } else {
            self.add_undefined_symbol(&entry.target)
        };

        let flags = match entry.kind {
            RelocKind::PC32 => RelocationFlags::Generic {
                kind: RelocationKind::Relative,
                encoding: RelocationEncoding::X86Branch,
                size: 32,
            },
            RelocKind::PLT32 => RelocationFlags::Generic {
                kind: RelocationKind::PltRelative,
                encoding: RelocationEncoding::X86Branch,
                size: 32,
            },
            RelocKind::Abs64 => RelocationFlags::Generic {
                kind: RelocationKind::Absolute,
                encoding: RelocationEncoding::Generic,
                size: 64,
            },
        };

        let reloc = Relocation {
            offset: entry.offset,
            symbol: sym_id,
            addend: entry.addend,
            flags,
        };

        self.obj.add_relocation(section, reloc)?;
        Ok(())
    }

    /// Add a `.note.paideia` section containing JSON-serialised record layouts.
    ///
    /// Per ELF specification and Phase 6 m3-006, the note section contains:
    /// - `n_namesz = 8` (b"paideia\0")
    /// - `n_type = 0x50441600` (PDX_LAYOUTS)
    /// - descriptor bytes = `serde_json::to_vec(&record_layouts)`
    ///
    /// The section is marked as SHT_NOTE and SHF_ALLOC=0 (not loaded into memory).
    ///
    /// # Arguments
    ///
    /// * `note_bytes` - Pre-encoded note bytes (typically from `notes::encode_paideia_note`)
    ///
    /// # Notes
    ///
    /// This method should only be called if the record layouts are non-empty.
    /// The `object` crate will handle section alignment and ELF formatting.
    pub fn add_note_section(
        &mut self,
        note_bytes: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create a `.note.paideia` section with SectionKind::Note.
        // The `object` crate automatically marks it as SHT_NOTE.
        let note_section = self.obj.add_section(
            vec![], // Empty segment name (notes are typically in their own segment)
            b".note.paideia".to_vec(),
            object::SectionKind::Note,
        );

        // Append the encoded note data to the section.
        self.obj.append_section_data(note_section, note_bytes, 4);

        Ok(())
    }

    /// Finalize and write the ELF object to bytes.
    ///
    /// Returns a vector of bytes representing a valid, parseable ELF64 object file.
    pub fn finalize(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(self.obj.write()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::{Object, ObjectSection};

    #[test]
    fn new_x86_64_relocatable_constructs() {
        let _writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        // Passes if constructor doesn't panic.
    }

    #[test]
    fn writer_has_standard_sections() {
        let writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        let section_names: Vec<&str> = writer
            .sections()
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        // Verify that all standard sections we explicitly allocated are present.
        assert!(section_names.contains(&".text"), "missing .text section");
        assert!(
            section_names.contains(&".rodata"),
            "missing .rodata section"
        );
        assert!(section_names.contains(&".data"), "missing .data section");
        assert!(section_names.contains(&".bss"), "missing .bss section");
    }

    #[test]
    fn writer_has_paideia_sections() {
        let writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        let section_names: Vec<&str> = writer
            .sections()
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        // Verify that all PaideiaOS-specific sections we allocated are present.
        assert!(
            section_names.contains(&".paideia.caps"),
            "missing .paideia.caps section"
        );
        assert!(
            section_names.contains(&".paideia.effects"),
            "missing .paideia.effects section"
        );
        assert!(
            section_names.contains(&".paideia.sig"),
            "missing .paideia.sig section"
        );
    }

    #[test]
    fn finalize_produces_valid_elf_bytes() {
        let writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        let bytes = writer
            .finalize()
            .expect("finalize should not fail on a valid writer");

        // Check ELF magic: 0x7F 'E' 'L' 'F'
        assert!(bytes.len() >= 4, "ELF output must be at least 4 bytes");
        assert_eq!(bytes[0], 0x7F, "ELF magic byte 0");
        assert_eq!(bytes[1], b'E', "ELF magic byte 1");
        assert_eq!(bytes[2], b'L', "ELF magic byte 2");
        assert_eq!(bytes[3], b'F', "ELF magic byte 3");
    }

    #[test]
    fn finalize_can_be_parsed_back() {
        let writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        let bytes = writer
            .finalize()
            .expect("finalize should not fail on a valid writer");

        // Attempt to parse the generated bytes as an ELF file.
        let result = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice());
        assert!(
            result.is_ok(),
            "finalized bytes should parse as a valid ELF64 file"
        );
    }

    #[test]
    fn section_count_matches() {
        let writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        // We explicitly allocate 4 standard sections + 3 paideia sections = 7 total.
        // The object crate may add additional sections (like .shstrtab) automatically,
        // so we verify that we have at least our 7 explicitly allocated sections.
        assert!(
            writer.sections().len() >= 7,
            "should have at least 7 explicitly allocated sections, got {}",
            writer.sections().len()
        );
    }

    #[test]
    fn writer_accepts_a_function_symbol() {
        use object::Object;

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        let entry = crate::symtab::SymbolEntry::func("main", 0, 5);

        let result = writer.add_symbol(entry);
        assert!(result.is_ok(), "adding a function symbol should succeed");

        // Finalize and verify the output is parseable ELF.
        let bytes = writer
            .finalize()
            .expect("finalize should not fail after adding a symbol");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
            .expect("finalized bytes should parse as valid ELF64");

        // Verify at least one symbol was written.
        let symbols: Vec<_> = elf.symbols().collect();
        assert!(
            !symbols.is_empty(),
            "ELF should contain at least one symbol"
        );
    }

    #[test]
    fn writer_accepts_a_pc32_call_relocation() {
        use crate::relocs::RelocEntry;
        use object::{Object, ObjectSection};

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        // Add two function symbols.
        let sym1 = crate::symtab::SymbolEntry::func("caller", 0, 10);
        let sym2 = crate::symtab::SymbolEntry::func("callee", 10, 5);

        writer
            .add_symbol(sym1)
            .expect("adding caller should succeed");
        writer
            .add_symbol(sym2)
            .expect("adding callee should succeed");

        // Get the .text section ID.
        let text_section = writer
            .sections()
            .iter()
            .find(|(name, _)| name == ".text")
            .map(|(_, id)| *id)
            .expect("should have .text section");

        // Add a PC32 relocation from caller to callee.
        let reloc = RelocEntry::call(5, "callee");
        let result = writer.add_relocation(text_section, reloc);
        assert!(result.is_ok(), "adding a PC32 relocation should succeed");

        // Finalize and verify the output is parseable ELF.
        let bytes = writer
            .finalize()
            .expect("finalize should not fail after adding a relocation");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
            .expect("finalized bytes should parse as valid ELF64");

        // Verify at least one relocation was written (in .text or .rela.text).
        let mut found_relocation = false;
        for section in elf.sections() {
            let reloc_vec: Vec<_> = section.relocations().collect();
            if !reloc_vec.is_empty() {
                found_relocation = true;
                break;
            }
        }
        assert!(
            found_relocation,
            "ELF should contain at least one relocation"
        );
    }

    #[test]
    fn writer_accepts_an_undefined_symbol() {
        use object::Object;

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
        let entry = crate::symtab::SymbolEntry::undefined("external_fn");

        let result = writer.add_symbol(entry);
        assert!(result.is_ok(), "adding an undefined symbol should succeed");

        // Finalize and verify the output is parseable ELF.
        let bytes = writer
            .finalize()
            .expect("finalize should not fail after adding an undefined symbol");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
            .expect("finalized bytes should parse as valid ELF64");

        // Verify at least one symbol was written.
        let symbols: Vec<_> = elf.symbols().collect();
        assert!(
            !symbols.is_empty(),
            "ELF should contain at least one symbol"
        );
    }

    #[test]
    fn writer_add_relocation_to_unknown_symbol_creates_undefined() {
        use crate::relocs::RelocEntry;
        use object::ObjectSymbol;

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        // Get the .text section ID.
        let text_section = writer
            .sections()
            .iter()
            .find(|(name, _)| name == ".text")
            .map(|(_, id)| *id)
            .expect("should have .text section");

        // Add a relocation to a symbol that was never explicitly added.
        // This should now succeed by creating an undefined symbol.
        let reloc = RelocEntry::call(5, "external_fn");
        let result = writer.add_relocation(text_section, reloc);

        assert!(
            result.is_ok(),
            "adding a relocation to an unknown symbol should create an undefined symbol"
        );

        // Finalize and verify the output is parseable ELF.
        let bytes = writer
            .finalize()
            .expect("finalize should succeed after adding a relocation to undefined symbol");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
            .expect("finalized bytes should parse as valid ELF64");

        // Verify the undefined symbol exists in the symbol table.
        let symbols: Vec<_> = elf.symbols().collect();
        let found_undefined = symbols
            .iter()
            .any(|sym| sym.name().unwrap_or("") == "external_fn" && sym.is_undefined());
        assert!(
            found_undefined,
            "ELF should contain undefined symbol 'external_fn'"
        );
    }

    #[test]
    fn writer_add_rodata_bytes_appends_to_rodata() {
        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        let bytes = vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];
        let offset = writer.add_rodata_bytes(&bytes, 8);

        // Offset should start at 0 for the first append.
        assert_eq!(offset, 0);

        // Finalize and verify the rodata section contains the bytes.
        let elf_bytes = writer
            .finalize()
            .expect("finalize should succeed after adding rodata");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
            .expect("should parse as valid ELF64");

        // Find and verify the .rodata section contains our bytes.
        let mut found_rodata_bytes = false;
        for section in elf.sections() {
            if section.name().unwrap_or("") == ".rodata" {
                let data = section.data().expect("rodata should have data");
                if data.len() >= 8 {
                    assert_eq!(&data[0..8], &bytes[..]);
                    found_rodata_bytes = true;
                }
            }
        }
        assert!(
            found_rodata_bytes,
            ".rodata section should contain the appended bytes"
        );
    }

    #[test]
    fn writer_add_data_bytes_appends_to_data() {
        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        let bytes = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let offset = writer.add_data_bytes(&bytes, 4);

        // Offset should start at 0 for the first append.
        assert_eq!(offset, 0);

        // Finalize and verify the data section contains the bytes.
        let elf_bytes = writer
            .finalize()
            .expect("finalize should succeed after adding data");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(elf_bytes.as_slice())
            .expect("should parse as valid ELF64");

        // Find and verify the .data section contains our bytes.
        let mut found_data_bytes = false;
        for section in elf.sections() {
            if section.name().unwrap_or("") == ".data" {
                let data = section.data().expect("data should have data");
                if data.len() >= 4 {
                    assert_eq!(&data[0..4], &bytes[..]);
                    found_data_bytes = true;
                }
            }
        }
        assert!(
            found_data_bytes,
            ".data section should contain the appended bytes"
        );
    }

    #[test]
    fn writer_multiple_rodata_appends_increase_offset() {
        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        let bytes1 = vec![0x11, 0x22, 0x33, 0x44];
        let offset1 = writer.add_rodata_bytes(&bytes1, 4);
        assert_eq!(offset1, 0);

        let bytes2 = vec![0x55, 0x66, 0x77, 0x88];
        let offset2 = writer.add_rodata_bytes(&bytes2, 4);
        // Second append should start after the first.
        assert!(offset2 > offset1);
    }

    #[test]
    fn writer_add_undefined_symbol_creates_undefined() {
        use object::ObjectSymbol;

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        // Add an undefined symbol.
        let _sym_id = writer.add_undefined_symbol("gdt_load");

        // Finalize and verify the output is parseable ELF.
        let bytes = writer
            .finalize()
            .expect("finalize should succeed after adding an undefined symbol");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
            .expect("finalized bytes should parse as valid ELF64");

        // Verify the undefined symbol exists in the symbol table.
        let symbols: Vec<_> = elf.symbols().collect();
        let found_undefined = symbols
            .iter()
            .any(|sym| sym.name().unwrap_or("") == "gdt_load" && sym.is_undefined());
        assert!(
            found_undefined,
            "ELF should contain undefined symbol 'gdt_load'"
        );
    }

    #[test]
    fn writer_add_undefined_symbol_deduplicates() {
        use object::ObjectSymbol;

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        // Add the same undefined symbol twice.
        let sym_id1 = writer.add_undefined_symbol("extern_fn");
        let sym_id2 = writer.add_undefined_symbol("extern_fn");

        // Both should return the same symbol ID.
        assert_eq!(
            sym_id1, sym_id2,
            "duplicate undefined symbols should return same ID"
        );

        // Finalize and verify only one symbol was created.
        let bytes = writer
            .finalize()
            .expect("finalize should succeed after adding duplicate undefined symbols");
        let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
            .expect("finalized bytes should parse as valid ELF64");

        // Count how many "extern_fn" symbols exist.
        let symbols: Vec<_> = elf.symbols().collect();
        let count = symbols
            .iter()
            .filter(|sym| sym.name().unwrap_or("") == "extern_fn")
            .count();
        assert_eq!(count, 1, "should have exactly one extern_fn symbol");
    }
}
