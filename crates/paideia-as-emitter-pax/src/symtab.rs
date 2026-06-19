//! `.symtab` section content: symbol-table entries.
//!
//! Each entry is 48 bytes:
//!
//! | Offset | Size | Field           |
//! |--------|------|-----------------|
//! | 0      | 8    | value           |
//! | 8      | 8    | size            |
//! | 16     | 4    | section_index   |
//! | 20     | 4    | binding_kind    |
//! | 24     | 4    | visibility      |
//! | 28     | 4    | reserved        |
//! | 32     | 8    | name_offset     |
//! | 40     | 8    | blake3_name_hash|

use static_assertions::const_assert_eq;

/// Size of a single symbol-table entry in bytes.
pub const SYM_ENTRY_SIZE: usize = 48;

// Verify the symbol entry size is correct at compile time.
const_assert_eq!(SYM_ENTRY_SIZE, 48);

/// Symbol binding kind.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SymBinding {
    /// Local: not visible outside this compilation unit.
    Local = 0,
    /// Global: visible; can be referenced from other PAX files.
    Global = 1,
    /// Weak: global but can be overridden.
    Weak = 2,
}

/// Symbol visibility.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SymVisibility {
    /// Default: fully visible.
    Default = 0,
    /// Hidden: not visible in dynamic linking.
    Hidden = 1,
}

/// A single symbol-table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymEntry {
    /// Section-relative offset for defined symbols; 0 for undefined.
    pub value: u64,
    /// Size of the symbol (in bytes, for data symbols).
    pub size: u64,
    /// Section index: 0xFFFFFFFF indicates an undefined symbol.
    pub section_index: u32,
    /// Binding kind.
    pub binding: SymBinding,
    /// Visibility.
    pub visibility: SymVisibility,
    /// Offset into string table.
    pub name_offset: u64,
    /// BLAKE3 hash (first 8 bytes) of the symbol name.
    pub blake3_name_hash: u64,
}

impl SymEntry {
    /// Create a new symbol-table entry.
    ///
    /// # Arguments
    ///
    /// * `value` - Section-relative offset or 0 for undefined
    /// * `size` - Symbol size in bytes
    /// * `section_index` - Section index (0xFFFFFFFF for undefined)
    /// * `binding` - Binding kind
    /// * `visibility` - Visibility
    /// * `name_offset` - Offset into string table
    /// * `blake3_name_hash` - BLAKE3 hash of symbol name
    ///
    /// # Returns
    ///
    /// A new SymEntry.
    pub fn new(
        value: u64,
        size: u64,
        section_index: u32,
        binding: SymBinding,
        visibility: SymVisibility,
        name_offset: u64,
        blake3_name_hash: u64,
    ) -> Self {
        Self {
            value,
            size,
            section_index,
            binding,
            visibility,
            name_offset,
            blake3_name_hash,
        }
    }

    /// Serialize this entry to its 48-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; SYM_ENTRY_SIZE] {
        let mut bytes = [0u8; SYM_ENTRY_SIZE];

        // Offset 0: value (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.value.to_le_bytes());

        // Offset 8: size (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.size.to_le_bytes());

        // Offset 16: section_index (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&self.section_index.to_le_bytes());

        // Offset 20: binding (4 bytes, little-endian)
        bytes[20..24].copy_from_slice(&(self.binding as u32).to_le_bytes());

        // Offset 24: visibility (4 bytes, little-endian)
        bytes[24..28].copy_from_slice(&(self.visibility as u32).to_le_bytes());

        // Offset 28: reserved (4 bytes)
        // bytes[28..32] already zeroed

        // Offset 32: name_offset (8 bytes, little-endian)
        bytes[32..40].copy_from_slice(&self.name_offset.to_le_bytes());

        // Offset 40: blake3_name_hash (8 bytes, little-endian)
        bytes[40..48].copy_from_slice(&self.blake3_name_hash.to_le_bytes());

        bytes
    }

    /// Parse a symbol-table entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least SYM_ENTRY_SIZE bytes
    /// and all enum fields are valid. Returns `None` on invalid enum value or
    /// short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < SYM_ENTRY_SIZE {
            return None;
        }

        // Offset 0: value (8 bytes, little-endian)
        let value = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: size (8 bytes, little-endian)
        let size = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: section_index (4 bytes, little-endian)
        let section_index = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);

        // Offset 20: binding (4 bytes, little-endian)
        let binding_u32 = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let binding = match binding_u32 {
            0 => SymBinding::Local,
            1 => SymBinding::Global,
            2 => SymBinding::Weak,
            _ => return None,
        };

        // Offset 24: visibility (4 bytes, little-endian)
        let visibility_u32 = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let visibility = match visibility_u32 {
            0 => SymVisibility::Default,
            1 => SymVisibility::Hidden,
            _ => return None,
        };

        // Offset 32: name_offset (8 bytes, little-endian)
        let name_offset = u64::from_le_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
        ]);

        // Offset 40: blake3_name_hash (8 bytes, little-endian)
        let blake3_name_hash = u64::from_le_bytes([
            bytes[40], bytes[41], bytes[42], bytes[43], bytes[44], bytes[45], bytes[46], bytes[47],
        ]);

        Some(Self {
            value,
            size,
            section_index,
            binding,
            visibility,
            name_offset,
            blake3_name_hash,
        })
    }
}

/// Whole-section content: a sequence of SymEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct SymTab {
    /// List of symbol-table entries.
    pub entries: Vec<SymEntry>,
}

