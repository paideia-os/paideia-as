//! Arena allocator for IR nodes (parallels [`paideia_as_ast::AstArena`]).
//!
//! [`paideia_as_ast::AstArena`]: paideia_as_ast::AstArena

use paideia_as_diagnostics::Span;
use std::ops::Index;

use crate::node::{IrKind, IrNodeData, IrNodeId};

/// Slab-allocated IR storage for one source file.
#[derive(Debug, Default)]
pub struct IrArena {
    nodes: Vec<IrNodeData>,
}

impl IrArena {
    /// Construct an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an arena pre-reserved for `n` nodes.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(n),
        }
    }

    /// Allocate a new node with the supplied kind and span. The new node
    /// inherits the default `lin_class = Unrestricted` and
    /// `effect_row = EMPTY`. The elaborator may mutate those fields in
    /// later passes.
    ///
    /// Returns the freshly-allocated [`IrNodeId`].
    pub fn alloc(&mut self, kind: IrKind, span: Span) -> IrNodeId {
        let next = self.nodes.len() + 1;
        let id = IrNodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("non-zero next index");
        self.nodes.push(IrNodeData::new(kind, span));
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
    pub fn as_slice(&self) -> &[IrNodeData] {
        &self.nodes
    }

    /// Return `None` if `id` was not minted by this arena.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&IrNodeData> {
        self.nodes.get(id.index())
    }

    /// Mutable access to the node data; the elaborator updates `lin_class`
    /// and `effect_row` through this.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut IrNodeData> {
        self.nodes.get_mut(id.index())
    }
}

impl Index<IrNodeId> for IrArena {
    type Output = IrNodeData;

    fn index(&self, id: IrNodeId) -> &Self::Output {
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
        let mut a = IrArena::new();
        let a1 = a.alloc(IrKind::Placeholder, span());
        let a2 = a.alloc(IrKind::Module, span());
        assert_eq!(a1.get(), 1);
        assert_eq!(a2.get(), 2);
    }

    #[test]
    fn index_returns_node_data() {
        let mut a = IrArena::new();
        let id = a.alloc(IrKind::Lambda, span());
        assert_eq!(a[id].kind, IrKind::Lambda);
        assert_eq!(a[id].span, span());
    }

    #[test]
    fn get_mut_allows_elaborator_to_update_class_and_effects() {
        let mut a = IrArena::new();
        let id = a.alloc(IrKind::Let, span());
        let d = a.get_mut(id).unwrap();
        d.lin_class = crate::node::LinClass::Linear;
        d.effect_row = crate::node::EffectRowId(7);
        assert_eq!(a[id].lin_class, crate::node::LinClass::Linear);
        assert_eq!(a[id].effect_row, crate::node::EffectRowId(7));
    }

    #[test]
    fn len_and_empty_track_state() {
        let mut a = IrArena::new();
        assert!(a.is_empty());
        a.alloc(IrKind::Placeholder, span());
        assert_eq!(a.len(), 1);
    }
}
