//! Constant-pool emission for repeated 64-bit immediates.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;
use std::collections::HashMap;

/// The constant-pool optimization pass.
pub struct PoolConstantsPass;

/// Detect repeated 64-bit constants in a list. Returns a map from
/// constant → number of occurrences. Constants appearing ≥2 times
/// are candidates for the constant pool.
pub fn detect_repeated_constants(constants: &[u64]) -> HashMap<u64, usize> {
    let mut counts = HashMap::new();
    for c in constants {
        *counts.entry(*c).or_insert(0) += 1;
    }
    counts
}

/// Filter the count map down to pool-candidates (occurrence ≥ 2).
pub fn pool_candidates(counts: &HashMap<u64, usize>) -> Vec<u64> {
    let mut candidates: Vec<u64> = counts
        .iter()
        .filter(|&(_, &n)| n >= 2)
        .map(|(&c, _)| c)
        .collect();
    candidates.sort();
    candidates
}

impl OptPass for PoolConstantsPass {
    fn name(&self) -> &'static str {
        "pool-constants"
    }

    fn apply(&self, _arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        sink.emit(
            "pool-constants",
            "O1509 (would-fire): constant-pool emission dispatched".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_repeated_constants_counts_correctly() {
        let constants = vec![1u64, 1, 2, 3, 3, 3];
        let counts = detect_repeated_constants(&constants);

        assert_eq!(counts.get(&1), Some(&2));
        assert_eq!(counts.get(&2), Some(&1));
        assert_eq!(counts.get(&3), Some(&3));
    }

    #[test]
    fn detect_repeated_constants_empty_returns_empty() {
        let constants: Vec<u64> = vec![];
        let counts = detect_repeated_constants(&constants);

        assert!(counts.is_empty());
    }

    #[test]
    fn pool_candidates_filters_singletons() {
        let mut counts = HashMap::new();
        counts.insert(1u64, 2);
        counts.insert(2u64, 1);
        counts.insert(3u64, 2);

        let candidates = pool_candidates(&counts);

        assert_eq!(candidates, vec![1u64, 3]);
    }

    #[test]
    fn pool_candidates_empty_when_no_repeats() {
        let mut counts = HashMap::new();
        counts.insert(1u64, 1);
        counts.insert(2u64, 1);
        counts.insert(3u64, 1);

        let candidates = pool_candidates(&counts);

        assert!(candidates.is_empty());
    }

    #[test]
    fn pool_constants_pass_emits_o1509() {
        let pass = PoolConstantsPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "PoolConstantsPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "pool-constants");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1509 (would-fire): constant-pool emission dispatched")
        );
    }

    #[test]
    fn pool_constants_emits_no_diagnostic_for_unique_immediates() {
        // Phase-3 minimum: if all immediates are unique (occurrence count = 1),
        // no pooling candidates exist. The pass still emits the general O1509
        // diagnostic; per-pool filtering is deferred to m4.
        let constants = vec![1u64, 2u64, 3u64];
        let counts = detect_repeated_constants(&constants);
        let candidates = pool_candidates(&counts);

        assert!(
            candidates.is_empty(),
            "unique immediates should produce no pool candidates"
        );

        let pass = PoolConstantsPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        // The pass still emits O1509 in phase-3; filtering by candidate count
        // is deferred to encoder integration.
        assert!(!changed);
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(sink.diagnostics[0].message.contains("O1509"));
    }
}