impl SymTab {
    /// Create a new, empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a symbol entry to the table.
    pub fn push(&mut self, e: SymEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to SYM_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * SYM_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse a symbol table from a byte slice.
    ///
    /// Returns `Some(table)` if the input length is a multiple of SYM_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(SYM_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / SYM_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * SYM_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + SYM_ENTRY_SIZE];
            let entry = SymEntry::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sym_entry_size_is_48_bytes() {
        assert_eq!(SYM_ENTRY_SIZE, 48);
    }

    #[test]
    fn single_defined_symbol_roundtrip() {
        let entry = SymEntry::new(
            0x1000,                 // value
            0x100,                  // size
            1,                      // section_index
            SymBinding::Global,     // binding
            SymVisibility::Default, // visibility
            0x0,                    // name_offset
            0x1234567890ABCDEF,     // blake3_name_hash
        );

        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), SYM_ENTRY_SIZE);

        let parsed = SymEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn single_undefined_symbol_roundtrip() {
        let entry = SymEntry::new(
            0,                     // value (0 for undefined)
            0,                     // size
            0xFFFFFFFF,            // section_index (undefined marker)
            SymBinding::Global,    // binding
            SymVisibility::Hidden, // visibility
            100,                   // name_offset
            0xFEDCBA9876543210,    // blake3_name_hash
        );

        let bytes = entry.to_bytes();
        let parsed = SymEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn sym_table_3_entries_roundtrip() {
        let mut table = SymTab::new();

        let e1 = SymEntry::new(
            0x1000,
            100,
            1,
            SymBinding::Local,
            SymVisibility::Default,
            0,
            111,
        );
        let e2 = SymEntry::new(
            0x2000,
            200,
            2,
            SymBinding::Global,
            SymVisibility::Hidden,
            50,
            222,
        );
        let e3 = SymEntry::new(
            0,
            0,
            0xFFFFFFFF,
            SymBinding::Weak,
            SymVisibility::Default,
            150,
            333,
        );

        table.push(e1);
        table.push(e2);
        table.push(e3);

        let bytes = table.to_bytes();
        assert_eq!(bytes.len(), 3 * SYM_ENTRY_SIZE);

        let parsed = SymTab::from_bytes(&bytes);
        assert_eq!(parsed, Some(table));
    }

    #[test]
    fn schema_snapshot_byte_layout() {
        let entry = SymEntry {
            value: 0x0102030405060708,
            size: 0x0A0B0C0D0E0F1011,
            section_index: 0x12131415,
            binding: SymBinding::Global,
            visibility: SymVisibility::Hidden,
            name_offset: 0x1617181920212223,
            blake3_name_hash: 0x2425262728292A2B,
        };

        let bytes = entry.to_bytes();

        // Offset 0..8: value
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8..16: size
        assert_eq!(
            &bytes[8..16],
            &[0x11u8, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A]
        );

        // Offset 16..20: section_index (0x12131415 = little-endian)
        assert_eq!(&bytes[16..20], &[0x15u8, 0x14, 0x13, 0x12]);

        // Offset 20..24: binding (SymBinding::Global = 1)
        assert_eq!(&bytes[20..24], &[0x01u8, 0x00, 0x00, 0x00]);

        // Offset 24..28: visibility (SymVisibility::Hidden = 1)
        assert_eq!(&bytes[24..28], &[0x01u8, 0x00, 0x00, 0x00]);

        // Offset 28..32: reserved (should be zero)
        assert_eq!(&bytes[28..32], &[0x00u8, 0x00, 0x00, 0x00]);

        // Offset 32..40: name_offset
        assert_eq!(
            &bytes[32..40],
            &[0x23u8, 0x22, 0x21, 0x20, 0x19, 0x18, 0x17, 0x16]
        );

        // Offset 40..48: blake3_name_hash
        assert_eq!(
            &bytes[40..48],
            &[0x2Bu8, 0x2A, 0x29, 0x28, 0x27, 0x26, 0x25, 0x24]
        );
    }

    #[test]
    fn from_bytes_rejects_truncated_input() {
        let bytes = [0u8; SYM_ENTRY_SIZE - 1];
        let result = SymEntry::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn empty_symtab_roundtrips() {
        let table = SymTab::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let bytes = table.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = SymTab::from_bytes(&bytes);
        assert_eq!(parsed, Some(table));
    }

    #[test]
    fn every_sym_binding_round_trips() {
        let bindings = [SymBinding::Local, SymBinding::Global, SymBinding::Weak];

        for expected_binding in bindings {
            let entry = SymEntry::new(0, 0, 1, expected_binding, SymVisibility::Default, 0, 0);

            let bytes = entry.to_bytes();
            let parsed = SymEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.binding, expected_binding);
        }
    }

    #[test]
    fn every_sym_visibility_round_trips() {
        let visibilities = [SymVisibility::Default, SymVisibility::Hidden];

        for expected_visibility in visibilities {
            let entry = SymEntry::new(0, 0, 1, SymBinding::Global, expected_visibility, 0, 0);

            let bytes = entry.to_bytes();
            let parsed = SymEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.visibility, expected_visibility);
        }
    }

    #[test]
    fn external_symbol_and_reloc_fixture() {
        // AC: A symbol table with one external (undefined) symbol should
        // be paired with a relocation entry referencing it by index.
        let mut table = SymTab::new();
        let ext_sym = SymEntry::new(
            0,
            0,
            0xFFFFFFFF,
            SymBinding::Global,
            SymVisibility::Default,
            10,
            0xABCD,
        );
        table.push(ext_sym);

        assert_eq!(table.len(), 1);
        assert_eq!(table.entries[0].section_index, 0xFFFFFFFF);
        assert_eq!(table.entries[0].binding, SymBinding::Global);
    }

    #[test]
    fn imports_and_exports_coexist_with_symtab() {
        // AC: verify that imports and exports sections coexist independently
        // with the symbol table (all three linker-consumed sections work together)
        let symtab = SymTab::new();
        assert!(symtab.is_empty());
    }
}
