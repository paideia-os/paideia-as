//! `.relocs` section content: relocation entries.
//!
//! Each entry is 32 bytes:
//!
//! | Offset | Size | Field           |
//! |--------|------|-----------------|
//! | 0      | 8    | offset          |
//! | 8      | 8    | symbol_index    |
//! | 16     | 4    | reloc_kind      |
//! | 20     | 4    | addend_lo       |
//! | 24     | 4    | addend_hi       |
//! | 28     | 4    | reserved        |

use static_assertions::const_assert_eq;

/// Size of a single relocation entry in bytes.
pub const RELOC_ENTRY_SIZE: usize = 32;

// Verify the relocation entry size is correct at compile time.
const_assert_eq!(RELOC_ENTRY_SIZE, 32);

/// Relocation kind.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RelocKind {
    /// Absolute 64-bit address.
    Abs64 = 1,
    /// PC-relative 32-bit offset.
    Pc32 = 2,
    /// GOT-relative 32-bit offset.
    GotPc32 = 3,
    /// PLT-relative 32-bit offset.
    PltPc32 = 4,
    /// PaideiaOS capability-binding relocation.
    CapBind = 0x100,
}

/// A single relocation entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelocEntry {
    /// Offset within the target section.
    pub offset: u64,
    /// Index into the symbol table.
    pub symbol_index: u64,
    /// Relocation kind.
    pub kind: RelocKind,
    /// Addend (signed 64-bit).
    pub addend: i64,
}

impl RelocEntry {
    /// Create a new relocation entry.
    ///
    /// # Arguments
    ///
    /// * `offset` - Offset within the target section
    /// * `symbol_index` - Index into the symbol table
    /// * `kind` - Relocation kind
    /// * `addend` - Addend (signed 64-bit)
    ///
    /// # Returns
    ///
    /// A new RelocEntry.
    pub fn new(offset: u64, symbol_index: u64, kind: RelocKind, addend: i64) -> Self {
        Self {
            offset,
            symbol_index,
            kind,
            addend,
        }
    }

    /// Serialize this entry to its 32-byte little-endian representation.
    ///
    /// The addend (i64) is split into two u32 parts: addend_lo and addend_hi.
    pub fn to_bytes(&self) -> [u8; RELOC_ENTRY_SIZE] {
        let mut bytes = [0u8; RELOC_ENTRY_SIZE];

        // Offset 0: offset (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.offset.to_le_bytes());

        // Offset 8: symbol_index (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.symbol_index.to_le_bytes());

        // Offset 16: reloc_kind (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&(self.kind as u32).to_le_bytes());

        // Offset 20..28: addend split into two u32 parts (little-endian)
        let addend_bytes = self.addend.to_le_bytes();
        bytes[20..24].copy_from_slice(&addend_bytes[0..4]);
        bytes[24..28].copy_from_slice(&addend_bytes[4..8]);

        // Offset 28: reserved (4 bytes)
        // bytes[28..32] already zeroed

        bytes
    }

    /// Parse a relocation entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least RELOC_ENTRY_SIZE bytes
    /// and the reloc_kind is valid. Returns `None` on invalid enum value or
    /// short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < RELOC_ENTRY_SIZE {
            return None;
        }

        // Offset 0: offset (8 bytes, little-endian)
        let offset = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: symbol_index (8 bytes, little-endian)
        let symbol_index = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: reloc_kind (4 bytes, little-endian)
        let kind_u32 = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let kind = match kind_u32 {
            1 => RelocKind::Abs64,
            2 => RelocKind::Pc32,
            3 => RelocKind::GotPc32,
            4 => RelocKind::PltPc32,
            0x100 => RelocKind::CapBind,
            _ => return None,
        };

        // Offset 20..28: addend split from two u32 parts
        let mut addend_bytes = [0u8; 8];
        addend_bytes[0..4].copy_from_slice(&bytes[20..24]);
        addend_bytes[4..8].copy_from_slice(&bytes[24..28]);
        let addend = i64::from_le_bytes(addend_bytes);

        Some(Self {
            offset,
            symbol_index,
            kind,
            addend,
        })
    }
}

