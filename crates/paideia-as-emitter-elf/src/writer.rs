//! ELF64 object file writer for paideia-as.

use crate::relocs::RelocEntry;
use crate::relocs::RelocKind;
use crate::sections::PAIDEIA_SECTIONS;
use crate::symtab::SymbolEntry;
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

    /// Append `bytes` to the `.text` section. Returns the offset at
    /// which the append starts. Phase-1 helper used by the CLI to
    /// land function bodies; later refinements will accept a
    /// per-function bytes payload + automatic symbol binding.
    pub fn add_text_bytes(&mut self, bytes: &[u8]) -> u64 {
        let text_section = self.obj.section_id(StandardSection::Text);
        self.obj.append_section_data(text_section, bytes, 1)
    }

    /// Append `bytes` to the `.rodata` section with the specified alignment.
    /// Returns the offset at which the append starts.
    /// Phase-1 helper used for read-only data (constants, GDT descriptors, etc).
    pub fn add_rodata_bytes(&mut self, bytes: &[u8], align: u8) -> u64 {
        let rodata_section = self.obj.section_id(StandardSection::ReadOnlyData);
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
            section: if entry.offset.is_some() {
                // For defined symbols, we would ideally link to the actual section.
                // For now, we use Undefined and let the linker resolve via absolute addressing.
                // In a full implementation, the caller would specify which section the symbol belongs to.
                SymbolSection::Undefined
            } else {
                SymbolSection::Undefined
            },
            flags: SymbolFlags::None,
        });

        self.symbols.insert(sym_name, (entry, sym_id));
        Ok(())
    }

    /// Add a relocation to a section.
    ///
    /// Registers a relocation request for the given section. The target symbol
    /// must already have been added via [`add_symbol`](Self::add_symbol).
    ///
    /// Maps paideia-as relocation kinds to `object` crate kinds:
    /// - [`RelocKind::PC32`] → [`RelocationKind::Relative`] (32-bit PC-relative)
    /// - [`RelocKind::Abs64`] → [`RelocationKind::Absolute`] (64-bit absolute)
    ///
    /// # Errors
    ///
    /// Returns an error if the target symbol is not found in the symbol table,
    /// or if the underlying `object` crate operation fails.
    pub fn add_relocation(
        &mut self,
        section: SectionId,
        entry: RelocEntry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Look up the target symbol in our symbol table.
        let sym_id = self
            .symbols
            .get(&entry.target)
            .map(|(_, id)| *id)
            .ok_or_else(|| -> Box<dyn std::error::Error> {
                format!("symbol '{}' not found in symbol table", entry.target).into()
            })?;

        let flags = match entry.kind {
            RelocKind::PC32 => RelocationFlags::Generic {
                kind: RelocationKind::Relative,
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
    fn writer_rejects_relocation_to_unknown_symbol() {
        use crate::relocs::RelocEntry;

        let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

        // Get the .text section ID.
        let text_section = writer
            .sections()
            .iter()
            .find(|(name, _)| name == ".text")
            .map(|(_, id)| *id)
            .expect("should have .text section");

        // Try to add a relocation to a symbol that was never added.
        let reloc = RelocEntry::call(5, "unknown_symbol");
        let result = writer.add_relocation(text_section, reloc);

        assert!(
            result.is_err(),
            "adding a relocation to an unknown symbol should fail"
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
}
