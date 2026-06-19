//! PAX section table.
//!
//! Each section descriptor is 64 bytes (canonical, fixed-size):
//!
//! | Offset | Size | Field                |
//! |--------|------|----------------------|
//! | 0      | 4    | section_type         |
//! | 4      | 4    | flags                |
//! | 8      | 8    | content_offset       |
//! | 16     | 8    | content_size         |
//! | 24     | 8    | virtual_address      |
//! | 32     | 8    | alignment            |
//! | 40     | 24   | name (NUL-terminated UTF-8) |

use static_assertions::const_assert_eq;

/// Size of a single section descriptor in bytes.
pub const SECTION_DESCRIPTOR_SIZE: usize = 64;

/// Maximum bytes reserved for section name (NUL-terminated UTF-8).
pub const SECTION_NAME_MAX: usize = 24;

// Verify the section descriptor size is correct at compile time.
const_assert_eq!(SECTION_DESCRIPTOR_SIZE, 64);

/// Section flags, composable via bitwise OR.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum SectionFlag {
    /// No flags set.
    None = 0,
    /// Section is executable.
    Executable = 0x1,
    /// Section is writable.
    Writable = 0x2,
    /// Section contains no actual content (BSS-like).
    BssNoContent = 0x4,
}

/// Section type identifiers.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SectionType {
    /// Executable code section.
    Code = 0x01,
    /// Read-only data section.
    RoData = 0x02,
    /// Initialized data section.
    Data = 0x03,
    /// Uninitialized data section (BSS).
    Bss = 0x04,
    /// Capability annotations (PaideiaOS-specific, m4-003..006).
    Caps = 0x10,
    /// Effect annotations (PaideiaOS-specific).
    Effects = 0x11,
    /// Unsafe code annotations (PaideiaOS-specific).
    Unsafe = 0x12,
    /// Optimization passes metadata (PaideiaOS-specific).
    OptPasses = 0x13,
    /// Linearity annotations (PaideiaOS-specific).
    Linearity = 0x14,
    /// Functor bindings (PaideiaOS-specific).
    Functors = 0x15,
    /// Symbol table (PaideiaOS-specific).
    Symtab = 0x20,
    /// Relocation entries (PaideiaOS-specific).
    Relocs = 0x21,
    /// Import symbols (PaideiaOS-specific).
    Imports = 0x22,
    /// Export symbols (PaideiaOS-specific).
    Exports = 0x23,
}

/// A PAX section descriptor.
#[derive(Clone, Debug)]
pub struct Section {
    /// Type of this section.
    pub ty: SectionType,
    /// Flags composing section attributes.
    pub flags: u32,
    /// Absolute offset to the section content in the file.
    pub content_offset: u64,
    /// Size of the section content in bytes.
    pub content_size: u64,
    /// Virtual address where this section is loaded (currently unused; reserved for future).
    pub virtual_address: u64,
    /// Alignment requirement for this section in bytes.
    pub alignment: u64,
    /// Human-readable section name (up to SECTION_NAME_MAX bytes when serialized).
    pub name: String,
}

impl Section {
    /// Create a new `.code` (executable) section.
    ///
    /// # Arguments
    ///
    /// * `content_offset` - Absolute offset to the code content in the file
    /// * `content_size` - Size of the code content in bytes
    ///
    /// # Returns
    ///
    /// A new `.code` section with EXECUTABLE flag and 16-byte alignment.
    pub fn code(content_offset: u64, content_size: u64) -> Self {
        Self {
            ty: SectionType::Code,
            flags: SectionFlag::Executable as u32,
            content_offset,
            content_size,
            virtual_address: 0,
            alignment: 16,
            name: ".code".to_owned(),
        }
    }

    /// Create a new `.rodata` (read-only data) section.
    ///
    /// # Arguments
    ///
    /// * `content_offset` - Absolute offset to the data content in the file
    /// * `content_size` - Size of the data content in bytes
    ///
    /// # Returns
    ///
    /// A new `.rodata` section with no flags and 8-byte alignment.
    pub fn rodata(content_offset: u64, content_size: u64) -> Self {
        Self {
            ty: SectionType::RoData,
            flags: SectionFlag::None as u32,
            content_offset,
            content_size,
            virtual_address: 0,
            alignment: 8,
            name: ".rodata".to_owned(),
        }
    }