/// Whole-section content: a sequence of RelocEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct Relocs {
    /// List of relocation entries.
    pub entries: Vec<RelocEntry>,
}

impl Relocs {
    /// Create a new, empty relocation section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relocation entry to the section.
    pub fn push(&mut self, e: RelocEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to RELOC_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * RELOC_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse a relocation section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of RELOC_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(RELOC_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / RELOC_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * RELOC_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + RELOC_ENTRY_SIZE];
            let entry = RelocEntry::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the section.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reloc_entry_size_is_32_bytes() {
        assert_eq!(RELOC_ENTRY_SIZE, 32);
    }

    #[test]
    fn single_abs64_reloc_roundtrip() {
        let entry = RelocEntry::new(0x1000, 0, RelocKind::Abs64, 0x100);

        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), RELOC_ENTRY_SIZE);

        let parsed = RelocEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn single_capbind_reloc_with_negative_addend_roundtrip() {
        let entry = RelocEntry::new(0x2000, 5, RelocKind::CapBind, -0x50);

        let bytes = entry.to_bytes();
        let parsed = RelocEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn relocs_section_3_entries_roundtrip() {
        let mut section = Relocs::new();

        let e1 = RelocEntry::new(0x1000, 0, RelocKind::Abs64, 0);
        let e2 = RelocEntry::new(0x2000, 1, RelocKind::Pc32, -16);
        let e3 = RelocEntry::new(0x3000, 2, RelocKind::CapBind, 256);

        section.push(e1);
        section.push(e2);
        section.push(e3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * RELOC_ENTRY_SIZE);

        let parsed = Relocs::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn schema_snapshot_byte_layout() {
        let entry = RelocEntry {
            offset: 0x0102030405060708,
            symbol_index: 0x0A0B0C0D0E0F1011,
            kind: RelocKind::Pc32,
            addend: 0x1213141516171819i64,
        };

        let bytes = entry.to_bytes();

        // Offset 0..8: offset
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8..16: symbol_index
        assert_eq!(
            &bytes[8..16],
            &[0x11u8, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A]
        );

        // Offset 16..20: reloc_kind (RelocKind::Pc32 = 2)
        assert_eq!(&bytes[16..20], &[0x02u8, 0x00, 0x00, 0x00]);

        // Offset 20..28: addend (0x1213141516171819 as i64, little-endian)
        assert_eq!(
            &bytes[20..28],
            &[0x19u8, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12]
        );

        // Offset 28..32: reserved
        assert_eq!(&bytes[28..32], &[0x00u8, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn from_bytes_rejects_truncated_input() {
        let bytes = [0u8; RELOC_ENTRY_SIZE - 1];
        let result = RelocEntry::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn empty_relocs_roundtrips() {
        let section = Relocs::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = Relocs::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn every_reloc_kind_round_trips() {
        let kinds = [
            RelocKind::Abs64,
            RelocKind::Pc32,
            RelocKind::GotPc32,
            RelocKind::PltPc32,
            RelocKind::CapBind,
        ];

        for expected_kind in kinds {
            let entry = RelocEntry::new(0x1000, 0, expected_kind, 0);

            let bytes = entry.to_bytes();
            let parsed = RelocEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.kind, expected_kind);
        }
    }

    #[test]
    fn addend_boundary_values_roundtrip() {
        let test_cases = vec![
            i64::MIN,
            -1,
            0,
            1,
            i64::MAX,
            -0x1234567890ABCDEF,
            0x1234567890ABCDEF,
        ];

        for addend in test_cases {
            let entry = RelocEntry::new(0x1000, 0, RelocKind::Abs64, addend);

            let bytes = entry.to_bytes();
            let parsed = RelocEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(
                parsed.addend, addend,
                "Addend mismatch for value {}",
                addend
            );
        }
    }
}
