//! Arena allocator for AST nodes.
//!
//! Every node lives in `AstArena.nodes: Vec<NodeData>`, indexed by
//! [`NodeId`]. Parent/child traversal uses arena indices, not Rust
//! references — this keeps the AST `Copy`-friendly and avoids the
//! borrow-checker tax of tree shapes.
//!
//! [`NodeId`]: crate::NodeId

use paideia_as_diagnostics::Span;
use static_assertions::const_assert;
use std::mem::size_of;
use std::ops::Index;

use crate::node_id::NodeId;

/// Discriminant for an AST node's variant.
///
/// Phase-1 ships with a single placeholder variant; PR-16 and later
/// expand this enum with the actual surface-AST kinds (items,
/// expressions, patterns, types). Storage is `#[repr(u32)]` so the
/// per-node size budget is predictable.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u32)]
#[non_exhaustive]
pub enum NodeKind {
    /// Placeholder kind, replaced when concrete variants land.
    Placeholder,
}

/// Per-node arena entry: variant discriminant and source position.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct NodeData {
    /// Variant discriminant.
    pub kind: NodeKind,
    /// Source span this node covers.
    pub span: Span,
}

// AC: `size_of::<NodeData>() <= 32 bytes`. Currently 16 with alignment.
const_assert!(size_of::<NodeData>() <= 32);

impl NodeData {
    /// Construct a `NodeData` directly. Most callers should use
    /// [`AstArena::alloc`] instead.
    #[must_use]
    pub fn new(kind: NodeKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Slab-allocated AST storage for one source file.
///
/// `AstArena` is the owner of every AST node; nodes are referenced by
/// [`NodeId`] for the arena's lifetime. The arena is single-pass write,
/// many-pass read: parsers and lowering passes mint new ids in order,
/// then later passes index into the arena read-only.
#[derive(Debug, Default)]
pub struct AstArena {
    nodes: Vec<NodeData>,
}

impl AstArena {
    /// Construct an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an arena with capacity for `n` nodes pre-reserved.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(n),
        }
    }

    /// Allocate a new node with the given kind and span, returning its
    /// stable [`NodeId`]. IDs are monotonically increasing.
    ///
    /// # Panics
    ///
    /// Panics if the arena would exceed `u32::MAX` nodes — a 4 G AST
    /// is not a realistic target.
    pub fn alloc(&mut self, kind: NodeKind, span: Span) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        id
    }

    /// Number of nodes allocated so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` iff no nodes have been allocated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Borrow the underlying slice of node data.
    #[must_use]
    pub fn as_slice(&self) -> &[NodeData] {
        &self.nodes
    }

    /// Return `None` if `id` was not minted by this arena (i.e., its
    /// index is past the current size).
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&NodeData> {
        self.nodes.get(id.index())
    }
}

impl Index<NodeId> for AstArena {
    type Output = NodeData;

    fn index(&self, id: NodeId) -> &Self::Output {
        &self.nodes[id.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn alloc_returns_increasing_ids() {
        let mut a = AstArena::new();
        let a1 = a.alloc(NodeKind::Placeholder, span());
        let a2 = a.alloc(NodeKind::Placeholder, span());
        let a3 = a.alloc(NodeKind::Placeholder, span());
        assert_eq!(a1.get(), 1);
        assert_eq!(a2.get(), 2);
        assert_eq!(a3.get(), 3);
    }

    #[test]
    fn index_returns_node_data() {
        let mut a = AstArena::new();
        let id = a.alloc(NodeKind::Placeholder, span());
        assert_eq!(a[id].kind, NodeKind::Placeholder);
        assert_eq!(a[id].span, span());
    }

    #[test]
    fn get_returns_none_for_out_of_range() {
        let a = AstArena::new();
        let stray = NodeId::new(7).unwrap();
        assert!(a.get(stray).is_none());
    }

    #[test]
    fn len_and_empty_reflect_state() {
        let mut a = AstArena::new();
        assert!(a.is_empty());
        assert_eq!(a.len(), 0);
        a.alloc(NodeKind::Placeholder, span());
        assert!(!a.is_empty());
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn with_capacity_pre_reserves() {
        // No assertion about Vec internals; just verify the constructor
        // does not panic and produces an empty arena.
        let a = AstArena::with_capacity(64);
        assert_eq!(a.len(), 0);
    }

    #[test]
    fn one_million_allocs_completes() {
        // Informational: the AC mentions <200ms; we do not measure here
        // (no bench harness yet) but we do verify correctness at scale.
        let mut a = AstArena::with_capacity(1_000_000);
        for _ in 0..1_000_000 {
            a.alloc(NodeKind::Placeholder, span());
        }
        assert_eq!(a.len(), 1_000_000);
        let last = NodeId::new(1_000_000).unwrap();
        assert_eq!(a[last].kind, NodeKind::Placeholder);
    }

    #[test]
    fn node_data_size_is_within_budget() {
        // §AC: size_of::<NodeData>() <= 32 bytes. const_assert above is
        // the binding gate; runtime check mirrors it for visibility.
        assert!(size_of::<NodeData>() <= 32);
    }
}
