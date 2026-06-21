//! Side-table for data section entries (rodata and data).
//!
//! Maps IrNodeId (data-bearing nodes) to their bytes, symbol names, alignment,
//! and section kind. This enables the EmitWalker to recognize module-level Let
//! bindings with Literal or ArrayLit bodies and stage them for ELF emission.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Section kind for data entries.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SectionKind {
    /// Read-only data section (.rodata). Default for immutable let bindings.
    Rodata,
    /// Initialized data section (.data). Used for mutable let mut bindings (Phase 6+).
    Data,
}

impl Default for SectionKind {
    fn default() -> Self {
        Self::Rodata
    }
}

/// A single entry in the data section.
///
/// Represents a module-level data binding that has been lowered to bytes
/// and is ready for ELF emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataEntry {
    /// Which section to emit into (.rodata or .data).
    pub section: SectionKind,
    /// Little-endian packed bytes of the data.
    pub bytes: Vec<u8>,
    /// Symbol name (defaults to source binding identifier).
    pub symbol_name: String,
    /// Alignment requirement in bytes (power of 2). Common values: 1, 4, 8, 16.
    pub align: u8,
}

impl DataEntry {
    /// Construct a new data entry for .rodata.
    ///
    /// # Arguments
    /// * `bytes` - little-endian packed bytes
    /// * `symbol_name` - C-friendly symbol identifier
    /// * `align` - power-of-2 alignment (e.g., 8 for 8-byte aligned)
    #[must_use]
    pub fn new_rodata(bytes: Vec<u8>, symbol_name: String, align: u8) -> Self {
        Self {
            section: SectionKind::Rodata,
            bytes,
            symbol_name,
            align,
        }
    }

    /// Construct a new data entry for .data (mutable, Phase 6+).
    #[must_use]
    pub fn new_data(bytes: Vec<u8>, symbol_name: String, align: u8) -> Self {
        Self {
            section: SectionKind::Data,
            bytes,
            symbol_name,
            align,
        }
    }
}

/// Side-table mapping IrNodeId (data-bearing Let nodes) → DataEntry.
///
/// Pattern: mirrors LiteralValueTable / LoadStoreSideTable.
/// Indexed by Let node ID to allow the elaborator to associate data metadata
/// with module-level let bindings.
#[derive(Default, Debug, Clone)]
pub struct DataSideTable {
    /// Sparse mapping: data node id -> DataEntry.
    entries: HashMap<IrNodeId, DataEntry>,
}

impl DataSideTable {
    /// Construct an empty data side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a data entry.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, entry: DataEntry) -> Option<DataEntry> {
        self.entries.insert(id, entry)
    }

    /// Look up a data entry.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&DataEntry> {
        self.entries.get(&id)
    }

    /// Look up a data entry (mutable).
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut DataEntry> {
        self.entries.get_mut(&id)
    }

    /// Number of data entries registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no data entries are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove a data entry.
    ///
    /// Returns the entry if one existed.
    pub fn remove(&mut self, id: IrNodeId) -> Option<DataEntry> {
        self.entries.remove(&id)
    }

    /// Iterate over all entries (id, entry) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &DataEntry)> {
        self.entries.iter()
    }

    /// Borrow the underlying HashMap (read-only).
    #[must_use]
    pub fn entries(&self) -> &HashMap<IrNodeId, DataEntry> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_kind_default_is_rodata() {
        assert_eq!(SectionKind::default(), SectionKind::Rodata);
    }

    #[test]
    fn data_entry_new_rodata_constructs() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let entry = DataEntry::new_rodata(bytes.clone(), "gdt".to_string(), 8);
        assert_eq!(entry.section, SectionKind::Rodata);
        assert_eq!(entry.bytes, bytes);
        assert_eq!(entry.symbol_name, "gdt");
        assert_eq!(entry.align, 8);
    }

    #[test]
    fn data_entry_new_data_constructs() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04];
        let entry = DataEntry::new_data(bytes.clone(), "mut_data".to_string(), 4);
        assert_eq!(entry.section, SectionKind::Data);
        assert_eq!(entry.bytes, bytes);
        assert_eq!(entry.symbol_name, "mut_data");
        assert_eq!(entry.align, 4);
    }

    #[test]
    fn data_side_table_insert_and_get() {
        let mut table = DataSideTable::new();
        let data_id = IrNodeId::new(1).unwrap();
        let entry = DataEntry::new_rodata(vec![0x42], "x".to_string(), 1);

        table.insert(data_id, entry.clone());
        let retrieved = table.get(data_id).unwrap();
        assert_eq!(retrieved.bytes, entry.bytes);
        assert_eq!(retrieved.symbol_name, entry.symbol_name);
    }

    #[test]
    fn data_side_table_get_returns_none_for_unknown() {
        let table = DataSideTable::new();
        let unknown_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unknown_id), None);
    }

    #[test]
    fn data_side_table_len_and_is_empty() {
        let mut table = DataSideTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let id1 = IrNodeId::new(1).unwrap();
        table.insert(id1, DataEntry::new_rodata(vec![1], "a".to_string(), 1));
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn data_side_table_remove() {
        let mut table = DataSideTable::new();
        let data_id = IrNodeId::new(1).unwrap();
        let entry = DataEntry::new_rodata(vec![0xFF], "x".to_string(), 1);

        table.insert(data_id, entry.clone());
        assert_eq!(table.len(), 1);

        let removed = table.remove(data_id).unwrap();
        assert_eq!(removed.symbol_name, entry.symbol_name);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn data_side_table_iter() {
        let mut table = DataSideTable::new();
        let id1 = IrNodeId::new(1).unwrap();
        let id2 = IrNodeId::new(2).unwrap();

        table.insert(id1, DataEntry::new_rodata(vec![1, 2], "a".to_string(), 2));
        table.insert(id2, DataEntry::new_rodata(vec![3, 4], "b".to_string(), 4));

        let entries: Vec<_> = table.iter().collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn data_side_table_get_mut() {
        let mut table = DataSideTable::new();
        let data_id = IrNodeId::new(1).unwrap();
        let entry = DataEntry::new_rodata(vec![1, 2], "test".to_string(), 2);

        table.insert(data_id, entry);
        let mut_entry = table.get_mut(data_id).unwrap();
        mut_entry.align = 8;

        assert_eq!(table.get(data_id).unwrap().align, 8);
    }
}
