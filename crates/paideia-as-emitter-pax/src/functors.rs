//! `.paideia.functors` section content: per-functor entry descriptors.
//!
//! Each entry is 40 bytes:
//!
//! | Offset | Size | Field               |
//! |--------|------|---------------------|
//! | 0      | 8    | functor_symbol_id   |
//! | 8      | 8    | param_signature_hash|
//! | 16     | 8    | result_signature_hash|
//! | 24     | 8    | closure_data_offset |
//! | 32     | 4    | closure_data_size   |
//! | 36     | 4    | flags               |

use static_assertions::const_assert_eq;

/// Size of a single functor entry in bytes.
pub const FUNCTOR_ENTRY_SIZE: usize = 40;

// Verify the functor entry size is correct at compile time.
const_assert_eq!(FUNCTOR_ENTRY_SIZE, 40);

/// A single functor binding entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctorEntry {
    /// Symbol table id for the functor.
    pub functor_symbol_id: u64,
    /// Hash of the parameter signature.
    pub param_signature_hash: u64,
    /// Hash of the result signature.
    pub result_signature_hash: u64,
    /// Offset to closure data (phase-2+; placeholder in m5-011).
    pub closure_data_offset: u64,
    /// Size of closure data (phase-2+; placeholder in m5-011).
    pub closure_data_size: u32,
    /// Flags for the functor (phase-2+; placeholder in m5-011).
    pub flags: u32,
}

impl FunctorEntry {
    /// Serialize this entry to its 40-byte little-endian representation.
    ///
    /// Returns a fixed-size array matching the canonical PAX functor
    /// entry layout.
    pub fn to_bytes(&self) -> [u8; FUNCTOR_ENTRY_SIZE] {
        let mut bytes = [0u8; FUNCTOR_ENTRY_SIZE];

        // Offset 0: functor_symbol_id (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.functor_symbol_id.to_le_bytes());

        // Offset 8: param_signature_hash (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.param_signature_hash.to_le_bytes());

        // Offset 16: result_signature_hash (8 bytes, little-endian)
        bytes[16..24].copy_from_slice(&self.result_signature_hash.to_le_bytes());

        // Offset 24: closure_data_offset (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.closure_data_offset.to_le_bytes());

        // Offset 32: closure_data_size (4 bytes, little-endian)
        bytes[32..36].copy_from_slice(&self.closure_data_size.to_le_bytes());

        // Offset 36: flags (4 bytes, little-endian)
        bytes[36..40].copy_from_slice(&self.flags.to_le_bytes());

        bytes
    }

    /// Parse a functor entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least FUNCTOR_ENTRY_SIZE bytes.
    /// Returns `None` on short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < FUNCTOR_ENTRY_SIZE {
            return None;
        }

        // Offset 0: functor_symbol_id (8 bytes, little-endian)
        let functor_symbol_id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: param_signature_hash (8 bytes, little-endian)
        let param_signature_hash = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: result_signature_hash (8 bytes, little-endian)
        let result_signature_hash = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        // Offset 24: closure_data_offset (8 bytes, little-endian)
        let closure_data_offset = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        // Offset 32: closure_data_size (4 bytes, little-endian)
        let closure_data_size = u32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);

        // Offset 36: flags (4 bytes, little-endian)
        let flags = u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]);

        Some(Self {
            functor_symbol_id,
            param_signature_hash,
            result_signature_hash,
            closure_data_offset,
            closure_data_size,
            flags,
        })
    }
}

/// Whole-section content: a sequence of FunctorEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct FunctorsSection {
    /// List of functor entries.
    pub entries: Vec<FunctorEntry>,
}

