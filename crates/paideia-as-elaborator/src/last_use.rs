//! Compute the last-use point of each borrow in the IR.
//!
//! Phase 4 m6-005: Implements a simple post-order walk that records the
//! highest-numbered IrNodeId where each binding/region is used.
//!
//! This is the foundation of NLL (Non-Lexical Lifetimes) semantics, where
//! a borrow ends at its last-use point rather than at scope end.

use std::collections::HashMap;

/// Tracks the last-use point of each (binding, region) pair in the IR.
///
/// A "last-use point" is the highest-numbered IrNodeId where a specific
/// binding+region pair is used. After this point, the borrow is no longer
/// live and doesn't conflict with subsequent borrows.
#[derive(Default, Debug, Clone)]
pub struct LastUseAnalyzer {
    /// Map from (binding_id, region_id) to the highest IrNodeId where it was used.
    last_use: HashMap<(u32, u32), u32>,
}

impl LastUseAnalyzer {
    /// Creates a new, empty LastUseAnalyzer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a use of a binding in a region at the given IR node.
    ///
    /// Updates the last-use point to the maximum of its previous value
    /// and the provided `ir_node_id`.
    pub fn record_use(&mut self, binding: u32, region: u32, ir_node_id: u32) {
        let entry = self.last_use.entry((binding, region)).or_insert(0);
        if ir_node_id > *entry {
            *entry = ir_node_id;
        }
    }

    /// Retrieves the last-use point (highest IrNodeId) for a (binding, region) pair.
    ///
    /// Returns `Some(ir_node_id)` if the pair has been recorded, or `None` if never seen.
    #[must_use]
    pub fn last_use_of(&self, binding: u32, region: u32) -> Option<u32> {
        self.last_use.get(&(binding, region)).copied()
    }

    /// Returns all recorded last-use points.
    ///
    /// Useful for introspection and testing.
    #[must_use]
    pub fn all_last_uses(&self) -> &HashMap<(u32, u32), u32> {
        &self.last_use
    }

    /// Clears all recorded last-use points.
    pub fn clear(&mut self) {
        self.last_use.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_use_analyzer_records_highest_node_id() {
        let mut analyzer = LastUseAnalyzer::new();
        analyzer.record_use(1, 100, 50);
        analyzer.record_use(1, 100, 75);
        analyzer.record_use(1, 100, 60);

        assert_eq!(analyzer.last_use_of(1, 100), Some(75));
    }

    #[test]
    fn last_use_analyzer_returns_none_for_unseen() {
        let analyzer = LastUseAnalyzer::new();
        assert_eq!(analyzer.last_use_of(1, 100), None);
        assert_eq!(analyzer.last_use_of(99, 200), None);
    }

    #[test]
    fn last_use_analyzer_tracks_multiple_bindings() {
        let mut analyzer = LastUseAnalyzer::new();
        analyzer.record_use(1, 100, 10);
        analyzer.record_use(2, 100, 20);
        analyzer.record_use(3, 100, 30);

        assert_eq!(analyzer.last_use_of(1, 100), Some(10));
        assert_eq!(analyzer.last_use_of(2, 100), Some(20));
        assert_eq!(analyzer.last_use_of(3, 100), Some(30));
    }

    #[test]
    fn last_use_analyzer_tracks_multiple_regions() {
        let mut analyzer = LastUseAnalyzer::new();
        analyzer.record_use(1, 100, 10);
        analyzer.record_use(1, 101, 20);
        analyzer.record_use(1, 102, 30);

        assert_eq!(analyzer.last_use_of(1, 100), Some(10));
        assert_eq!(analyzer.last_use_of(1, 101), Some(20));
        assert_eq!(analyzer.last_use_of(1, 102), Some(30));
    }
}
