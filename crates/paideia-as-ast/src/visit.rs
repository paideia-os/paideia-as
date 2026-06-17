//! Visitor trait and dispatch for item traversal.
//!
//! [`ItemVisitor`] is a trait for traversing item nodes. [`walk_item`]
//! dispatches by node kind to the appropriate visitor method.

use crate::{AstArena, NodeId, NodeKind};

/// Visitor trait for traversing item nodes.
///
/// Implement this trait to perform actions on specific item kinds. Each
/// `visit_*` method receives the arena and the node ID for the item being
/// visited. Implement only the methods you need; defaults are no-ops.
pub trait ItemVisitor {
    /// Visit a Module item.
    fn visit_module(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Signature item.
    fn visit_signature(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Structure item.
    fn visit_structure(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Functor item.
    fn visit_functor(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a FunctorParam item.
    fn visit_functor_param(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an Effect item.
    fn visit_effect(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an OpSig item.
    fn visit_op_sig(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Capability item.
    fn visit_capability(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Let item.
    fn visit_let(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Struct item.
    fn visit_struct(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an Enum item.
    fn visit_enum(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an UnsafeBlock item.
    fn visit_unsafe_block(&mut self, _arena: &AstArena, _id: NodeId) {}
}

/// Dispatch visitor call by node kind.
///
/// Looks up the node in the arena, checks its kind, and calls the
/// appropriate `visit_*` method on the visitor. Does nothing for
/// non-item kinds.
pub fn walk_item<V: ItemVisitor>(visitor: &mut V, arena: &AstArena, id: NodeId) {
    let node_data = match arena.get(id) {
        Some(nd) => nd,
        None => return,
    };

    match node_data.kind {
        NodeKind::Module => visitor.visit_module(arena, id),
        NodeKind::Signature => visitor.visit_signature(arena, id),
        NodeKind::Structure => visitor.visit_structure(arena, id),
        NodeKind::Functor => visitor.visit_functor(arena, id),
        NodeKind::FunctorParam => visitor.visit_functor_param(arena, id),
        NodeKind::Effect => visitor.visit_effect(arena, id),
        NodeKind::OpSig => visitor.visit_op_sig(arena, id),
        NodeKind::Capability => visitor.visit_capability(arena, id),
        NodeKind::Let => visitor.visit_let(arena, id),
        NodeKind::Struct => visitor.visit_struct(arena, id),
        NodeKind::Enum => visitor.visit_enum(arena, id),
        NodeKind::UnsafeBlock => visitor.visit_unsafe_block(arena, id),
        // Non-item kinds: do nothing.
        NodeKind::Placeholder | NodeKind::Ident => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn visit_module_dispatches() {
        use crate::ItemData;

        let mut arena = AstArena::new();
        let name_id = arena.alloc(NodeKind::Ident, span());
        let body_id = arena.alloc(NodeKind::Structure, span());

        let module_id = arena.alloc_item(
            NodeKind::Module,
            span(),
            ItemData::Module {
                name: name_id,
                sig: None,
                body: body_id,
                doc: None,
            },
        );

        struct CountingVisitor {
            module_count: usize,
        }

        impl ItemVisitor for CountingVisitor {
            fn visit_module(&mut self, _arena: &AstArena, _id: NodeId) {
                self.module_count += 1;
            }
        }

        let mut visitor = CountingVisitor { module_count: 0 };
        walk_item(&mut visitor, &arena, module_id);
        assert_eq!(visitor.module_count, 1);
    }

    #[test]
    fn visit_non_item_is_no_op() {
        struct CountingVisitor {
            call_count: usize,
        }

        impl ItemVisitor for CountingVisitor {
            fn visit_let(&mut self, _arena: &AstArena, _id: NodeId) {
                self.call_count += 1;
            }
        }

        let mut arena = AstArena::new();
        let placeholder_id = arena.alloc(NodeKind::Placeholder, span());

        let mut visitor = CountingVisitor { call_count: 0 };
        walk_item(&mut visitor, &arena, placeholder_id);
        assert_eq!(visitor.call_count, 0); // No dispatch to visit_let
    }

    #[test]
    fn walk_item_ignores_out_of_range_ids() {
        struct CountingVisitor {
            call_count: usize,
        }

        impl ItemVisitor for CountingVisitor {
            fn visit_module(&mut self, _arena: &AstArena, _id: NodeId) {
                self.call_count += 1;
            }
        }

        let arena = AstArena::new();
        let stray_id = NodeId::new(999).unwrap();

        let mut visitor = CountingVisitor { call_count: 0 };
        walk_item(&mut visitor, &arena, stray_id);
        assert_eq!(visitor.call_count, 0); // No panic, no dispatch
    }
}
