//! `.paideia.effects` section content: per-function effect-row descriptors.
//!
//! Each entry is variable-length:
//!
//! | Offset | Size | Field           |
//! |--------|------|-----------------|
//! | 0      | 8    | function_symbol_id |
//! | 8      | 4    | fixed_count     |
//! | 12     | 4    | row_var_id      |
//! | 16     | 4×fixed_count | effect_ids |
//!
//! The header (first 16 bytes) is fixed; the trailing effect-id array makes
//! each entry `16 + 4 * fixed_count` bytes.

/// Effect-row entry for a single function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectRowEntry {
    /// The function's symbol id.
    pub function_symbol_id: u64,
    /// Fixed effect ids (must-have effects).
    pub fixed_effects: Vec<u32>,
    /// Row variable id: None = closed row, Some(id) = open row with variable.
    pub row_var_id: Option<u32>,
}

impl EffectRowEntry {
    /// Create a new effect-row entry.
    ///
    /// # Arguments
    ///
    /// * `function_symbol_id` - The function's symbol id
    /// * `fixed_effects` - Vector of effect ids
    /// * `row_var_id` - Optional row variable id (None for closed row)
    ///
    /// # Returns
    ///
    /// A new EffectRowEntry.
    pub fn new(function_symbol_id: u64, fixed_effects: Vec<u32>, row_var_id: Option<u32>) -> Self {
        Self {
            function_symbol_id,
            fixed_effects,
            row_var_id,
        }
    }

    /// Serialize this entry to bytes.
    ///
    /// Returns a vector of `16 + 4 * fixed_count` bytes in little-endian format:
    /// - 8 bytes: function_symbol_id
    /// - 4 bytes: fixed_count (length of fixed_effects)
    /// - 4 bytes: row_var_id (0 for None, else the id)
    /// - 4*fixed_count bytes: effect ids
    pub fn to_bytes(&self) -> Vec<u8> {
        let fixed_count = self.fixed_effects.len() as u32;
        let row_var_id_val = self.row_var_id.unwrap_or(0);

        let mut bytes = Vec::with_capacity(16 + (self.fixed_effects.len() * 4));

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        bytes.extend_from_slice(&self.function_symbol_id.to_le_bytes());

        // Offset 8: fixed_count (4 bytes, little-endian)
        bytes.extend_from_slice(&fixed_count.to_le_bytes());

        // Offset 12: row_var_id (4 bytes, little-endian)
        bytes.extend_from_slice(&row_var_id_val.to_le_bytes());

        // Offset 16+: effect ids (4 bytes each, little-endian)
        for effect_id in &self.fixed_effects {
            bytes.extend_from_slice(&effect_id.to_le_bytes());
        }

        bytes
    }

    /// Parse an effect-row entry from bytes.
    ///
    /// Returns `Some((entry, bytes_consumed))` on success, or `None` if:
    /// - Input is shorter than 16 bytes, or
    /// - Input doesn't contain enough bytes for all effect ids.
    pub fn from_bytes(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.len() < 16 {
            return None;
        }

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        let function_symbol_id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: fixed_count (4 bytes, little-endian)
        let fixed_count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;

        // Offset 12: row_var_id (4 bytes, little-endian)
        let row_var_id_val = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let row_var_id = if row_var_id_val == 0 {
            None
        } else {
            Some(row_var_id_val)
        };

        // Verify we have enough bytes for the effect ids
        let total_size = 16 + (fixed_count * 4);
        if bytes.len() < total_size {
            return None;
        }

        // Offset 16+: effect ids (4 bytes each, little-endian)
        let mut fixed_effects = Vec::with_capacity(fixed_count);
        for i in 0..fixed_count {
            let offset = 16 + (i * 4);
            let effect_id = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            fixed_effects.push(effect_id);
        }

        let entry = Self {
            function_symbol_id,
            fixed_effects,
            row_var_id,
        };

        Some((entry, total_size))
    }
}

/// Whole-section content: a sequence of EffectRowEntry.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct EffectsSection {
    /// List of effect-row entries.
    pub entries: Vec<EffectRowEntry>,
}

