//! `.paideia.opt-passes` section: per-pass rewrite count records.
//!
//! Records optimization pass results with variable-length pass names.
//!
//! Each entry is variable-length:
//!
//! | Offset | Size | Field           |
//! |--------|------|-----------------|
//! | 0      | 4    | pass_name_len   |
//! | 4      | pass_name_len | pass_name |
//! | 4+pass_name_len | 8 | function_id |
//! | 12+pass_name_len | 4 | rewrite_count |

use paideia_as_ir::IrNodeId;

/// A single optimization pass rewrite record.
///
/// Tracks the number of rewrites applied by a specific pass to a function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptPassRecord {
    /// Name of the optimization pass.
    pub pass_name: String,
    /// Function that was optimized (IR node id).
    pub function_id: IrNodeId,
    /// Number of rewrites/transformations applied.
    pub rewrite_count: u32,
}

impl OptPassRecord {
    /// Create a new optimization pass record.
    ///
    /// # Arguments
    ///
    /// * `pass_name` - The optimization pass name
    /// * `function_id` - The IR node id of the optimized function
    /// * `rewrite_count` - Number of rewrites applied
    ///
    /// # Returns
    ///
    /// A new OptPassRecord.
    pub fn new(pass_name: String, function_id: IrNodeId, rewrite_count: u32) -> Self {
        Self {
            pass_name,
            function_id,
            rewrite_count,
        }
    }

    /// Serialize this record to bytes (length-prefixed binary format).
    ///
    /// Format:
    /// - 4 bytes: pass_name length (little-endian u32)
    /// - pass_name_len bytes: UTF-8 encoded pass name
    /// - 8 bytes: function_id (little-endian u64)
    /// - 4 bytes: rewrite_count (little-endian u32)
    pub fn to_bytes(&self) -> Vec<u8> {
        let pass_name_bytes = self.pass_name.as_bytes();
        let pass_name_len = pass_name_bytes.len() as u32;

        let mut bytes = Vec::with_capacity(16 + pass_name_bytes.len());

        // Offset 0: pass_name_len (4 bytes, little-endian)
        bytes.extend_from_slice(&pass_name_len.to_le_bytes());

        // Offset 4: pass_name (variable length)
        bytes.extend_from_slice(pass_name_bytes);

        // Offset 4+pass_name_len: function_id (8 bytes, little-endian)
        let function_id_u64 = self.function_id.get() as u64;
        bytes.extend_from_slice(&function_id_u64.to_le_bytes());

        // Offset 12+pass_name_len: rewrite_count (4 bytes, little-endian)
        bytes.extend_from_slice(&self.rewrite_count.to_le_bytes());

        bytes
    }

    /// Parse an optimization pass record from bytes.
    ///
    /// Returns `Some((record, bytes_consumed))` on success, or `None` if:
    /// - Input is shorter than 16 bytes, or
    /// - pass_name is not valid UTF-8, or
    /// - Input doesn't contain enough bytes for all fields.
    pub fn from_bytes(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.len() < 16 {
            return None;
        }

        // Offset 0: pass_name_len (4 bytes, little-endian)
        let pass_name_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

        // Verify we have enough bytes for pass_name, function_id, and rewrite_count
        let total_len = 4 + pass_name_len + 8 + 4;
        if bytes.len() < total_len {
            return None;
        }

        // Offset 4: pass_name (variable length)
        let pass_name_bytes = &bytes[4..4 + pass_name_len];
        let pass_name = match std::str::from_utf8(pass_name_bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => return None, // Invalid UTF-8
        };

        // Offset 4+pass_name_len: function_id (8 bytes, little-endian)
        let offset_func_id = 4 + pass_name_len;
        let function_id_u64 = u64::from_le_bytes([
            bytes[offset_func_id],
            bytes[offset_func_id + 1],
            bytes[offset_func_id + 2],
            bytes[offset_func_id + 3],
            bytes[offset_func_id + 4],
            bytes[offset_func_id + 5],
            bytes[offset_func_id + 6],
            bytes[offset_func_id + 7],
        ]);
        let function_id = IrNodeId::new(function_id_u64 as u32)?;

        // Offset 12+pass_name_len: rewrite_count (4 bytes, little-endian)
        let offset_rewrite = offset_func_id + 8;
        let rewrite_count = u32::from_le_bytes([
            bytes[offset_rewrite],
            bytes[offset_rewrite + 1],
            bytes[offset_rewrite + 2],
            bytes[offset_rewrite + 3],
        ]);

        Some((
            Self {
                pass_name,
                function_id,
                rewrite_count,
            },
            total_len,
        ))
    }
}

