//! Side-table for data section entries (rodata and data).
//!
//! Maps IrNodeId (data-bearing nodes) to their bytes, symbol names, alignment,
//! and section kind. This enables the EmitWalker to recognize module-level Let
//! bindings with Literal or ArrayLit bodies and stage them for ELF emission.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Relocation width specifier per PA10-006u.
///
/// Indicates the size of the relocation slot in the data section.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RelocWidth {
    /// 32-bit (4-byte) relocation slot.
    W32,
    /// 64-bit (8-byte) relocation slot.
    W64,
}

/// A relocation entry within a data section.
///
/// Specifies that a slot at a given offset should be patched
/// with the address of a symbol (PA10-002: string intern symbols;
/// PA10-006u: address-of static initializers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelocSpec {
    /// Byte offset within the data entry where the relocation applies.
    pub offset: u64,
    /// Target symbol name to be relocated (resolved by the linker).
    pub symbol: String,
    /// Width of the relocation slot (W32 or W64).
    pub width: RelocWidth,
    /// Addend: adjustment to the relocation value.
    /// For simple address relocations, this is typically 0.
    pub addend: i64,
}

impl RelocSpec {
    /// Construct a new relocation entry.
    #[must_use]
    pub fn new(offset: u64, symbol: String) -> Self {
        Self {
            offset,
            symbol,
            width: RelocWidth::W64,
            addend: 0,
        }
    }

    /// Construct a new relocation entry with explicit width and addend (PA10-006u).
    #[must_use]
    pub fn with_width(offset: u64, symbol: String, width: RelocWidth, addend: i64) -> Self {
        Self {
            offset,
            symbol,
            width,
            addend,
        }
    }
}

/// Section kind for data entries.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub enum SectionKind {
    /// Read-only data section (.rodata). Default for immutable let bindings.
    #[default]
    Rodata,
    /// Initialized data section (.data). Used for mutable let mut bindings (Phase 6+).
    Data,
    /// Uninitialized data section (.bss). Used for uninit mutable bindings (Phase 6+).
    Bss,
    /// Code section (.text). Used for code emitted during module walk.
    Text,
}