impl EffectsSection {
    /// Create a new, empty effects section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an effect-row entry to the section.
    pub fn push(&mut self, e: EffectRowEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized in order; variable-length entries are
    /// concatenated directly.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for entry in &self.entries {
            bytes.extend(entry.to_bytes());
        }
        bytes
    }

    /// Parse an effects section from a byte slice.
    ///
    /// Reads entries sequentially until the input is exhausted.
    /// Returns `Some(section)` if all bytes are successfully parsed.
    /// Returns `None` if parsing fails (truncated entry, etc.).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset < bytes.len() {
            let remaining = &bytes[offset..];
            let (entry, consumed) = EffectRowEntry::from_bytes(remaining)?;
            entries.push(entry);
            offset += consumed;
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
    fn closed_row_2_effects_roundtrip() {
        // AC 1: function with 2 fixed effects, closed row (row_var=None)
        let entry = EffectRowEntry::new(12345, vec![0x0001, 0x0002], None);

        let bytes = entry.to_bytes();
        let (parsed, consumed) = EffectRowEntry::from_bytes(&bytes).expect("Failed to parse");

        assert_eq!(parsed, entry);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn closed_row_empty_roundtrip() {
        // AC 2: pure function (fixed effects empty, row_var=None)
        let entry = EffectRowEntry::new(99999, vec![], None);

        let bytes = entry.to_bytes();
        let (parsed, consumed) = EffectRowEntry::from_bytes(&bytes).expect("Failed to parse");

        assert_eq!(parsed, entry);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn open_row_with_var_roundtrip() {
        // AC 3: function with open row {Io | r1} → row_var=Some(1)
        let entry = EffectRowEntry::new(54321, vec![0x0003], Some(1));

        let bytes = entry.to_bytes();
        let (parsed, consumed) = EffectRowEntry::from_bytes(&bytes).expect("Failed to parse");

        assert_eq!(parsed, entry);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn effects_section_3_entries_roundtrip() {
        // AC 4: section with 3 entries
        let mut section = EffectsSection::new();

        let e1 = EffectRowEntry::new(111, vec![0x0001, 0x0002], None);
        let e2 = EffectRowEntry::new(222, vec![], Some(5));
        let e3 = EffectRowEntry::new(333, vec![0x0005, 0x0006, 0x0007], None);

        section.push(e1);
        section.push(e2);
        section.push(e3);

        let bytes = section.to_bytes();
        let parsed = EffectsSection::from_bytes(&bytes).expect("Failed to parse section");

        assert_eq!(parsed, section);
    }

    #[test]
    fn entry_size_formula() {
        // AC 5: confirm size == 16 + 4*fixed_count
        let entry0 = EffectRowEntry::new(1, vec![], None);
        let bytes0 = entry0.to_bytes();
        assert_eq!(bytes0.len(), 16); // 16 + 4*0

        let entry1 = EffectRowEntry::new(2, vec![0x0001], None);
        let bytes1 = entry1.to_bytes();
        assert_eq!(bytes1.len(), 20); // 16 + 4*1

        let entry2 = EffectRowEntry::new(3, vec![0x0001, 0x0002], None);
        let bytes2 = entry2.to_bytes();
        assert_eq!(bytes2.len(), 24); // 16 + 4*2

        let entry3 = EffectRowEntry::new(4, vec![0x0001, 0x0002, 0x0003, 0x0004], None);
        let bytes3 = entry3.to_bytes();
        assert_eq!(bytes3.len(), 32); // 16 + 4*4
    }

    #[test]
    fn from_bytes_rejects_truncated() {
        // AC 6: truncated input should be rejected
        let truncated1 = [0u8; 15]; // Less than 16
        let result1 = EffectRowEntry::from_bytes(&truncated1);
        assert_eq!(result1, None);

        // Header ok (16 bytes) but claims 2 effects, truncated at 18 bytes (needs 24)
        let mut bytes = [0u8; 20];
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes()); // fixed_count = 2
        let truncated2 = &bytes[0..18]; // Only 18 bytes total, needs 24
        let result2 = EffectRowEntry::from_bytes(truncated2);
        assert_eq!(result2, None);
    }

    #[test]
    fn mixed_closed_and_open_entries_roundtrip() {
        // AC 7: mix of closed and open rows in a section
        let mut section = EffectsSection::new();

        let e1 = EffectRowEntry::new(1001, vec![0x0010], None); // closed, 1 effect
        let e2 = EffectRowEntry::new(1002, vec![0x0020, 0x0021], Some(7)); // open, 2 effects
        let e3 = EffectRowEntry::new(1003, vec![], None); // closed, 0 effects

        section.push(e1.clone());
        section.push(e2.clone());
        section.push(e3.clone());

        let bytes = section.to_bytes();
        let parsed = EffectsSection::from_bytes(&bytes).expect("Failed to parse section");

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed.entries[0], e1);
        assert_eq!(parsed.entries[1], e2);
        assert_eq!(parsed.entries[2], e3);
    }

    #[test]
    fn schema_snapshot_byte_layout() {
        // AC 8: hand-built entry; assert specific byte offsets
        let entry = EffectRowEntry {
            function_symbol_id: 0x0102030405060708,
            fixed_effects: vec![0x0A0B0C0D, 0x0E0F1011],
            row_var_id: Some(0x12131415),
        };

        let bytes = entry.to_bytes();

        // Offset 0: function_symbol_id (0x0102030405060708) as u64 little-endian
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8: fixed_count (2) as u32 little-endian
        assert_eq!(&bytes[8..12], &[0x02u8, 0x00, 0x00, 0x00]);

        // Offset 12: row_var_id (0x12131415) as u32 little-endian
        assert_eq!(&bytes[12..16], &[0x15u8, 0x14, 0x13, 0x12]);

        // Offset 16: first effect id (0x0A0B0C0D) as u32 little-endian
        assert_eq!(&bytes[16..20], &[0x0Du8, 0x0C, 0x0B, 0x0A]);

        // Offset 20: second effect id (0x0E0F1011) as u32 little-endian
        assert_eq!(&bytes[20..24], &[0x11u8, 0x10, 0x0F, 0x0E]);
    }

    #[test]
    fn empty_effects_section_roundtrips() {
        let section = EffectsSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = EffectsSection::from_bytes(&bytes).expect("Failed to parse");
        assert_eq!(parsed, section);
    }
}
