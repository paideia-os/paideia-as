//! Side-table for lambda parameter enum types (#1156).
//!
//! Issue #1156 (receiver-side): Tracks which lambda parameters have enum types,
//! enabling register_nested_lambda_params to detect enum-typed pos-0 parameters
//! and install (RAX, RDX) pair bindings instead of scalar RDI binding.
//!
//! Populated by populate_lambda_param_enum_types during elaboration.
//! Consumed by register_nested_lambda_params during emission.

use crate::enum_layout::EnumTypeId;
use crate::node::IrNodeId;
use std::collections::HashMap;

/// Side-table: (Lambda IrNodeId, param_index) -> EnumTypeId when the
/// param's declared type resolves to a registered enum.
/// Populated by populate_lambda_param_enum_types. Consumed by
/// register_nested_lambda_params to install a (RAX, RDX) pair binding
/// for register-form pair-passing enum parameters at pos 0.
#[derive(Debug, Default, Clone)]
pub struct LambdaParamEnumTypeTable {
    entries: HashMap<(IrNodeId, u32), EnumTypeId>,
}

impl LambdaParamEnumTypeTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping (lambda_id, param_index) -> enum_type_id.
    pub fn insert(&mut self, key: (IrNodeId, u32), value: EnumTypeId) {
        self.entries.insert(key, value);
    }

    /// Look up enum type for a lambda parameter.
    pub fn get(&self, key: &(IrNodeId, u32)) -> Option<&EnumTypeId> {
        self.entries.get(key)
    }

    /// Number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if the table has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&(IrNodeId, u32), &EnumTypeId)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut table = LambdaParamEnumTypeTable::new();
        let lambda_id = IrNodeId::new(1).unwrap();
        let enum_id = EnumTypeId(42);
        let key = (lambda_id, 0u32);

        table.insert(key, enum_id);
        assert_eq!(table.get(&key), Some(&enum_id));
    }

    #[test]
    fn get_returns_none_for_unknown() {
        let table = LambdaParamEnumTypeTable::new();
        let lambda_id = IrNodeId::new(1).unwrap();
        let key = (lambda_id, 0u32);

        assert_eq!(table.get(&key), None);
    }

    #[test]
    fn len_and_is_empty() {
        let mut table = LambdaParamEnumTypeTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let lambda_id = IrNodeId::new(1).unwrap();
        let enum_id = EnumTypeId(42);
        table.insert((lambda_id, 0u32), enum_id);

        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn iter_visits_all_entries() {
        let mut table = LambdaParamEnumTypeTable::new();
        let lambda_id1 = IrNodeId::new(1).unwrap();
        let lambda_id2 = IrNodeId::new(2).unwrap();
        let enum_id1 = EnumTypeId(42);
        let enum_id2 = EnumTypeId(43);

        table.insert((lambda_id1, 0u32), enum_id1);
        table.insert((lambda_id2, 0u32), enum_id2);

        let entries: Vec<_> = table.iter().collect();
        assert_eq!(entries.len(), 2);
    }
}
