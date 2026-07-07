//! Side-table for address-of static initializer entries (PA10-006u).
//!
//! Maps IrNodeId (address-of expression nodes) to their symbol names and addends.
//! This enables the elaborator to recognize `& sym` in static-init contexts and
//! wire them into the relocation subsystem.

use crate::node::IrNodeId;

/// Metadata for an address-of constant in a static initializer.
///
/// Represents the target symbol and optional addend for a `& sym` expression
/// in a module-level let binding. The encoder uses this to generate a relocation
/// and populate the data section with zero bytes (which the linker will patch).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddrOfMeta {
    /// Target symbol name (must resolve to a module-level or extern symbol).
    pub symbol: String,
    /// Addend: adjustment to the relocation value.
    /// For simple address-of, typically 0; future phases may support `& sym + N`.
    pub addend: i64,
}

impl AddrOfMeta {
    /// Construct a new address-of metadata entry.
    #[must_use]
    pub fn new(symbol: String) -> Self {
        Self { symbol, addend: 0 }
    }

    /// Construct a new address-of metadata entry with explicit addend.
    #[must_use]
    pub fn with_addend(symbol: String, addend: i64) -> Self {
        Self { symbol, addend }
    }
}

crate::impl_named_side_table!(
    /// Side-table mapping IrNodeId (Borrow nodes in static-init contexts)
    /// → AddrOfMeta.
    ///
    /// Indexed by Borrow node ID to allow the elaborator to associate
    /// address-of metadata with `& sym` expressions in module-level let
    /// bindings.
    pub struct AddrOfSideTable, IrNodeId => AddrOfMeta
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addr_of_meta_new_constructs() {
        let meta = AddrOfMeta::new("target".to_string());
        assert_eq!(meta.symbol, "target");
        assert_eq!(meta.addend, 0);
    }

    #[test]
    fn addr_of_meta_with_addend() {
        let meta = AddrOfMeta::with_addend("target".to_string(), 42);
        assert_eq!(meta.symbol, "target");
        assert_eq!(meta.addend, 42);
    }

    #[test]
    fn addr_of_side_table_insert_and_get() {
        let mut table = AddrOfSideTable::new();
        let node_id = IrNodeId::new(1).unwrap();
        let meta = AddrOfMeta::new("foo".to_string());

        table.insert(node_id, meta.clone());
        let retrieved = table.get(node_id).unwrap();
        assert_eq!(retrieved.symbol, meta.symbol);
        assert_eq!(retrieved.addend, meta.addend);
    }

    #[test]
    fn addr_of_side_table_get_returns_none_for_unknown() {
        let table = AddrOfSideTable::new();
        let unknown_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unknown_id), None);
    }

    #[test]
    fn addr_of_side_table_len_and_is_empty() {
        let mut table = AddrOfSideTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let id1 = IrNodeId::new(1).unwrap();
        table.insert(id1, AddrOfMeta::new("a".to_string()));
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn addr_of_side_table_remove() {
        let mut table = AddrOfSideTable::new();
        let node_id = IrNodeId::new(1).unwrap();
        let meta = AddrOfMeta::new("target".to_string());

        table.insert(node_id, meta.clone());
        assert_eq!(table.len(), 1);

        let removed = table.remove(node_id).unwrap();
        assert_eq!(removed.symbol, meta.symbol);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn addr_of_side_table_iter() {
        let mut table = AddrOfSideTable::new();
        let id1 = IrNodeId::new(1).unwrap();
        let id2 = IrNodeId::new(2).unwrap();

        table.insert(id1, AddrOfMeta::new("a".to_string()));
        table.insert(id2, AddrOfMeta::new("b".to_string()));

        let entries: Vec<_> = table.iter().collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn addr_of_side_table_get_mut() {
        let mut table = AddrOfSideTable::new();
        let node_id = IrNodeId::new(1).unwrap();
        let meta = AddrOfMeta::new("target".to_string());

        table.insert(node_id, meta);
        let mut_entry = table.get_mut(node_id).unwrap();
        mut_entry.addend = 16;

        assert_eq!(table.get(node_id).unwrap().addend, 16);
    }
}
