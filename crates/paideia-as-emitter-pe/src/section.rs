//! PE/COFF section headers and section table emission for Microsoft x64 / UEFI binaries.
//!
//! This module defines the binary layout, serialization, and RVA computation for section headers
//! and the section table. All multi-byte fields are serialized in little-endian byte order.
//!
//! Layout summary:
//! - Section Header (40 bytes): Name, sizes, addresses, and characteristics for each section.
//! - Section Table: A collection of sections with finalization logic to compute RVAs and file pointers.

use static_assertions::const_assert_eq;

// ============================================================================
// Constants
// ============================================================================

/// Size of a section header in bytes (PE/COFF standard).
pub const SECTION_HEADER_SIZE: usize = 40;

const_assert_eq!(SECTION_HEADER_SIZE, 40);

/// Length of a section name field in bytes.
pub const SECTION_NAME_LEN: usize = 8;

/// Section characteristic: Contains executable code.
pub const IMAGE_SCN_CNT_CODE: u32 = 0x0000_0020;

/// Section characteristic: Contains initialized data.
pub const IMAGE_SCN_CNT_INITIALIZED_DATA: u32 = 0x0000_0040;

/// Section characteristic: Contains uninitialized data (BSS).
pub const IMAGE_SCN_CNT_UNINITIALIZED_DATA: u32 = 0x0000_0080;

/// Section characteristic: Section is executable.
pub const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;

/// Section characteristic: Section is readable.
pub const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;

/// Section characteristic: Section is writable.
pub const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

/// Combined characteristics for .text section (code, executable, readable).
pub const CHARACTERISTICS_TEXT: u32 =
    IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ;

/// Combined characteristics for .rdata section (initialized data, readable).
pub const CHARACTERISTICS_RDATA: u32 = IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ;

/// Combined characteristics for .data section (initialized data, readable, writable).
pub const CHARACTERISTICS_DATA: u32 =
    IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE;

/// Combined characteristics for .bss section (uninitialized data, readable, writable).
pub const CHARACTERISTICS_BSS: u32 =
    IMAGE_SCN_CNT_UNINITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE;

// ============================================================================
// Utilities
// ============================================================================

/// Align a value up to the next multiple of a power-of-two alignment.
///
/// # Panics
///
/// Panics in debug builds if `align` is not a power of two.
pub fn align_up(value: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

// ============================================================================
// SectionHeader
// ============================================================================

/// Section header (40 bytes).
///
/// Describes the layout, size, and permissions of a section within a PE file.
/// All multi-byte fields are stored in little-endian byte order.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SectionHeader {
    /// ASCII section name, NUL-padded to 8 bytes.
    pub name: [u8; 8],
    /// Size of section in memory (virtual size).
    pub virtual_size: u32,
    /// RVA (relative virtual address) of section in memory.
    pub virtual_address: u32,
    /// Size of section in file.
    pub size_of_raw_data: u32,
    /// File offset to section data.
    pub pointer_to_raw_data: u32,
    /// File offset to relocation table (usually 0).
    pub pointer_to_relocations: u32,
    /// File offset to line number table (usually 0).
    pub pointer_to_line_numbers: u32,
    /// Number of relocations (usually 0).
    pub number_of_relocations: u16,
    /// Number of line numbers (usually 0).
    pub number_of_line_numbers: u16,
    /// Section characteristics (e.g., executable, readable, writable).
    pub characteristics: u32,
}

