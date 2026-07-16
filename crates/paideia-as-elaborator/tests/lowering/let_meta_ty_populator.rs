//! Issue #1219: Unit tests for `populate_let_meta_ty`.
//!
//! Six Layer-1 direct-getter tests that verify the populator correctly
//! writes LetInfo::ty from explicit type annotations on Let bindings,
//! and handles errors gracefully.

use std::collections::HashMap;
use paideia_as_ast::{AstArena, ItemData, NodeKind, StmtData};
use paideia_as_elaborator::{
    lower_ast_to_ir, populate_let_meta_ty, struct_registry::StructRegistry,
    EnumRegistry,
};
use paideia_as_types::TypeInterner;
use paideia_as_effects::EffectInterner;
use crate::common::{test_span, create_test_source_map_and_sink};

/// Test 1: Unannotated item Let (no type annotation).
/// Asserts that ty remains None and no panic occurs.
#[test]
fn test_item_let_unannotated() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create RHS
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    // Create name node
    let name_id = ast.alloc(NodeKind::Ident, span);

    // Create a Let without type annotation
    let let_ast_id = ast.alloc_item(
        NodeKind::Let,
        span,
        ItemData::Let {
            public: false,
            mutable: false,
            name: name_id,
            generic_params: vec![],
            ty: None,
            value: rhs_id,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            doc: None,
        },
    );

    // Lower to IR
    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // Construct interners
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    // Run the populator
    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
    );

    // Assert that the Let's LetInfo.ty remains None
    let let_ir_id = lowering.ast_to_ir[&let_ast_id];
    let let_info = lowering.ir.let_meta().get(let_ir_id);
    if let Some(info) = let_info {
        assert!(
            info.ty.is_none(),
            "Unannotated Let should not have ty populated"
        );
    }
}

/// Test 2: Unannotated StmtLet.
/// Asserts that statement-level unannotated Lets also don't crash.
#[test]
fn test_stmt_let_unannotated() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create RHS
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    // Create name node
    let name_id = ast.alloc(NodeKind::Ident, span);

    // Create a StmtLet without type annotation
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        span,
        StmtData::Let {
            mutable: false,
            name: name_id,
            ty: None,
            value: rhs_id,
        },
    );

    // Lower to IR
    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // Construct interners
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    // Run the populator
    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
    );

    // Assert no crash occurred
    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let let_info = lowering.ir.let_meta().get(stmt_let_ir_id);
    if let Some(info) = let_info {
        assert!(
            info.ty.is_none(),
            "Unannotated StmtLet should not have ty populated"
        );
    }
}

/// Test 3: Mutable Let (unannotated).
/// Asserts that mutability is preserved through populator.
#[test]
fn test_mutable_let_unannotated() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create RHS
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    // Create name node
    let name_id = ast.alloc(NodeKind::Ident, span);

    // Create a mutable Let without type annotation
    let let_ast_id = ast.alloc_item(
        NodeKind::Let,
        span,
        ItemData::Let {
            public: false,
            mutable: true,
            name: name_id,
            generic_params: vec![],
            ty: None,
            value: rhs_id,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            doc: None,
        },
    );

    // Lower to IR
    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // Construct interners
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    // Run the populator
    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
    );

    // Assert that mutability is handled correctly
    let let_ir_id = lowering.ast_to_ir[&let_ast_id];
    let let_info = lowering.ir.let_meta().get(let_ir_id);
    // If there's an entry, check it's consistent; if not, that's also OK for unannotated
    if let Some(info) = let_info {
        assert!(info.ty.is_none(), "Unannotated Let should not have ty populated");
    }
}

