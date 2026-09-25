//! R226.M7 — synthetic TypedGraph substrate for Datalog graph
//! traversal.
//!
//! # Position in the pipeline
//!
//! ```text
//!     TypedGraph (nodes + labelled edges)
//!            │
//!            ▼
//!   Evaluator::register_graph_predicate — bind (predicate, graph, edge_label)
//!            │
//!            ▼
//!   Evaluator::run_query_with_graph    — pre-seed graph tuples, then fixpoint
//! ```
//!
//! # What ships at M7 (and what does not)
//!
//! The real FS TypedGraph (paideia-os's on-disk typed graph store —
//! R226.M7-followup) is **not** wired here. This module is the plug-in
//! shape a Datalog caller uses today: a `TypedGraph` built in-memory
//! from `add_node` / `add_edge` calls, registered under a predicate
//! name on an [`crate::eval::Evaluator`], and consulted at query time.
//! The follow-up milestone will swap this substrate for an FS-backed
//! reader without disturbing the [`crate::eval::Evaluator`] surface —
//! the extension points are the constructor and the `add_*`
//! mutators; every code path *downstream* of `Arc<TypedGraph>` reads
//! only through the accessors on this type.
//!
//! # Semantics of one registered predicate
//!
//! For a call
//!
//! ```text
//!     evaluator.register_graph_predicate("edge", graph.clone(), "child");
//! ```
//!
//! and a query goal `edge(?x, ?y)`, the evaluator behaves as if the
//! program had an implicit fact `edge(from, to)` for **every** edge in
//! `graph` whose label is `"child"`. That is:
//!
//! * `edge(?x, ?y)` — enumerates all edges with the matching label as
//!   `(from, to)` tuples.
//! * `edge(a, ?x)` — enumerates every child of `a` under the label.
//! * `edge(a, b)`  — reduces to a binary membership check.
//!
//! # Pre-seed vs. per-atom dispatch — the M7 shape choice
//!
//! The task description offered two evaluator-integration paths:
//!
//! 1. Splice a per-atom dispatch into the fixpoint's tuple-derivation
//!    loop that consults the graph directly whenever a body atom
//!    mentions a registered predicate.
//! 2. Pre-seed the graph-derived tuples into the initial database and
//!    let the standard seminaïve fixpoint consume them as ordinary
//!    extensional facts.
//!
//! M7 picks **(2)**. Rationale:
//!
//! * **Minimal diff.** The evaluator already carries a session-EDB
//!   overlay path (R226.M8, [`crate::session_edb::SessionEdb`]). The
//!   graph-predicate registration reuses that path by projecting the
//!   registered graphs into a synthesized [`SessionEdb`] before the
//!   fixpoint runs — no new branch inside [`crate::eval::derive_round`],
//!   no new stratification interaction, no new fingerprint or progress
//!   emission code path.
//! * **Cycle safety for free.** Fixpoint dedup through the underlying
//!   `HashSet<Vec<Value>>` handles cyclic graphs naturally: pre-seeding
//!   enumerates *edges*, not *walks*, so no BFS or visited-set is
//!   required inside this module. Test `r226m7-graph-04` pins that a
//!   cyclic graph terminates without divergence.
//! * **Preservation of rule semantics.** A rule that combines a
//!   registered predicate with a rule-derived predicate (test
//!   `r226m7-graph-06`) works with no special case: once the graph
//!   tuples land in the EDB, they are indistinguishable from any other
//!   extensional fact.
//! * **Answer-preservation.** Every query semantic in the M7 test
//!   corpus (bound-first, both-free, both-bound, multi-label, multi-
//!   graph) is answered by the standard [`crate::eval::query`]
//!   engine over a database that already contains the graph's edges as
//!   tuples — no per-atom code path is required to differentiate the
//!   argument-binding shapes.
//!
//! The trade-off is memory: pre-seed materialises every matching edge
//! up front. For the M7 in-memory substrate the graphs are small (test
//! corpus <20 nodes), so this is a non-cost. When the FS-backed
//! substrate arrives, the R226.M7-followup will revisit — likely by
//! deferring materialisation until a body atom actually mentions the
//! predicate, on the same registration surface.
//!
//! # Fingerprints
//!
//! Test corpus tags `r226m7-graph-01`..`r226m7-graph-10` (10 fixtures).

use std::collections::HashMap;

/// One node in a [`TypedGraph`].
///
/// Both fields are public so a caller running an assertion over a
/// `children_of` result can read `type_name` directly (test
/// `r226m7-graph-07`). A future FS-backed variant may replace the
/// storage but will keep the same accessor shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphNode {
    /// Unique identifier — the key under which this node was added.
    pub id: String,
    /// Type tag — an opaque label the caller uses to distinguish node
    /// kinds (e.g. "file", "dir", "symlink" in the FS substrate).
    pub type_name: String,
}

/// One directed, labelled edge in a [`TypedGraph`].
///
/// Edges are keyed on their source (`from`); `label` distinguishes
/// overlapping edges between the same pair of nodes (a "parent" edge
/// and a "sibling" edge, say). Test `r226m7-graph-05` pins the
/// per-label filter behaviour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// Source node id (matched against [`GraphNode::id`]).
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Label — the discriminator a registered graph-predicate matches
    /// against.
    pub label: String,
}

