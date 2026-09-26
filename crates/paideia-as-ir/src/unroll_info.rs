//! Unroll-info side-table: records the post-rewrite shape of a loop
//! that `opt::unroll` has actually unrolled.
//!
//! PAS-DEBT-B3-003 (issue #1516): the unroll pass writes one
//! `UnrollInfo` entry per rewritten Loop node so downstream stages
//! (encoder, dispatch bookkeeping, later diagnostic passes) can tell
//! at a glance:
//!
//! - the unroll factor that was applied,
//! - how many iterations the main (unrolled) loop now runs,
//! - the residual iteration count for the tail loop, when the trip
//!   count did not divide evenly (`remainder_iters > 0`),
//! - the IrNodeId of the emitted remainder-loop node, if any.
//!
//! A Loop with no entry here was left alone by the unroll pass —
//! either the trip count was unknown, the body carried an unroll
//! blocker (Call, RepMovsb, …), or the pass simply did not run.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Post-unroll metadata for a single rewritten Loop node.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct UnrollInfo {
    /// Unroll factor applied (k copies of the body inlined per main
    /// iteration).
    pub factor: u32,
    /// Iterations the main (unrolled) loop runs (trip_count / factor).
    pub main_iters: u32,
    /// Residual iterations executed by the remainder loop
    /// (trip_count % factor). Zero when the trip count divided evenly.
    pub remainder_iters: u32,
    /// IrNodeId of the freshly-allocated remainder-loop node, when
    /// `remainder_iters > 0`. `None` otherwise.
    pub remainder_loop: Option<IrNodeId>,
}

/// Sparse mapping: original Loop IrNodeId → UnrollInfo.
#[derive(Default, Debug, Clone)]
pub struct UnrollInfoTable {
    entries: HashMap<IrNodeId, UnrollInfo>,
}

impl UnrollInfoTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record unroll metadata for a Loop node.
    pub fn insert(&mut self, loop_id: IrNodeId, info: UnrollInfo) {
        self.entries.insert(loop_id, info);
    }

    /// Retrieve the unroll metadata for a Loop node, if recorded.
    #[must_use]
    pub fn get(&self, loop_id: IrNodeId) -> Option<UnrollInfo> {
        self.entries.get(&loop_id).copied()
    }

    /// True iff no entries have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut t = UnrollInfoTable::new();
        let id = IrNodeId::new(4).unwrap();
        let info = UnrollInfo {
            factor: 4,
            main_iters: 2,
            remainder_iters: 0,
            remainder_loop: None,
        };
        t.insert(id, info);
        assert_eq!(t.get(id), Some(info));
    }

    #[test]
    fn missing_returns_none() {
        let t = UnrollInfoTable::new();
        assert_eq!(t.get(IrNodeId::new(1).unwrap()), None);
    }

    #[test]
    fn remainder_variant_round_trips() {
        let mut t = UnrollInfoTable::new();
        let id = IrNodeId::new(5).unwrap();
        let rem_id = IrNodeId::new(6).unwrap();
        let info = UnrollInfo {
            factor: 4,
            main_iters: 2,
            remainder_iters: 2,
            remainder_loop: Some(rem_id),
        };
        t.insert(id, info);
        let got = t.get(id).unwrap();
        assert_eq!(got.factor, 4);
        assert_eq!(got.main_iters, 2);
        assert_eq!(got.remainder_iters, 2);
        assert_eq!(got.remainder_loop, Some(rem_id));
    }
}
