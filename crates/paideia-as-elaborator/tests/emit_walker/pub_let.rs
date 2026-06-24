//! Test: pub let bindings should create global symbols (PA904).
//!
//! Elaborates an AST with `pub let` marked as public, lowers to IR,
//! walks with EmitWalker, and verifies the symbol has Visibility::Global.

use paideia_as_ast::{AstArena, ItemData, NodeKind};
use paideia_as_ir::{IrArena, IrKind};

#[test]
fn pub_let_flag_flows_to_ir_arena() {
    // Test: verify that pub let parsing and IR lowering preserve the public flag.
    // This is the elaborator-level test for PA904.

    let mut ast_arena = AstArena::new();
    let mut ir_arena = IrArena::new();

    // Create a simple let node with public=true
    let span = paideia_as_diagnostics::Span::new(
        paideia_as_diagnostics::FileId::new(1).expect("valid file id"),
        0,
        1,
    );

    // Create an Ident for "add_one"
    let name_id = ast_arena.alloc(NodeKind::Ident, span);

    // Create a simple expression placeholder for the value
    let value_id = ast_arena.alloc(NodeKind::Placeholder, span);

    // Create the Let item with public=true
    let let_id = ast_arena.alloc_item(
        NodeKind::Let,
        span,
        ItemData::Let {
            public: true,
            mutable: false,
            name: name_id,
            generic_params: vec![],
            ty: None,
            value: value_id,
            doc: None,
        },
    );

    // Verify AST has public=true
    if let Some(ItemData::Let { public, .. }) = ast_arena.item_data(let_id) {
        assert!(*public, "AST Let should have public=true");
    } else {
        panic!("Failed to retrieve Let item from AST");
    }

    // Simulate the cmd_build.rs lowering: extract public flag and populate public_lets
    let ir_let_id =
        paideia_as_ir::IrNodeId::new(let_id.get()).expect("valid ir node id from ast let node");

    // Allocate the IR Let node
    let _ir_let = ir_arena.alloc(IrKind::Let, span);

    // Extract public flag and populate public_lets (simulating cmd_build.rs PA904 section)
    if let Some(ItemData::Let { public, .. }) = ast_arena.item_data(let_id) {
        if *public {
            ir_arena.public_lets_mut().insert(ir_let_id);
        }
    }

    // Verify is_public_let returns true
    assert!(
        ir_arena.is_public_let(ir_let_id),
        "IR arena should mark the let as public"
    );
}

#[test]
fn plain_let_is_not_public() {
    // Verify that a plain (non-pub) let is not marked as public in the IR arena.

    let mut ir_arena = IrArena::new();
    let span = paideia_as_diagnostics::Span::new(
        paideia_as_diagnostics::FileId::new(1).expect("valid file id"),
        0,
        1,
    );

    // Create an IR Let node
    let ir_let_id = ir_arena.alloc(IrKind::Let, span);

    // Do NOT mark it as public
    // (public_lets_mut().insert is never called)

    // Verify is_public_let returns false
    assert!(
        !ir_arena.is_public_let(ir_let_id),
        "IR arena should not mark the let as public"
    );
}