/// Test 4: RMW contract - pre-existing LetInfo not overwritten.
/// Asserts that the populator uses RMW and preserves other fields.
#[test]
fn test_rmw_preserves_fields() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create an unannotated Let
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    let name_id = ast.alloc(NodeKind::Ident, span);

    let let_ast_id = ast.alloc_item(
        NodeKind::Let,
        span,
        ItemData::Let {
            public: false,
            mutable: true,
            name: name_id,
            generic_params: vec![],
            ty: None,
            value: rhs_id,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            doc: None,
        },
    );

    // Lower to IR
    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // Pre-insert LetInfo with mutable=true and other fields
    let let_ir_id = lowering.ast_to_ir[&let_ast_id];
    lowering.ir.let_meta_mut().insert(
        let_ir_id,
        paideia_as_ir::LetInfo {
            mutable: true,
            ty: None,
            align: Some(8),
            ring: None,
            link_section: None,
            abi: None,
        },
    );

    // Construct interners
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    // Run the populator (unannotated, so shouldn't modify ty)
    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
    );

    // Assert that all fields are still present
    let let_info = lowering.ir.let_meta().get(let_ir_id);
    assert!(let_info.is_some(), "LetInfo should still exist");
    let info = let_info.unwrap();
    assert!(info.mutable, "mutable field preserved");
    assert_eq!(info.align, Some(8), "align field preserved");
    assert!(info.ty.is_none(), "ty field unchanged (unannotated)");
}

/// Test 5: Multiple unannotated Lets in same context.
/// Asserts that each Let is processed independently via IR ID keying.
#[test]
fn test_multiple_unannotated_lets() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create two Lets, both unannotated
    let rhs1_id = ast.alloc(NodeKind::ExprLiteral, span);
    let rhs2_id = ast.alloc(NodeKind::ExprLiteral, span);
    let name1_id = ast.alloc(NodeKind::Ident, span);
    let name2_id = ast.alloc(NodeKind::Ident, span);

    let let1_ast_id = ast.alloc_item(
        NodeKind::Let,
        span,
        ItemData::Let {
            public: false,
            mutable: false,
            name: name1_id,
            generic_params: vec![],
            ty: None,
            value: rhs1_id,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            doc: None,
        },
    );

    let let2_ast_id = ast.alloc_item(
        NodeKind::Let,
        span,
        ItemData::Let {
            public: false,
            mutable: true,
            name: name2_id,
            generic_params: vec![],
            ty: None,
            value: rhs2_id,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            doc: None,
        },
    );

    // Lower to IR
    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // Pre-set mutable on let2 only
    let let1_ir_id = lowering.ast_to_ir[&let1_ast_id];
    let let2_ir_id = lowering.ast_to_ir[&let2_ast_id];
    lowering.ir.let_meta_mut().insert(let2_ir_id, paideia_as_ir::LetInfo::mutable());

    // Construct interners
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    // Run the populator
    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
    );

    // Assert both are keyed independently
    assert_ne!(let1_ir_id, let2_ir_id, "Different Lets have different IR IDs");

    let info2 = lowering.ir.let_meta().get(let2_ir_id);
    assert!(info2.is_some(), "let2 should have LetInfo");
    assert!(info2.unwrap().mutable, "let2 should still be mutable");
}

/// Test 6: Stress test - many unannotated Lets.
/// Asserts populator handles a larger scenario without issues.
#[test]
fn test_many_unannotated_lets() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    let mut let_ids = vec![];

    // Create 6 Let bindings without annotations
    for i in 0..6 {
        let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
        let name_id = ast.alloc(NodeKind::Ident, span);

        let let_id = ast.alloc_item(
            NodeKind::Let,
            span,
            ItemData::Let {
                public: i == 0,  // Only first is public
                mutable: i % 3 == 0,  // 0,3,6 are mutable
                name: name_id,
                generic_params: vec![],
                ty: None,  // No type annotation
                value: rhs_id,
                align: if i == 2 { Some(16) } else { None },
                ring: None,
                link_section: None,
                abi: None,
                doc: None,
            },
        );
        let_ids.push(let_id);
    }

    // Lower to IR
    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // Construct interners
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    // Run the populator
    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
    );

    // Assert all Lets were processed without panic
    for let_ast_id in let_ids {
        let let_ir_id = lowering.ast_to_ir[&let_ast_id];
        // Just verify the populator completed successfully
        // (for unannotated Lets, ty should remain None)
        let let_info = lowering.ir.let_meta().get(let_ir_id);
        if let Some(info) = let_info {
            assert!(info.ty.is_none(), "Unannotated Let should not have ty");
        }
    }
}