    /// Create a new `.data` (initialized data) section.
    ///
    /// # Arguments
    ///
    /// * `content_offset` - Absolute offset to the data content in the file
    /// * `content_size` - Size of the data content in bytes
    ///
    /// # Returns
    ///
    /// A new `.data` section with WRITABLE flag and 8-byte alignment.
    pub fn data(content_offset: u64, content_size: u64) -> Self {
        Self {
            ty: SectionType::Data,
            flags: SectionFlag::Writable as u32,
            content_offset,
            content_size,
            virtual_address: 0,
            alignment: 8,
            name: ".data".to_owned(),
        }
    }

    /// Create a new `.bss` (uninitialized data) section.
    ///
    /// # Arguments
    ///
    /// * `content_size` - Size of the BSS area in bytes
    ///
    /// # Returns
    ///
    /// A new `.bss` section with WRITABLE and BSS_NO_CONTENT flags, no content offset,
    /// and 8-byte alignment.
    pub fn bss(content_size: u64) -> Self {
        Self {
            ty: SectionType::Bss,
            flags: (SectionFlag::Writable as u32) | (SectionFlag::BssNoContent as u32),
            content_offset: 0,
            content_size,
            virtual_address: 0,
            alignment: 8,
            name: ".bss".to_owned(),
        }
    }

    /// Serialize this section to its 64-byte little-endian representation.
    ///
    /// Returns a fixed-size array matching the canonical PAX section descriptor layout.
    /// The name field is NUL-terminated and truncated to SECTION_NAME_MAX bytes.
    pub fn to_bytes(&self) -> [u8; SECTION_DESCRIPTOR_SIZE] {
        let mut bytes = [0u8; SECTION_DESCRIPTOR_SIZE];

        // Offset 0: section_type (4 bytes, little-endian)
        bytes[0..4].copy_from_slice(&(self.ty as u32).to_le_bytes());

        // Offset 4: flags (4 bytes, little-endian)
        bytes[4..8].copy_from_slice(&self.flags.to_le_bytes());

        // Offset 8: content_offset (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.content_offset.to_le_bytes());

        // Offset 16: content_size (8 bytes, little-endian)
        bytes[16..24].copy_from_slice(&self.content_size.to_le_bytes());

        // Offset 24: virtual_address (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.virtual_address.to_le_bytes());

        // Offset 32: alignment (8 bytes, little-endian)
        bytes[32..40].copy_from_slice(&self.alignment.to_le_bytes());

        // Offset 40: name (24 bytes, NUL-terminated UTF-8)
        let name_bytes = self.name.as_bytes();
        let name_len = std::cmp::min(name_bytes.len(), SECTION_NAME_MAX - 1);
        bytes[40..40 + name_len].copy_from_slice(&name_bytes[0..name_len]);
        bytes[40 + name_len] = 0; // NUL terminator

        bytes
    }

    /// Parse a PAX section descriptor from a byte slice.
    ///
    /// Returns `Some(section)` if the input contains at least SECTION_DESCRIPTOR_SIZE bytes
    /// and the section_type is a valid enum variant. Returns `None` on invalid type or
    /// short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < SECTION_DESCRIPTOR_SIZE {
            return None;
        }

        // Offset 0: section_type (4 bytes, little-endian)
        let ty_u32 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let ty = match ty_u32 {
            0x01 => SectionType::Code,
            0x02 => SectionType::RoData,
            0x03 => SectionType::Data,
            0x04 => SectionType::Bss,
            0x10 => SectionType::Caps,
            0x11 => SectionType::Effects,
            0x12 => SectionType::Unsafe,
            0x13 => SectionType::OptPasses,
            0x14 => SectionType::Linearity,
            0x15 => SectionType::Functors,
            0x20 => SectionType::Symtab,
            0x21 => SectionType::Relocs,
            0x22 => SectionType::Imports,
            0x23 => SectionType::Exports,
            _ => return None,
        };

        // Offset 4: flags (4 bytes, little-endian)
        let flags = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        // Offset 8: content_offset (8 bytes, little-endian)
        let content_offset = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: content_size (8 bytes, little-endian)
        let content_size = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        // Offset 24: virtual_address (8 bytes, little-endian)
        let virtual_address = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        // Offset 32: alignment (8 bytes, little-endian)
        let alignment = u64::from_le_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
        ]);

        // Offset 40: name (24 bytes, NUL-terminated UTF-8)
        let name_bytes = &bytes[40..64];
        let name_str = match std::ffi::CStr::from_bytes_until_nul(name_bytes) {
            Ok(cstr) => match cstr.to_str() {
                Ok(s) => s.to_owned(),
                Err(_) => return None, // Invalid UTF-8
            },
            Err(_) => {
                // No NUL terminator found within 24 bytes; treat all as valid
                // but still convert to string
                match std::str::from_utf8(name_bytes) {
                    Ok(s) => s.to_owned(),
                    Err(_) => return None,
                }
            }
        };

        Some(Self {
            ty,
            flags,
            content_offset,
            content_size,
            virtual_address,
            alignment,
            name: name_str,
        })
    }
}

