//! Integration test for item tree building and traversal.

use paideia_as_ast::{AstArena, ItemData, ItemVisitor, NodeKind, walk_item};
use paideia_as_diagnostics::{FileId, Span};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn build_and_walk_full_module_tree() {
    // Build a tiny module tree:
    // Module Test = struct {
    //   let x = <expr>
    //   let y = <expr>
    // }

    let mut arena = AstArena::new();

    // Allocate identifiers (Ident nodes).
    let module_name = arena.alloc(NodeKind::Ident, span());
    let let_x_name = arena.alloc(NodeKind::Ident, span());
    let let_y_name = arena.alloc(NodeKind::Ident, span());

    // Allocate expression placeholders.
    let expr_x = arena.alloc(NodeKind::Placeholder, span());
    let expr_y = arena.alloc(NodeKind::Placeholder, span());

    // Allocate Let items.
    let let_x_id = arena.alloc_item(
        NodeKind::Let,
        span(),
        ItemData::Let {
            mutable: false,
            name: let_x_name,
            generic_params: vec![],
            ty: None,
            value: expr_x,
            doc: None,
        },
    );

    let let_y_id = arena.alloc_item(
        NodeKind::Let,
        span(),
        ItemData::Let {
            mutable: false,
            name: let_y_name,
            generic_params: vec![],
            ty: None,
            value: expr_y,
            doc: None,
        },
    );

    // Allocate Structure body.
    let structure_id = arena.alloc_item(
        NodeKind::Structure,
        span(),
        ItemData::Structure {
            items: vec![let_x_id, let_y_id],
            doc: None,
        },
    );

    // Allocate Module.
    let module_id = arena.alloc_item(
        NodeKind::Module,
        span(),
        ItemData::Module {
            name: module_name,
            sig: None,
            body: structure_id,
            doc: None,
        },
    );

    // Walk the tree and count items.
    struct CountingVisitor {
        item_count: usize,
        module_count: usize,
        structure_count: usize,
        let_count: usize,
    }

    impl ItemVisitor for CountingVisitor {
        fn visit_module(&mut self, _arena: &AstArena, _id: paideia_as_ast::NodeId) {
            self.item_count += 1;
            self.module_count += 1;
        }
        fn visit_structure(&mut self, _arena: &AstArena, _id: paideia_as_ast::NodeId) {
            self.item_count += 1;
            self.structure_count += 1;
        }
        fn visit_let(&mut self, _arena: &AstArena, _id: paideia_as_ast::NodeId) {
            self.item_count += 1;
            self.let_count += 1;
        }
    }

    let mut visitor = CountingVisitor {
        item_count: 0,
        module_count: 0,
        structure_count: 0,
        let_count: 0,
    };

    // Walk the module. In a real traversal, we'd recursively walk children,
    // but for this test we just verify the entry point works.
    walk_item(&mut visitor, &arena, module_id);
    assert_eq!(visitor.module_count, 1, "should visit exactly one module");

    // Manually walk the structure.
    walk_item(&mut visitor, &arena, structure_id);
    assert_eq!(
        visitor.structure_count, 1,
        "should visit exactly one structure"
    );

    // Manually walk the two let bindings.
    walk_item(&mut visitor, &arena, let_x_id);
    walk_item(&mut visitor, &arena, let_y_id);
    assert_eq!(
        visitor.let_count, 2,
        "should visit exactly two let bindings"
    );

    // Total items visited: 1 module + 1 structure + 2 lets = 4
    assert_eq!(visitor.item_count, 4);
}

#[test]
fn functor_with_parameters() {
    let mut arena = AstArena::new();

    // Allocate parameter names and signatures.
    let param1_name = arena.alloc(NodeKind::Ident, span());
    let param1_sig = arena.alloc(NodeKind::Placeholder, span()); // SignatureRef placeholder

    let param2_name = arena.alloc(NodeKind::Ident, span());
    let param2_sig = arena.alloc(NodeKind::Placeholder, span());

    // Allocate FunctorParam items.
    let param1_id = arena.alloc_item(
        NodeKind::FunctorParam,
        span(),
        ItemData::FunctorParam {
            name: param1_name,
            sig: param1_sig,
        },
    );

    let param2_id = arena.alloc_item(
        NodeKind::FunctorParam,
        span(),
        ItemData::FunctorParam {
            name: param2_name,
            sig: param2_sig,
        },
    );

    // Allocate empty Structure body.
    let body_id = arena.alloc_item(
        NodeKind::Structure,
        span(),
        ItemData::Structure {
            items: vec![],
            doc: None,
        },
    );

    // Allocate Functor.
    let functor_id = arena.alloc_item(
        NodeKind::Functor,
        span(),
        ItemData::Functor {
            params: vec![param1_id, param2_id],
            body: body_id,
            doc: None,
        },
    );

    // Verify the structure.
    let functor_data = arena.item_data(functor_id).expect("functor data exists");
    match functor_data {
        ItemData::Functor { params, body, .. } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0], param1_id);
            assert_eq!(params[1], param2_id);
            assert_eq!(*body, body_id);
        }
        _ => panic!("expected Functor variant"),
    }
}

#[test]
fn effect_with_operations() {
    let mut arena = AstArena::new();

    let effect_name = arena.alloc(NodeKind::Ident, span());

    // Allocate operation signatures.
    let op1_name = arena.alloc(NodeKind::Ident, span());
    let op1_type = arena.alloc(NodeKind::Placeholder, span()); // Type placeholder

    let op2_name = arena.alloc(NodeKind::Ident, span());
    let op2_type = arena.alloc(NodeKind::Placeholder, span());

    let op1_id = arena.alloc_item(
        NodeKind::OpSig,
        span(),
        ItemData::OpSig {
            name: op1_name,
            ty: op1_type,
            effect_set: None,
        },
    );

    let op2_id = arena.alloc_item(
        NodeKind::OpSig,
        span(),
        ItemData::OpSig {
            name: op2_name,
            ty: op2_type,
            effect_set: None,
        },
    );

    // Allocate Effect.
    let effect_id = arena.alloc_item(
        NodeKind::Effect,
        span(),
        ItemData::Effect {
            name: effect_name,
            ops: vec![op1_id, op2_id],
            doc: None,
        },
    );

    // Verify the structure.
    let effect_data = arena.item_data(effect_id).expect("effect data exists");
    match effect_data {
        ItemData::Effect {
            name: _,
            ops,
            doc: _,
        } => {
            assert_eq!(ops.len(), 2);
        }
        _ => panic!("expected Effect variant"),
    }
}
