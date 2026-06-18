//! ELF64 object file writer for paideia-as.

use crate::sections::PAIDEIA_SECTIONS;
use object::{
    Architecture, BinaryFormat, Endianness, SectionKind,
    write::{Object, SectionId, StandardSection, StandardSegment},
};
use static_assertions::const_assert_eq;
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

        Self { obj, sections }
    }

    /// Returns a slice of section tuples (name, id) in declaration order.
    ///
    /// This includes both standard and PaideiaOS-specific sections allocated
    /// during construction.
    pub fn sections(&self) -> &[(String, SectionId)] {
        &self.sections
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
}