impl PartialEq for Section {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty
            && self.flags == other.flags
            && self.content_offset == other.content_offset
            && self.content_size == other.content_size
            && self.virtual_address == other.virtual_address
            && self.alignment == other.alignment
            && self.name == other.name
    }
}

/// PAX section table: a sequence of section descriptors.
///
/// Serialized at `PaxHeader.section_table_offset`, with `PaxHeader.section_count` entries.
#[derive(Clone, Debug)]
pub struct SectionTable {
    /// List of sections in this table.
    pub sections: Vec<Section>,
}

impl SectionTable {
    /// Create a new, empty section table.
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Add a section to the table.
    pub fn push(&mut self, section: Section) {
        self.sections.push(section);
    }

    /// Return the number of sections in the table.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Check if the section table is empty.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Serialize the section table to a byte vector.
    ///
    /// Each section is serialized to SECTION_DESCRIPTOR_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.sections.len() * SECTION_DESCRIPTOR_SIZE);
        for section in &self.sections {
            bytes.extend_from_slice(&section.to_bytes());
        }
        bytes
    }

    /// Parse a PAX section table from a byte slice.
    ///
    /// Returns `Some(table)` if the input contains at least `count * SECTION_DESCRIPTOR_SIZE` bytes
    /// and all section descriptors parse successfully. Returns `None` on short input or invalid
    /// section descriptors.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The byte buffer containing section descriptors
    /// * `count` - The number of sections to parse
    pub fn from_bytes(bytes: &[u8], count: u32) -> Option<Self> {
        let count = count as usize;
        let required_len = count * SECTION_DESCRIPTOR_SIZE;
        if bytes.len() < required_len {
            return None;
        }

        let mut sections = Vec::with_capacity(count);
        for i in 0..count {
            let offset = i * SECTION_DESCRIPTOR_SIZE;
            let section_bytes = &bytes[offset..offset + SECTION_DESCRIPTOR_SIZE];
            let section = Section::from_bytes(section_bytes)?;
            sections.push(section);
        }

        Some(Self { sections })
    }
}

impl Default for SectionTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for SectionTable {
    fn eq(&self, other: &Self) -> bool {
        self.sections == other.sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_descriptor_size_is_64_bytes() {
        // Const assertion: this will not compile if SECTION_DESCRIPTOR_SIZE != 64.
        const _: () = {
            const _: [(); 1] = [(); (SECTION_DESCRIPTOR_SIZE == 64) as usize];
        };
        assert_eq!(SECTION_DESCRIPTOR_SIZE, 64);
    }

    #[test]
    fn section_code_constructor_sets_executable_flag_and_align_16() {
        let section = Section::code(100, 256);
        assert_eq!(section.ty, SectionType::Code);
        assert_eq!(section.flags, SectionFlag::Executable as u32);
        assert_eq!(section.alignment, 16);
        assert_eq!(section.content_offset, 100);
        assert_eq!(section.content_size, 256);
        assert_eq!(section.name, ".code");
    }

    #[test]
    fn section_rodata_constructor_correct() {
        let section = Section::rodata(400, 128);
        assert_eq!(section.ty, SectionType::RoData);
        assert_eq!(section.flags, SectionFlag::None as u32);
        assert_eq!(section.alignment, 8);
        assert_eq!(section.content_offset, 400);
        assert_eq!(section.content_size, 128);
        assert_eq!(section.name, ".rodata");
    }

    #[test]
    fn section_data_constructor_sets_writable() {
        let section = Section::data(600, 64);
        assert_eq!(section.ty, SectionType::Data);
        assert_eq!(section.flags, SectionFlag::Writable as u32);
        assert_eq!(section.alignment, 8);
        assert_eq!(section.content_offset, 600);
        assert_eq!(section.content_size, 64);
        assert_eq!(section.name, ".data");
    }

    #[test]
    fn section_bss_constructor_sets_no_content_and_writable() {
        let section = Section::bss(256);
        assert_eq!(section.ty, SectionType::Bss);
        assert_eq!(
            section.flags,
            (SectionFlag::Writable as u32) | (SectionFlag::BssNoContent as u32)
        );
        assert_eq!(section.content_offset, 0);
        assert_eq!(section.content_size, 256);
        assert_eq!(section.alignment, 8);
        assert_eq!(section.name, ".bss");
    }

    #[test]
    fn section_to_bytes_round_trips() {
        let original = Section::code(100, 256);
        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), SECTION_DESCRIPTOR_SIZE);
        let parsed = Section::from_bytes(&bytes);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn section_name_truncated_to_24_bytes() {
        let long_name = ".code.very.long.section.name.that.exceeds.limit";
        let section = {
            let mut s = Section::code(100, 256);
            s.name = long_name.to_owned();
            s
        };

        let bytes = section.to_bytes();

        // Name field starts at offset 40, has 24 bytes
        let name_field = &bytes[40..64];

        // Verify that the name is truncated and NUL-terminated
        let name_str = std::ffi::CStr::from_bytes_until_nul(name_field)
            .expect("Name field should be NUL-terminated")
            .to_str()
            .expect("Name field should be valid UTF-8");

        // The name should be truncated to at most 23 bytes (plus NUL)
        assert!(name_str.len() < SECTION_NAME_MAX);
        assert!(name_str.starts_with(".code"));
    }