/// A single entry in the data section.
///
/// Represents a module-level data binding that has been lowered to bytes
/// and is ready for ELF emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataEntry {
    /// Which section to emit into (.rodata, .data, or .bss).
    pub section: SectionKind,
    /// Little-endian packed bytes of the data.
    pub bytes: Vec<u8>,
    /// Symbol name (defaults to source binding identifier).
    pub symbol_name: String,
    /// Alignment requirement in bytes (power of 2). Common values: 1, 4, 8, 16.
    pub align: u8,
    /// Size hint in bytes. For .rodata and .data: bytes.len(). For .bss: computed from type.
    pub size_hint: u64,
    /// Relocations within this data entry (PA10-002: string intern symbols).
    /// Empty vec if no relocations are needed.
    pub relocations: Vec<RelocSpec>,
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
        let size_hint = bytes.len() as u64;
        Self {
            section: SectionKind::Rodata,
            bytes,
            symbol_name,
            align,
            size_hint,
            relocations: Vec::new(),
        }
    }

    /// Construct a new data entry for .rodata with relocations (PA10-002).
    ///
    /// # Arguments
    /// * `bytes` - little-endian packed bytes
    /// * `symbol_name` - C-friendly symbol identifier
    /// * `align` - power-of-2 alignment (e.g., 8 for 8-byte aligned)
    /// * `relocations` - vector of relocation entries within the data
    #[must_use]
    pub fn new_rodata_with_relocs(
        bytes: Vec<u8>,
        symbol_name: String,
        align: u8,
        relocations: Vec<RelocSpec>,
    ) -> Self {
        let size_hint = bytes.len() as u64;
        Self {
            section: SectionKind::Rodata,
            bytes,
            symbol_name,
            align,
            size_hint,
            relocations,
        }
    }

    /// Construct a new data entry for .data (mutable, Phase 6+).
    #[must_use]
    pub fn new_data(bytes: Vec<u8>, symbol_name: String, align: u8) -> Self {
        let size_hint = bytes.len() as u64;
        Self {
            section: SectionKind::Data,
            bytes,
            symbol_name,
            align,
            size_hint,
            relocations: Vec::new(),
        }
    }

    /// Construct a new data entry for .bss (uninitialized, Phase 6+).
    ///
    /// # Arguments
    /// * `symbol_name` - C-friendly symbol identifier
    /// * `align` - power-of-2 alignment (e.g., 8 for 8-byte aligned)
    /// * `size_hint` - size in bytes (computed from type at elaboration time)
    #[must_use]
    pub fn new_bss(symbol_name: String, align: u8, size_hint: u64) -> Self {
        Self {
            section: SectionKind::Bss,
            bytes: Vec::new(),
            symbol_name,
            align,
            size_hint,
            relocations: Vec::new(),
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

    #[test]
    fn section_kind_bss_variant_exists() {
        let bss = SectionKind::Bss;
        assert_eq!(bss, SectionKind::Bss);
        assert_ne!(bss, SectionKind::Rodata);
        assert_ne!(bss, SectionKind::Data);
    }

    #[test]
    fn data_entry_new_bss_constructs() {
        let entry = DataEntry::new_bss("uninit_array".to_string(), 8, 4096);
        assert_eq!(entry.section, SectionKind::Bss);
        assert!(entry.bytes.is_empty());
        assert_eq!(entry.symbol_name, "uninit_array");
        assert_eq!(entry.align, 8);
        assert_eq!(entry.size_hint, 4096);
    }

    #[test]
    fn data_entry_rodata_size_hint_equals_bytes_len() {
        let bytes = vec![1, 2, 3, 4, 5];
        let entry = DataEntry::new_rodata(bytes.clone(), "data".to_string(), 4);
        assert_eq!(entry.size_hint, bytes.len() as u64);
        assert_eq!(entry.size_hint, 5);
    }

    #[test]
    fn data_entry_data_size_hint_equals_bytes_len() {
        let bytes = vec![0xAA; 16];
        let entry = DataEntry::new_data(bytes.clone(), "mut_data".to_string(), 8);
        assert_eq!(entry.size_hint, bytes.len() as u64);
        assert_eq!(entry.size_hint, 16);
    }

    #[test]
    fn data_entry_bss_size_hint_independent_of_bytes() {
        let entry = DataEntry::new_bss("uninit".to_string(), 8, 8192);
        assert!(entry.bytes.is_empty());
        assert_eq!(entry.size_hint, 8192);
    }

    #[test]
    fn data_entry_sections_have_correct_size_hints() {
        let rodata_entry = DataEntry::new_rodata(vec![1, 2], "ro".to_string(), 1);
        let data_entry = DataEntry::new_data(vec![3, 4, 5], "rw".to_string(), 1);
        let bss_entry = DataEntry::new_bss("uninit".to_string(), 8, 64);

        assert_eq!(rodata_entry.size_hint, 2);
        assert_eq!(data_entry.size_hint, 3);
        assert_eq!(bss_entry.size_hint, 64);
    }

    // PA10-006u tests
    #[test]
    fn reloc_spec_width_and_addend_roundtrip() {
        let spec = RelocSpec::with_width(0, "target".to_string(), RelocWidth::W64, 0);
        assert_eq!(spec.offset, 0);
        assert_eq!(spec.symbol, "target");
        assert_eq!(spec.width, RelocWidth::W64);
        assert_eq!(spec.addend, 0);

        let spec32 = RelocSpec::with_width(4, "other".to_string(), RelocWidth::W32, 16);
        assert_eq!(spec32.offset, 4);
        assert_eq!(spec32.symbol, "other");
        assert_eq!(spec32.width, RelocWidth::W32);
        assert_eq!(spec32.addend, 16);
    }

    #[test]
    fn reloc_spec_default_width_is_w64() {
        let spec = RelocSpec::new(0, "target".to_string());
        assert_eq!(spec.width, RelocWidth::W64);
        assert_eq!(spec.addend, 0);
    }

    #[test]
    fn addr_of_static_init_single_symbol() {
        // let p : u64 = & target → 8 zero bytes + reloc at offset 0
        let bytes = vec![0u8; 8];
        let reloc = RelocSpec::with_width(0, "target".to_string(), RelocWidth::W64, 0);
        let entry = DataEntry::new_rodata_with_relocs(bytes, "p".to_string(), 8, vec![reloc]);

        assert_eq!(entry.bytes.len(), 8);
        assert!(entry.bytes.iter().all(|&b| b == 0));
        assert_eq!(entry.relocations.len(), 1);
        assert_eq!(entry.relocations[0].symbol, "target");
        assert_eq!(entry.relocations[0].offset, 0);
        assert_eq!(entry.relocations[0].width, RelocWidth::W64);
    }

    #[test]
    fn addr_of_static_init_array_of_symbols() {
        // let arr : [u64; 2] = [& a, & b] → 16 zero bytes + 2 relocs at offsets 0, 8
        let bytes = vec![0u8; 16];
        let relocs = vec![
            RelocSpec::with_width(0, "a".to_string(), RelocWidth::W64, 0),
            RelocSpec::with_width(8, "b".to_string(), RelocWidth::W64, 0),
        ];
        let entry = DataEntry::new_rodata_with_relocs(bytes, "arr".to_string(), 8, relocs);

        assert_eq!(entry.bytes.len(), 16);
        assert_eq!(entry.relocations.len(), 2);
        assert_eq!(entry.relocations[0].offset, 0);
        assert_eq!(entry.relocations[0].symbol, "a");
        assert_eq!(entry.relocations[1].offset, 8);
        assert_eq!(entry.relocations[1].symbol, "b");
    }
}
