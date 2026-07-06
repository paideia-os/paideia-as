//! Per-parameter type hints for lambda parameters.
//!
//! During parsing of fn-style lambdas with comma-separated parameters,
//! each pattern is paired with a type. This side-table stores the mapping
//! from pattern NodeId to type NodeId so later passes (elaborator, lowering)
//! can look up the type of any parameter by its pattern node.

use std::collections::HashMap;
use crate::node_id::NodeId;

/// Maps pattern NodeId → type NodeId for lambda parameters.
#[derive(Debug, Default)]
pub struct PatternTypeHints {
    entries: HashMap<NodeId, NodeId>,
}

impl PatternTypeHints {
    /// Construct a new empty hint table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a pattern → type mapping.
    pub fn insert(&mut self, pattern_id: NodeId, type_id: NodeId) {
        self.entries.insert(pattern_id, type_id);
    }

    /// Look up the type NodeId for a pattern.
    #[must_use]
    pub fn get(&self, pattern_id: NodeId) -> Option<NodeId> {
        self.entries.get(&pattern_id).copied()
    }

    /// Check if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Iterate over (pattern, type) pairs.
    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let hints = PatternTypeHints::new();
        assert!(hints.is_empty());
        assert_eq!(hints.len(), 0);
    }

    #[test]
    fn insert_and_get() {
        let mut hints = PatternTypeHints::new();
        let pattern_id = NodeId::new(1).unwrap();
        let type_id = NodeId::new(2).unwrap();
        hints.insert(pattern_id, type_id);
        assert_eq!(hints.get(pattern_id), Some(type_id));
    }

    #[test]
    fn len_increases_on_insert() {
        let mut hints = PatternTypeHints::new();
        assert_eq!(hints.len(), 0);
        hints.insert(NodeId::new(1).unwrap(), NodeId::new(2).unwrap());
        assert_eq!(hints.len(), 1);
        hints.insert(NodeId::new(3).unwrap(), NodeId::new(4).unwrap());
        assert_eq!(hints.len(), 2);
    }

    #[test]
    fn clear_empties_table() {
        let mut hints = PatternTypeHints::new();
        hints.insert(NodeId::new(1).unwrap(), NodeId::new(2).unwrap());
        assert_eq!(hints.len(), 1);
        hints.clear();
        assert!(hints.is_empty());
    }

    #[test]
    fn iter_produces_all_pairs() {
        let mut hints = PatternTypeHints::new();
        let p1 = NodeId::new(1).unwrap();
        let t1 = NodeId::new(2).unwrap();
        let p2 = NodeId::new(3).unwrap();
        let t2 = NodeId::new(4).unwrap();
        hints.insert(p1, t1);
        hints.insert(p2, t2);

        let mut pairs: Vec<_> = hints.iter().collect();
        pairs.sort_by_key(|(p, _)| p.get());
        assert_eq!(pairs.len(), 2);
    }
}