    #[test]
    fn section_table_3_sections_serialises_correctly() {
        let mut table = SectionTable::new();
        table.push(Section::code(96 + 192, 512)); // After header and table
        table.push(Section::rodata(96 + 192 + 512, 256));
        table.push(Section::bss(1024));

        let bytes = table.to_bytes();
        assert_eq!(bytes.len(), 3 * SECTION_DESCRIPTOR_SIZE);

        let parsed = SectionTable::from_bytes(&bytes, 3);
        assert_eq!(parsed, Some(table));
    }

    #[test]
    fn section_table_alignment_respects_flags() {
        let section = Section::code(100, 256);
        assert_eq!(section.alignment, 16);

        let bytes = section.to_bytes();
        let parsed = Section::from_bytes(&bytes).expect("Failed to parse section");
        assert_eq!(parsed.alignment, 16);
    }

    #[test]
    fn snapshot_section_table_byte_layout() {
        let section = Section::code(100, 256);
        let bytes = section.to_bytes();

        // Verify section_type (0x01) at offset 0
        assert_eq!(&bytes[0..4], &[1u8, 0, 0, 0]);

        // Verify flags (0x1 = EXECUTABLE) at offset 4
        assert_eq!(&bytes[4..8], &[1u8, 0, 0, 0]);

        // Verify content_offset (100) at offset 8
        assert_eq!(&bytes[8..16], &[100u8, 0, 0, 0, 0, 0, 0, 0]);

        // Verify content_size (256) at offset 16
        assert_eq!(&bytes[16..24], &[0u8, 1, 0, 0, 0, 0, 0, 0]);

        // Verify virtual_address (0) at offset 24
        assert_eq!(&bytes[24..32], &[0u8; 8]);

        // Verify alignment (16) at offset 32
        assert_eq!(&bytes[32..40], &[16u8, 0, 0, 0, 0, 0, 0, 0]);

        // Verify name ".code\0" at offset 40
        let name_field = &bytes[40..64];
        assert_eq!(name_field[0..5], *b".code");
        assert_eq!(name_field[5], 0); // NUL terminator
    }

    #[test]
    fn section_all_types_parse_correctly() {
        let types = [
            SectionType::Code,
            SectionType::RoData,
            SectionType::Data,
            SectionType::Bss,
            SectionType::Caps,
            SectionType::Effects,
            SectionType::Unsafe,
            SectionType::OptPasses,
            SectionType::Linearity,
            SectionType::Functors,
            SectionType::Symtab,
            SectionType::Relocs,
            SectionType::Imports,
            SectionType::Exports,
        ];

        for expected_ty in types {
            let mut section = Section::code(0, 0);
            section.ty = expected_ty;
            let bytes = section.to_bytes();
            let parsed = Section::from_bytes(&bytes).expect("Failed to parse section");
            assert_eq!(parsed.ty, expected_ty);
        }
    }

    #[test]
    fn section_table_empty_is_valid() {
        let table = SectionTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let bytes = table.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = SectionTable::from_bytes(&bytes, 0);
        assert_eq!(parsed, Some(table));
    }

    #[test]
    fn section_from_bytes_rejects_invalid_type() {
        let mut bytes = [0u8; SECTION_DESCRIPTOR_SIZE];
        // Set an invalid section type
        bytes[0..4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        let result = Section::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn section_from_bytes_rejects_short_input() {
        let bytes = [0u8; 32];
        let result = Section::from_bytes(&bytes);
        assert_eq!(result, None);
    }
}