/// An in-memory typed, directed multigraph — the M7 substrate a
/// Datalog evaluator consults for graph-predicate resolution.
///
/// See the [`module-level docs`](self) for the pre-seed shape choice
/// and the substrate-swap plan for R226.M7-followup.
#[derive(Clone, Debug, Default)]
pub struct TypedGraph {
    nodes: HashMap<String, GraphNode>,
    /// Outgoing edges indexed by source id — mirrors the walk pattern
    /// [`Self::children_of`] performs.
    edges: HashMap<String, Vec<Edge>>,
}

impl TypedGraph {
    /// Fresh, empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) the node identified by `id` with the given
    /// `type_name`. Idempotent by id — re-adding an id overwrites the
    /// prior `type_name`, so a caller can up-cast a node's kind
    /// without a separate mutator.
    pub fn add_node(&mut self, id: impl Into<String>, type_name: impl Into<String>) {
        let id = id.into();
        let node = GraphNode {
            id: id.clone(),
            type_name: type_name.into(),
        };
        self.nodes.insert(id, node);
    }

    /// Add a directed, labelled edge from `from` to `to`. Keyed on
    /// `from` in the outgoing-adjacency index. Multi-edges with the
    /// same `(from, to, label)` triple accumulate (the graph is a
    /// multigraph); the Datalog dedup layer collapses identical
    /// tuples during fixpoint, so an accidental duplicate is not
    /// observable through a query.
    pub fn add_edge(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        label: impl Into<String>,
    ) {
        let from = from.into();
        let edge = Edge {
            from: from.clone(),
            to: to.into(),
            label: label.into(),
        };
        self.edges.entry(from).or_default().push(edge);
    }

    /// Walk outgoing edges of `id` whose label matches `edge_label`
    /// and return the referenced child nodes. Silently drops edges
    /// whose target id has no corresponding node — the graph is a
    /// caller-populated structure and dangling ids are the caller's
    /// invariant to hold; a downstream Datalog dispatch would surface
    /// the same drop as a missing predicate tuple.
    ///
    /// Order follows insertion order of the outgoing-edge list.
    pub fn children_of(&self, id: &str, edge_label: &str) -> Vec<&GraphNode> {
        let Some(edges) = self.edges.get(id) else {
            return Vec::new();
        };
        edges
            .iter()
            .filter(|e| e.label == edge_label)
            .filter_map(|e| self.nodes.get(&e.to))
            .collect()
    }

    /// Read a node by id — used by tests and by validation code that
    /// wants to confirm a `children_of` result's `type_name` without
    /// re-walking the edge list.
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Enumerate every edge whose label matches `edge_label`. Order is
    /// grouped by source id then insertion order within a source; the
    /// evaluator's downstream `HashSet` dedup makes any total order
    /// observationally equivalent, but this ordering keeps the
    /// pre-seed pass deterministic across runs on the same graph.
    ///
    /// Exposed on the public surface so a caller inspecting the
    /// per-label projection (e.g. a REPL `.graph-edges child`
    /// diagnostic) can enumerate without going through the evaluator.
    /// The R226.M7-followup FS variant will keep the same signature.
    pub fn edges_with_label<'a>(&'a self, edge_label: &str) -> Vec<&'a Edge> {
        let mut out: Vec<&Edge> = Vec::new();
        // Iterate sources in id order so the pre-seed's insertion into
        // the underlying `HashSet<Vec<Value>>` is order-stable at
        // debug-log time. Fixpoint semantics don't depend on this; the
        // determinism helps a reader reading a delta trace.
        let mut sources: Vec<&String> = self.edges.keys().collect();
        sources.sort();
        for src in sources {
            if let Some(edges) = self.edges.get(src) {
                for edge in edges {
                    if edge.label == edge_label {
                        out.push(edge);
                    }
                }
            }
        }
        out
    }

    /// Number of nodes currently in the graph. Convenience for tests
    /// (a fixture assembling a fixed-size graph asserts on this
    /// before running its query).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Total edge count across every source. Multi-edges count once
    /// per stored insertion.
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_has_no_children() {
        let g = TypedGraph::new();
        assert!(g.children_of("nope", "any").is_empty());
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn children_of_filters_by_label() {
        let mut g = TypedGraph::new();
        g.add_node("a", "dir");
        g.add_node("b", "file");
        g.add_node("c", "file");
        g.add_edge("a", "b", "child");
        g.add_edge("a", "c", "sibling");
        let child = g.children_of("a", "child");
        assert_eq!(child.len(), 1);
        assert_eq!(child[0].id, "b");
        let sib = g.children_of("a", "sibling");
        assert_eq!(sib.len(), 1);
        assert_eq!(sib[0].id, "c");
    }

    #[test]
    fn children_of_skips_dangling_target() {
        let mut g = TypedGraph::new();
        g.add_node("a", "dir");
        g.add_edge("a", "ghost", "child");
        assert!(g.children_of("a", "child").is_empty());
    }

    #[test]
    fn edges_with_label_enumerates_across_sources() {
        let mut g = TypedGraph::new();
        g.add_node("a", "dir");
        g.add_node("b", "dir");
        g.add_node("c", "file");
        g.add_edge("a", "c", "child");
        g.add_edge("b", "c", "child");
        g.add_edge("a", "b", "sibling");
        let all = g.edges_with_label("child");
        assert_eq!(all.len(), 2);
    }
}