impl FunctorsSection {
    /// Create a new, empty functors section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a functor entry to the section.
    pub fn push(&mut self, e: FunctorEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to FUNCTOR_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * FUNCTOR_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse a functors section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of FUNCTOR_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(FUNCTOR_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / FUNCTOR_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * FUNCTOR_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + FUNCTOR_ENTRY_SIZE];
            let entry = FunctorEntry::from_bytes(entry_bytes)?;
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
    fn empty_functors_section_roundtrips() {
        let section = FunctorsSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = FunctorsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn single_entry_roundtrips() {
        let entry = FunctorEntry {
            functor_symbol_id: 0x0102030405060708,
            param_signature_hash: 0x1112131415161718,
            result_signature_hash: 0x2122232425262728,
            closure_data_offset: 0x3132333435363738,
            closure_data_size: 0x41424344,
            flags: 0x51525354,
        };

        let mut section = FunctorsSection::new();
        section.push(entry.clone());

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), FUNCTOR_ENTRY_SIZE);

        let parsed = FunctorsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
        assert_eq!(parsed.unwrap().entries[0], entry);
    }

    #[test]
    fn multi_entry_3_functors_roundtrips() {
        let mut section = FunctorsSection::new();

        let e1 = FunctorEntry {
            functor_symbol_id: 1,
            param_signature_hash: 0xAAAAAAAAAAAAAAAA,
            result_signature_hash: 0xBBBBBBBBBBBBBBBB,
            closure_data_offset: 0,
            closure_data_size: 0,
            flags: 0,
        };
        let e2 = FunctorEntry {
            functor_symbol_id: 2,
            param_signature_hash: 0xCCCCCCCCCCCCCCCC,
            result_signature_hash: 0xDDDDDDDDDDDDDDDD,
            closure_data_offset: 0,
            closure_data_size: 0,
            flags: 0,
        };
        let e3 = FunctorEntry {
            functor_symbol_id: 3,
            param_signature_hash: 0xEEEEEEEEEEEEEEEE,
            result_signature_hash: 0xFFFFFFFFFFFFFFFF,
            closure_data_offset: 0,
            closure_data_size: 0,
            flags: 0,
        };

        section.push(e1.clone());
        section.push(e2.clone());
        section.push(e3.clone());

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * FUNCTOR_ENTRY_SIZE);

        let parsed = FunctorsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
        let unwrapped = parsed.unwrap();
        assert_eq!(unwrapped.entries[0], e1);
        assert_eq!(unwrapped.entries[1], e2);
        assert_eq!(unwrapped.entries[2], e3);
    }

    #[test]
    fn schema_snapshot_byte_layout() {
        // Hand-built entry with known byte values
        let entry = FunctorEntry {
            functor_symbol_id: 0x0102030405060708,
            param_signature_hash: 0x1112131415161718,
            result_signature_hash: 0x2122232425262728,
            closure_data_offset: 0x3132333435363738,
            closure_data_size: 0x0A0B0C0D,
            flags: 0x4142434D,
        };

        let bytes = entry.to_bytes();

        // Offset 0: functor_symbol_id (0x0102030405060708) as u64 little-endian
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8: param_signature_hash (0x1112131415161718) as u64 little-endian
        assert_eq!(
            &bytes[8..16],
            &[0x18u8, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11]
        );

        // Offset 16: result_signature_hash (0x2122232425262728) as u64 little-endian
        assert_eq!(
            &bytes[16..24],
            &[0x28u8, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22, 0x21]
        );

        // Offset 24: closure_data_offset (0x3132333435363738) as u64 little-endian
        assert_eq!(
            &bytes[24..32],
            &[0x38u8, 0x37, 0x36, 0x35, 0x34, 0x33, 0x32, 0x31]
        );

        // Offset 32: closure_data_size (0x0A0B0C0D) as u32 little-endian
        assert_eq!(&bytes[32..36], &[0x0Du8, 0x0C, 0x0B, 0x0A]);

        // Offset 36: flags (0x4142434D) as u32 little-endian
        assert_eq!(&bytes[36..40], &[0x4Du8, 0x43, 0x42, 0x41]);
    }
}
