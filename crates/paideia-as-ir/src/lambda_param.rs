//! Lambda parameter metadata side-table.
//!
//! Maps Lambda IR node IDs to their parameter pattern node IDs.
//! This enables emit_walker to look up actual parameter names from the
//! binding_names table during parameter registration.
//!
//! Pattern: m3-007 HandlerSideTable / m1-006 LoadStoreSideTable.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Side-table mapping Lambda node IDs → list of parameter pattern node IDs.
///
/// Tracks the parameter patterns for each Lambda, allowing the elaborator
/// to associate IR Lambda nodes with their AST parameter patterns.
#[derive(Default, Debug, Clone)]
pub struct LambdaParamTable {
    /// Mapping: Lambda IR node id -> list of parameter pattern IDs (AST node IDs).
    /// In the IR, these are the same as AST node IDs per the lowering invariant.
    entries: HashMap<IrNodeId, Vec<IrNodeId>>,
}

impl LambdaParamTable {
    /// Construct an empty lambda parameter side-table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert or update the parameter list for a Lambda node.
    pub fn insert(&mut self, lambda_id: IrNodeId, params: Vec<IrNodeId>) {
        self.entries.insert(lambda_id, params);
    }

    /// Retrieve the parameter list for a Lambda node.
    #[must_use]
    pub fn get(&self, lambda_id: IrNodeId) -> Option<&[IrNodeId]> {
        self.entries.get(&lambda_id).map(|v| v.as_slice())
    }

    /// Check if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Clear all entries from the table.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Iterate over all entries (lambda_id, params).
    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &Vec<IrNodeId>)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambda_param_table_insert_and_get() {
        let mut table = LambdaParamTable::new();
        let lambda_id = IrNodeId::new(42).unwrap();
        let param_ids = vec![IrNodeId::new(1).unwrap(), IrNodeId::new(2).unwrap()];
        table.insert(lambda_id, param_ids.clone());

        assert_eq!(table.get(lambda_id), Some(&param_ids[..]));
    }

    #[test]
    fn lambda_param_table_get_missing_returns_none() {
        let table = LambdaParamTable::new();
        let lambda_id = IrNodeId::new(42).unwrap();

        assert_eq!(table.get(lambda_id), None);
    }
}
