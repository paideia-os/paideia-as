//! Region identifiers for the m5 region calculus.
//!
//! Each `&` or `&mut` site at the AST level gets a fresh RegionId
//! during elaboration. The 'static lifetime is RegionId(0).

use std::collections::{HashMap, HashSet};

/// A unique identifier for a region (lifetime).
///
/// RegionId(0) is reserved for 'static.
/// Each `&` or `&mut` annotation gets a fresh RegionId during elaboration.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RegionId(pub u32);

impl RegionId {
    /// The 'static lifetime.
    pub const STATIC: RegionId = RegionId(0);

    /// Check if this region is 'static.
    pub fn is_static(self) -> bool {
        self.0 == 0
    }
}

/// Allocates fresh RegionIds for the m5 region calculus.
///
/// RegionId(0) is reserved for 'static; fresh regions start from RegionId(1).
#[derive(Debug, Clone)]
pub struct RegionInterner {
    next_id: u32,
}

impl RegionInterner {
    /// Create a new region interner.
    pub fn new() -> Self {
        Self { next_id: 1 } // 0 reserved for 'static
    }

    /// Allocate a fresh region ID.
    pub fn fresh(&mut self) -> RegionId {
        let id = RegionId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Return the count of regions allocated (including 'static at 0).
    pub fn count(&self) -> u32 {
        self.next_id
    }
}

impl Default for RegionInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// Records outlives-relations between regions.
///
/// 'a outlives 'b iff 'a's scope contains 'b's scope.
#[derive(Default, Debug, Clone)]
pub struct RegionGraph {
    /// outlives[a] = set of b such that a outlives b (a's scope ⊇ b's scope).
    outlives: HashMap<RegionId, HashSet<RegionId>>,
}

impl RegionGraph {
    /// Create a new empty region graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record: `outer` outlives `inner` (`outer`'s scope ⊇ `inner`'s scope).
    pub fn add_outlives(&mut self, outer: RegionId, inner: RegionId) {
        self.outlives.entry(outer).or_default().insert(inner);
    }

    /// Query: does `a` outlive `b`?
    pub fn outlives(&self, a: RegionId, b: RegionId) -> bool {
        if a == b {
            return true;
        }
        if a.is_static() {
            return true; // 'static outlives everything.
        }
        self.outlives.get(&a).is_some_and(|set| set.contains(&b))
    }

    /// Compute transitive closure: if a > b and b > c then a > c.
    ///
    /// Uses a Floyd-Warshall-style approach to add transitive edges.
    pub fn close_transitively(&mut self) {
        let nodes: Vec<RegionId> = self.outlives.keys().copied().collect();
        for &k in &nodes {
            let k_outs: Vec<RegionId> = self
                .outlives
                .get(&k)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default();
            for &i in &nodes {
                let i_has_k = self.outlives.get(&i).is_some_and(|s| s.contains(&k));
                if i_has_k {
                    for &j in &k_outs {
                        self.outlives.entry(i).or_default().insert(j);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_id_static_is_zero() {
        assert_eq!(RegionId::STATIC.0, 0);
        assert!(RegionId::STATIC.is_static());
    }

    #[test]
    fn region_interner_fresh_returns_unique() {
        let mut interner = RegionInterner::new();
        let r1 = interner.fresh();
        let r2 = interner.fresh();
        let r3 = interner.fresh();
        assert_eq!(r1, RegionId(1));
        assert_eq!(r2, RegionId(2));
        assert_eq!(r3, RegionId(3));
        assert!(r1 != r2);
        assert!(r2 != r3);
    }

    #[test]
    fn region_graph_outlives_self() {
        let graph = RegionGraph::new();
        assert!(graph.outlives(RegionId(1), RegionId(1)));
        assert!(graph.outlives(RegionId(5), RegionId(5)));
    }

    #[test]
    fn region_graph_static_outlives_all() {
        let graph = RegionGraph::new();
        assert!(graph.outlives(RegionId::STATIC, RegionId(1)));
        assert!(graph.outlives(RegionId::STATIC, RegionId(100)));
        assert!(graph.outlives(RegionId::STATIC, RegionId::STATIC));
    }

    #[test]
    fn region_graph_add_outlives_records() {
        let mut graph = RegionGraph::new();
        graph.add_outlives(RegionId(1), RegionId(2));
        assert!(graph.outlives(RegionId(1), RegionId(2)));
        assert!(!graph.outlives(RegionId(2), RegionId(1)));
    }

    #[test]
    fn region_graph_transitive_closure_2_steps() {
        let mut graph = RegionGraph::new();
        graph.add_outlives(RegionId(1), RegionId(2));
        graph.add_outlives(RegionId(2), RegionId(3));
        graph.close_transitively();
        // After closure, 1 should outlive 2 and 3.
        assert!(graph.outlives(RegionId(1), RegionId(2)));
        assert!(graph.outlives(RegionId(1), RegionId(3)));
        // 2 should outlive 3 but not 1.
        assert!(graph.outlives(RegionId(2), RegionId(3)));
        assert!(!graph.outlives(RegionId(2), RegionId(1)));
    }

    #[test]
    fn region_graph_transitive_closure_chain() {
        let mut graph = RegionGraph::new();
        graph.add_outlives(RegionId(1), RegionId(2));
        graph.add_outlives(RegionId(2), RegionId(3));
        graph.add_outlives(RegionId(3), RegionId(4));
        graph.close_transitively();
        // After closure, 1 should outlive 2, 3, and 4.
        assert!(graph.outlives(RegionId(1), RegionId(2)));
        assert!(graph.outlives(RegionId(1), RegionId(3)));
        assert!(graph.outlives(RegionId(1), RegionId(4)));
    }

    #[test]
    fn region_graph_no_outlives_returns_false() {
        let graph = RegionGraph::new();
        assert!(!graph.outlives(RegionId(1), RegionId(2)));
        assert!(!graph.outlives(RegionId(5), RegionId(3)));
    }
}