impl SectionHeader {
    /// Serialize to a 40-byte little-endian representation.
    ///
    /// Layout (offsets in bytes):
    /// - 0: name (8 bytes)
    /// - 8: virtual_size (4 bytes, little-endian)
    /// - 12: virtual_address (4 bytes, little-endian)
    /// - 16: size_of_raw_data (4 bytes, little-endian)
    /// - 20: pointer_to_raw_data (4 bytes, little-endian)
    /// - 24: pointer_to_relocations (4 bytes, little-endian)
    /// - 28: pointer_to_line_numbers (4 bytes, little-endian)
    /// - 32: number_of_relocations (2 bytes, little-endian)
    /// - 34: number_of_line_numbers (2 bytes, little-endian)
    /// - 36: characteristics (4 bytes, little-endian)
    pub fn to_bytes(&self) -> [u8; SECTION_HEADER_SIZE] {
        let mut bytes = [0u8; SECTION_HEADER_SIZE];

        // Offset 0: name (8 bytes)
        bytes[0..8].copy_from_slice(&self.name);

        // Offset 8: virtual_size (4 bytes, little-endian)
        bytes[8..12].copy_from_slice(&self.virtual_size.to_le_bytes());

        // Offset 12: virtual_address (4 bytes, little-endian)
        bytes[12..16].copy_from_slice(&self.virtual_address.to_le_bytes());

        // Offset 16: size_of_raw_data (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&self.size_of_raw_data.to_le_bytes());

        // Offset 20: pointer_to_raw_data (4 bytes, little-endian)
        bytes[20..24].copy_from_slice(&self.pointer_to_raw_data.to_le_bytes());

        // Offset 24: pointer_to_relocations (4 bytes, little-endian)
        bytes[24..28].copy_from_slice(&self.pointer_to_relocations.to_le_bytes());

        // Offset 28: pointer_to_line_numbers (4 bytes, little-endian)
        bytes[28..32].copy_from_slice(&self.pointer_to_line_numbers.to_le_bytes());

        // Offset 32: number_of_relocations (2 bytes, little-endian)
        bytes[32..34].copy_from_slice(&self.number_of_relocations.to_le_bytes());

        // Offset 34: number_of_line_numbers (2 bytes, little-endian)
        bytes[34..36].copy_from_slice(&self.number_of_line_numbers.to_le_bytes());

        // Offset 36: characteristics (4 bytes, little-endian)
        bytes[36..40].copy_from_slice(&self.characteristics.to_le_bytes());

        bytes
    }

    /// Parse from a byte slice.
    ///
    /// Returns `Some(header)` if input is at least 40 bytes.
    /// Returns `None` on short input.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < SECTION_HEADER_SIZE {
            return None;
        }

        // Offset 0: name (8 bytes)
        let name = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];

        // Offset 8: virtual_size (4 bytes, little-endian)
        let virtual_size = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);

        // Offset 12: virtual_address (4 bytes, little-endian)
        let virtual_address = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);

        // Offset 16: size_of_raw_data (4 bytes, little-endian)
        let size_of_raw_data = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);

        // Offset 20: pointer_to_raw_data (4 bytes, little-endian)
        let pointer_to_raw_data = u32::from_le_bytes([b[20], b[21], b[22], b[23]]);

        // Offset 24: pointer_to_relocations (4 bytes, little-endian)
        let pointer_to_relocations = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);

        // Offset 28: pointer_to_line_numbers (4 bytes, little-endian)
        let pointer_to_line_numbers = u32::from_le_bytes([b[28], b[29], b[30], b[31]]);

        // Offset 32: number_of_relocations (2 bytes, little-endian)
        let number_of_relocations = u16::from_le_bytes([b[32], b[33]]);

        // Offset 34: number_of_line_numbers (2 bytes, little-endian)
        let number_of_line_numbers = u16::from_le_bytes([b[34], b[35]]);

        // Offset 36: characteristics (4 bytes, little-endian)
        let characteristics = u32::from_le_bytes([b[36], b[37], b[38], b[39]]);

        Some(Self {
            name,
            virtual_size,
            virtual_address,
            size_of_raw_data,
            pointer_to_raw_data,
            pointer_to_relocations,
            pointer_to_line_numbers,
            number_of_relocations,
            number_of_line_numbers,
            characteristics,
        })
    }
}

// ============================================================================
// Section
// ============================================================================

/// A section with header and content.
///
/// The content is empty for BSS sections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Section header metadata.
    pub header: SectionHeader,
    /// Section content in memory (empty for BSS).
    pub content: Vec<u8>,
}

impl Section {
    /// Create a new section with the given name, characteristics, content, and virtual size.
    fn new_named(name: &[u8], characteristics: u32, content: Vec<u8>, virtual_size: u32) -> Self {
        let mut name_bytes = [0u8; SECTION_NAME_LEN];
        let copy_len = core::cmp::min(name.len(), SECTION_NAME_LEN);
        name_bytes[..copy_len].copy_from_slice(&name[..copy_len]);

        Self {
            header: SectionHeader {
                name: name_bytes,
                virtual_size,
                virtual_address: 0,
                size_of_raw_data: 0,
                pointer_to_raw_data: 0,
                pointer_to_relocations: 0,
                pointer_to_line_numbers: 0,
                number_of_relocations: 0,
                number_of_line_numbers: 0,
                characteristics,
            },
            content,
        }
    }
}

// ============================================================================
// SectionTable
// ============================================================================

/// A table of sections with finalization logic for RVA and file pointer computation.
pub struct SectionTable {
    /// Collection of sections.
    pub sections: Vec<Section>,
}