/// Whole-section content: a sequence of OptPassRecord.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct OptPassesSection {
    /// List of optimization pass records.
    pub records: Vec<OptPassRecord>,
}

impl OptPassesSection {
    /// Create a new, empty opt-passes section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an optimization pass record to the section.
    pub fn push(&mut self, record: OptPassRecord) {
        self.records.push(record);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Concatenates all records in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for record in &self.records {
            bytes.extend_from_slice(&record.to_bytes());
        }
        bytes
    }

    /// Parse an opt-passes section from a byte slice.
    ///
    /// Returns `Some(section)` if all records parse successfully from the input.
    /// Returns `None` if any record fails to parse or if the input is incomplete.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut records = Vec::new();
        let mut offset = 0;

        while offset < bytes.len() {
            let (record, consumed) = OptPassRecord::from_bytes(&bytes[offset..])?;
            records.push(record);
            offset += consumed;
        }

        Some(Self { records })
    }

    /// Return the number of records in the section.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_pass_record_roundtrip_simple() {
        let func_id = IrNodeId::new(42).unwrap();
        let record = OptPassRecord::new("peephole".to_string(), func_id, 5);
        let bytes = record.to_bytes();
        let parsed = OptPassRecord::from_bytes(&bytes);
        assert_eq!(parsed.map(|(r, _)| r), Some(record));
    }

    #[test]
    fn opt_pass_record_preserves_function_id() {
        let function_id = IrNodeId::new(42).unwrap();
        let record = OptPassRecord::new("dse".to_string(), function_id, 10);
        let bytes = record.to_bytes();
        let (parsed_record, _) = OptPassRecord::from_bytes(&bytes).unwrap();
        assert_eq!(parsed_record.function_id, function_id);
    }

    #[test]
    fn opt_pass_record_preserves_rewrite_count() {
        let rewrite_count = 42u32;
        let func_id = IrNodeId::new(123).unwrap();
        let record = OptPassRecord::new("const-fold".to_string(), func_id, rewrite_count);
        let bytes = record.to_bytes();
        let (parsed_record, _) = OptPassRecord::from_bytes(&bytes).unwrap();
        assert_eq!(parsed_record.rewrite_count, rewrite_count);
    }

    #[test]
    fn opt_pass_record_handles_long_pass_name() {
        let long_name = "super-aggressive-custom-optimization-pass-with-long-name";
        let func_id = IrNodeId::new(99).unwrap();
        let record = OptPassRecord::new(long_name.to_string(), func_id, 3);
        let bytes = record.to_bytes();
        let (parsed_record, _) = OptPassRecord::from_bytes(&bytes).unwrap();
        assert_eq!(parsed_record.pass_name, long_name);
    }

    #[test]
    fn opt_pass_section_3_records_roundtrip() {
        let mut section = OptPassesSection::new();

        let r1 = OptPassRecord::new("peephole".to_string(), IrNodeId::new(1).unwrap(), 5);
        let r2 = OptPassRecord::new("dse".to_string(), IrNodeId::new(2).unwrap(), 10);
        let r3 = OptPassRecord::new("const-fold".to_string(), IrNodeId::new(3).unwrap(), 15);

        section.push(r1);
        section.push(r2);
        section.push(r3);

        let bytes = section.to_bytes();
        let parsed = OptPassesSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn opt_pass_section_empty_is_valid() {
        let section = OptPassesSection::new();
        assert!(section.is_empty());
        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);
        let parsed = OptPassesSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn opt_pass_record_from_bytes_rejects_short_input() {
        let bytes = [0u8; 15]; // Too short
        let result = OptPassRecord::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn opt_pass_record_from_bytes_rejects_incomplete_name() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(10u32).to_le_bytes()); // pass_name_len = 10
        bytes.extend_from_slice(b"abc"); // Only 3 bytes, need 10+8+4=22 total
        let result = OptPassRecord::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn opt_pass_record_bytes_consumed_is_correct() {
        let func_id = IrNodeId::new(42).unwrap();
        let record = OptPassRecord::new("test".to_string(), func_id, 5);
        let bytes = record.to_bytes();
        let (_, consumed) = OptPassRecord::from_bytes(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn opt_pass_section_multiple_records_in_sequence() {
        let mut section = OptPassesSection::new();

        for i in 1..=10 {
            let pass_name = format!("pass-{}", i);
            let func_id = IrNodeId::new(i as u32).unwrap();
            let record = OptPassRecord::new(pass_name, func_id, i as u32 * 2);
            section.push(record);
        }

        let bytes = section.to_bytes();
        let parsed = OptPassesSection::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.len(), 10);
        assert_eq!(parsed, section);
    }
}
