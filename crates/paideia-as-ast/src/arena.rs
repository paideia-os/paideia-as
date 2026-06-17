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
/// Phase-1 ships with item-level variants (PR-16); expressions,
/// patterns, and types land in later PRs. Storage is `#[repr(u32)]`
/// so the per-node size budget is predictable.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u32)]
#[non_exhaustive]
pub enum NodeKind {
    /// Placeholder kind for non-item nodes (expressions, types, patterns, etc.).
    Placeholder,
    /// Identifier node.
    Ident,
    /// Module declaration.
    Module,
    /// Signature declaration.
    Signature,
    /// Structure (module body).
    Structure,
    /// Functor (parameterized module body).
    Functor,
    /// Functor parameter.
    FunctorParam,
    /// Effect declaration.
    Effect,
    /// Operation signature within an effect.
    OpSig,
    /// Capability declaration.
    Capability,
    /// Let binding.
    Let,
    /// Struct type declaration.
    Struct,
    /// Enum type declaration.
    Enum,
    /// Unsafe block.
    UnsafeBlock,
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
///
/// The `items` parallel vector stores optional item-specific data for
/// nodes that have it (see [`ItemData`]). For non-item nodes, the
/// corresponding slot is `None`.
///
/// [`ItemData`]: crate::ItemData
#[derive(Debug, Default)]
pub struct AstArena {
    nodes: Vec<NodeData>,
    items: Vec<Option<Box<crate::ItemData>>>,
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
            items: Vec::with_capacity(n),
        }
    }

    /// Allocate a new node with the given kind and span, returning its
    /// stable [`NodeId`]. IDs are monotonically increasing.
    ///
    /// For non-item nodes, the corresponding slot in `items` is set to
    /// `None`. Use [`alloc_item`] for item nodes.
    ///
    /// # Panics
    ///
    /// Panics if the arena would exceed `u32::MAX` nodes — a 4 G AST
    /// is not a realistic target.
    ///
    /// [`alloc_item`]: Self::alloc_item
    pub fn alloc(&mut self, kind: NodeKind, span: Span) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(None);
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

    /// Allocate an item node with its structured payload, returning its
    /// stable [`NodeId`].
    ///
    /// The `data` is stored in the parallel `items` vector.
    pub fn alloc_item(&mut self, kind: NodeKind, span: Span, data: crate::ItemData) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(Some(Box::new(data)));
        id
    }

    /// Look up the item-data for a node, returning `None` for non-item
    /// nodes or out-of-range ids.
    #[must_use]
    pub fn item_data(&self, id: NodeId) -> Option<&crate::ItemData> {
        self.items.get(id.index())?.as_ref().map(|b| b.as_ref())
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

    #[test]
    fn alloc_for_non_item_does_not_populate_item_data() {
        let mut a = AstArena::new();
        let id = a.alloc(NodeKind::Placeholder, span());
        assert!(a.item_data(id).is_none());
    }

    #[test]
    fn alloc_item_populates_item_data() {
        use crate::ItemData;
        let mut a = AstArena::new();
        // Allocate a Module with a non-existent name and body as a test.
        // In real parsing, these would point to actual Ident and Structure nodes.
        let name_id = NodeId::new(1).unwrap(); // Pretend this is an Ident node
        let body_id = NodeId::new(2).unwrap(); // Pretend this is a Structure node
        let module_id = a.alloc_item(
            NodeKind::Module,
            span(),
            ItemData::Module {
                name: name_id,
                sig: None,
                body: body_id,
                doc: None,
            },
        );
        let item = a.item_data(module_id).expect("item data should exist");
        match item {
            ItemData::Module {
                name,
                sig,
                body,
                doc,
            } => {
                assert_eq!(*name, name_id);
                assert!(sig.is_none());
                assert_eq!(*body, body_id);
                assert!(doc.is_none());
            }
            _ => panic!("expected Module variant"),
        }
    }
}