impl SectionTable {
    /// Create a new empty section table.
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Add a .text section (code).
    pub fn add_text(&mut self, content: Vec<u8>) {
        let vsize = content.len() as u32;
        self.sections.push(Section::new_named(
            b".text\0\0\0",
            CHARACTERISTICS_TEXT,
            content,
            vsize,
        ));
    }

    /// Get mutable reference to the text section if one exists, else create one.
    /// Returns a mutable reference to the text section's content buffer.
    pub fn text_section_mut(&mut self) -> &mut Vec<u8> {
        // Check if .text section already exists
        if self.sections.is_empty() || self.sections[0].header.name != *b".text\0\0\0" {
            // Create a new empty .text section at the front
            self.sections.insert(
                0,
                Section::new_named(b".text\0\0\0", CHARACTERISTICS_TEXT, Vec::new(), 0),
            );
        }
        &mut self.sections[0].content
    }

    /// Add a .data section (initialized data).
    pub fn add_data(&mut self, content: Vec<u8>) {
        let vsize = content.len() as u32;
        self.sections.push(Section::new_named(
            b".data\0\0\0",
            CHARACTERISTICS_DATA,
            content,
            vsize,
        ));
    }

    /// Add a .rdata section (read-only initialized data).
    pub fn add_rdata(&mut self, content: Vec<u8>) {
        let vsize = content.len() as u32;
        self.sections.push(Section::new_named(
            b".rdata\0\0",
            CHARACTERISTICS_RDATA,
            content,
            vsize,
        ));
    }

    /// Add a .bss section (uninitialized data) with the given size.
    pub fn add_bss(&mut self, size: u32) {
        self.sections.push(Section::new_named(
            b".bss\0\0\0\0",
            CHARACTERISTICS_BSS,
            Vec::new(),
            size,
        ));
    }

    /// Finalize the section table by computing RVAs and file pointers.
    ///
    /// This method:
    /// 1. Aligns the first RVA to `section_alignment` starting after headers.
    /// 2. Aligns the first file pointer to `file_alignment` starting after headers.
    /// 3. For each section:
    ///    - Sets virtual_address (RVA) and aligns it.
    ///    - For BSS: sets size_of_raw_data and pointer_to_raw_data to 0.
    ///    - For others: aligns size_of_raw_data to file_alignment and sets pointer_to_raw_data.
    ///    - Advances RVA and file pointer for the next section.
    pub fn finalize(&mut self, section_alignment: u32, file_alignment: u32, headers_size: u32) {
        let mut rva = align_up(headers_size, section_alignment);
        let mut file_ptr = align_up(headers_size, file_alignment);

        for section in &mut self.sections {
            // Set virtual address (RVA)
            section.header.virtual_address = rva;

            // Check if this is a BSS section (uninitialized data, empty content)
            if section.content.is_empty()
                && section.header.characteristics & IMAGE_SCN_CNT_UNINITIALIZED_DATA != 0
            {
                // BSS section: no raw data
                section.header.size_of_raw_data = 0;
                section.header.pointer_to_raw_data = 0;
            } else {
                // Non-BSS section: allocate raw data space
                let aligned_size = align_up(section.content.len() as u32, file_alignment);
                section.header.size_of_raw_data = aligned_size;
                section.header.pointer_to_raw_data = file_ptr;
                file_ptr += aligned_size;
            }

            // Always advance RVA by virtual size (BSS reserves RVA range too)
            rva += align_up(section.header.virtual_size, section_alignment);
        }
    }

    /// Serialize all section headers to a byte vector.
    pub fn to_bytes_headers(&self) -> Vec<u8> {
        let mut result = Vec::new();
        for section in &self.sections {
            result.extend_from_slice(&section.header.to_bytes());
        }
        result
    }

    /// Serialize all section content to a byte vector with file alignment padding.
    pub fn to_bytes_content(&self, file_alignment: u32) -> Vec<u8> {
        let mut result = Vec::new();

        for section in &self.sections {
            if !section.content.is_empty() {
                result.extend_from_slice(&section.content);
                // Pad to alignment
                let padding = align_up(section.content.len() as u32, file_alignment)
                    - section.content.len() as u32;
                result.extend_from_slice(&vec![0u8; padding as usize]);
            }
        }

        result
    }
}

impl Default for SectionTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_header_size_is_40() {
        assert_eq!(SECTION_HEADER_SIZE, 40);
    }

    #[test]
    fn section_header_round_trips() {
        let original = SectionHeader {
            name: *b".text\0\0\0",
            virtual_size: 0x1000,
            virtual_address: 0x1000,
            size_of_raw_data: 0x200,
            pointer_to_raw_data: 0x400,
            pointer_to_relocations: 0,
            pointer_to_line_numbers: 0,
            number_of_relocations: 0,
            number_of_line_numbers: 0,
            characteristics: CHARACTERISTICS_TEXT,
        };

        let bytes = original.to_bytes();
        let parsed = SectionHeader::from_bytes(&bytes);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn section_table_empty() {
        let table = SectionTable::new();
        assert_eq!(table.sections.len(), 0);
        assert_eq!(table.to_bytes_headers().len(), 0);
        assert_eq!(table.to_bytes_content(0x200).len(), 0);
    }

    #[test]
    fn add_text_finalize_alignment() {
        let mut table = SectionTable::new();
        table.add_text(vec![0u8; 256]);

        let headers_size = 328u32;
        table.finalize(0x1000, 0x400, headers_size);

        let section = &table.sections[0];
        // First RVA should be aligned to 0x1000 after headers_size
        assert_eq!(section.header.virtual_address, 0x1000);
        // First file pointer should be aligned to 0x400 after headers_size
        assert_eq!(section.header.pointer_to_raw_data, 0x400);
    }

    #[test]
    fn three_section_synthetic_finalize() {
        let mut table = SectionTable::new();
        table.add_text(vec![0u8; 256]);
        table.add_data(vec![0u8; 128]);
        table.add_bss(512);

        let headers_size = 328u32;
        table.finalize(0x1000, 0x400, headers_size);

        // Verify text section
        assert_eq!(table.sections[0].header.virtual_address, 0x1000);
        assert_eq!(table.sections[0].header.pointer_to_raw_data, 0x400);
        assert!(
            table.sections[0]
                .header
                .virtual_address
                .is_multiple_of(0x1000)
        );

        // Verify data section
        assert!(
            table.sections[1]
                .header
                .virtual_address
                .is_multiple_of(0x1000)
        );
        assert!(
            table.sections[1]
                .header
                .pointer_to_raw_data
                .is_multiple_of(0x200)
        );

        // Verify BSS section
        assert!(
            table.sections[2]
                .header
                .virtual_address
                .is_multiple_of(0x1000)
        );
        assert_eq!(table.sections[2].header.size_of_raw_data, 0);
        assert_eq!(table.sections[2].header.pointer_to_raw_data, 0);
    }

    #[test]
    fn defaults_match_optional_header() {
        let mut table = SectionTable::new();
        table.add_text(vec![0u8; 100]);

        let section_alignment = 0x1000u32;
        let file_alignment = 0x400u32;
        let headers_size = 328u32;

        table.finalize(section_alignment, file_alignment, headers_size);

        let section = &table.sections[0];
        assert_eq!(section.header.virtual_address, 0x1000);
        assert_eq!(section.header.pointer_to_raw_data, 0x400);
    }

    #[test]
    fn bss_has_zero_raw_data_but_virtual_size() {
        let mut table = SectionTable::new();
        table.add_bss(1024);

        table.finalize(0x1000, 0x200, 328);

        let bss_section = &table.sections[0];
        assert_eq!(bss_section.header.virtual_size, 1024);
        assert_eq!(bss_section.header.size_of_raw_data, 0);
        assert_eq!(bss_section.header.pointer_to_raw_data, 0);
        assert_eq!(bss_section.content.len(), 0);
    }

    #[test]
    fn snapshot_section_header_byte_layout() {
        let header = SectionHeader {
            name: *b".text\0\0\0",
            virtual_size: 0x1000,
            virtual_address: 0x1000,
            size_of_raw_data: 0x200,
            pointer_to_raw_data: 0x400,
            pointer_to_relocations: 0,
            pointer_to_line_numbers: 0,
            number_of_relocations: 0,
            number_of_line_numbers: 0,
            characteristics: CHARACTERISTICS_TEXT,
        };

        let bytes = header.to_bytes();

        // Check name at offset 0-7
        assert_eq!(&bytes[0..8], b".text\0\0\0");

        // Check virtual_size at offset 8-11 (little-endian 0x1000)
        let vsize = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(vsize, 0x1000);

        // Check characteristics at offset 36-39 (little-endian CHARACTERISTICS_TEXT)
        let chars = u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]);
        assert_eq!(chars, CHARACTERISTICS_TEXT);
    }
}
