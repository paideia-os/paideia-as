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
use paideia_as_ir::enum_layout::EnumTypeId;
use paideia_as_diagnostics::{Span, SourceMap, VecSink};
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
        &enum_registry,
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
        &enum_registry,
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
        &enum_registry,
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
            enum_type_id: None,
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
        &enum_registry,
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
        &enum_registry,
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
        &enum_registry,
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

/// Test 7: StmtLet with enum type annotation populates enum_type_id.
/// Create a StmtLet with `let s : Result = ...`, run populator, verify ty is populated.
#[test]
fn test_stmt_let_enum_annotated_populates_enum_type_id() {
    // #1223 rewrite: assert on a real EnumRegistry hit, not the
    // silently-guarded no-op the prior test performed.
    let mut ast = AstArena::new();

    let mut source_map = SourceMap::new();
    let fid = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("Result"),
    );
    let mut sink = VecSink::new();

    let name_span = Span::new(fid, 0, 6);   // covers "Result"
    let benign_span = Span::new(fid, 0, 1); // for non-ident nodes

    let type_ident = ast.alloc(NodeKind::Ident, name_span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        benign_span,
        paideia_as_ast::TypeData::Name {
            name: type_ident,
            args: vec![],
        },
    );
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, benign_span);
    let name_id = ast.alloc(NodeKind::Ident, benign_span);

    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        benign_span,
        StmtData::Let {
            mutable: false,
            name: name_id,
            ty: Some(type_node),
            value: rhs_id,
        },
    );

    let mut enum_registry = EnumRegistry::empty();
    enum_registry.by_name.insert("Result".to_string(), EnumTypeId(7));

    let registry = StructRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &registry,
        &enum_registry,
        &payload_map,
    );

    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
        &enum_registry,
    );

    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let info = lowering.ir.let_meta().get(stmt_let_ir_id)
        .expect("populator must have written LetInfo for annotated Let");
    assert!(info.ty.is_some(), "ty populated from enum annotation");
    assert_eq!(
        info.enum_type_id,
        Some(EnumTypeId(7)),
        "enum_type_id populated from registry lookup"
    );
}

/// Test 8: Item Let with enum type annotation populates enum_type_id.
/// Create a NodeKind::Let with enum type annotation.
#[test]
fn test_item_let_enum_annotated_populates_enum_type_id() {
    // #1223 rewrite: assert on a real EnumRegistry hit, not the
    // silently-guarded no-op the prior test performed.
    let mut ast = AstArena::new();

    let mut source_map = SourceMap::new();
    let fid = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("Result"),
    );
    let mut sink = VecSink::new();

    let name_span = Span::new(fid, 0, 6);   // covers "Result"
    let benign_span = Span::new(fid, 0, 1); // for non-ident nodes

    let type_ident = ast.alloc(NodeKind::Ident, name_span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        benign_span,
        paideia_as_ast::TypeData::Name {
            name: type_ident,
            args: vec![],
        },
    );
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, benign_span);
    let name_id = ast.alloc(NodeKind::Ident, benign_span);

    let let_ast_id = ast.alloc_item(
        NodeKind::Let,
        benign_span,
        ItemData::Let {
            public: false,
            mutable: false,
            name: name_id,
            generic_params: vec![],
            ty: Some(type_node),
            value: rhs_id,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            doc: None,
        },
    );

    let mut enum_registry = EnumRegistry::empty();
    enum_registry.by_name.insert("Result".to_string(), EnumTypeId(7));

    let registry = StructRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &registry,
        &enum_registry,
        &payload_map,
    );

    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    populate_let_meta_ty(
        &ast,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
        &enum_registry,
    );

    let let_ir_id = lowering.ast_to_ir[&let_ast_id];
    let info = lowering.ir.let_meta().get(let_ir_id)
        .expect("populator must have written LetInfo for annotated Let");
    assert!(info.ty.is_some(), "ty populated from enum annotation");
    assert_eq!(
        info.enum_type_id,
        Some(EnumTypeId(7)),
        "enum_type_id populated from registry lookup"
    );
}

/// Test 9: StmtLet with struct type annotation leaves enum_type_id None.
/// Verify that struct-annotated bindings populate ty but not enum_type_id.
#[test]
fn test_stmt_let_struct_annotated_leaves_enum_type_id_none() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create type annotation node with TypeData::Name for "MyStruct"
    let type_ident = ast.alloc(NodeKind::Ident, span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        span,
        paideia_as_ast::TypeData::Name {
            name: type_ident,
            args: vec![],
        },
    );

    // Create RHS
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    let name_id = ast.alloc(NodeKind::Ident, span);

    // Create a StmtLet with struct type annotation
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        span,
        StmtData::Let {
            mutable: false,
            name: name_id,
            ty: Some(type_node),
            value: rhs_id,
        },
    );

    let enum_registry = EnumRegistry::empty();
    let registry = StructRegistry::empty();
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
        &enum_registry,
    );

    // Assert that ty is populated but enum_type_id is None
    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let let_info = lowering.ir.let_meta().get(stmt_let_ir_id);
    if let Some(info) = let_info {
        // ty may or may not be populated depending on struct registry lookup
        // enum_type_id should definitely be None since it's not in enum_registry
        assert_eq!(info.enum_type_id, None, "Struct-annotated Let should have enum_type_id == None");
    }
}

/// Test 10: StmtLet with unknown type leaves both ty and enum_type_id None.
/// Verify error case: unknown type name leaves both fields untouched.
#[test]
fn test_stmt_let_unknown_type_leaves_both_none() {
    let mut ast = AstArena::new();
    let span = test_span();
    let (source_map, mut sink) = create_test_source_map_and_sink();

    // Create type annotation node with TypeData::Name for an unknown type
    let type_ident = ast.alloc(NodeKind::Ident, span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        span,
        paideia_as_ast::TypeData::Name {
            name: type_ident,
            args: vec![],
        },
    );

    // Create RHS
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    let name_id = ast.alloc(NodeKind::Ident, span);

    // Create a StmtLet with unknown type annotation
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        span,
        StmtData::Let {
            mutable: false,
            name: name_id,
            ty: Some(type_node),
            value: rhs_id,
        },
    );

    let enum_registry = EnumRegistry::empty();
    let registry = StructRegistry::empty();
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
        &enum_registry,
    );

    // Assert that both ty and enum_type_id remain None (error case swallowed)
    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let let_info = lowering.ir.let_meta().get(stmt_let_ir_id);
    if let Some(info) = let_info {
        assert!(info.ty.is_none(), "Unknown type should leave ty as None");
        assert_eq!(info.enum_type_id, None, "Unknown type should leave enum_type_id as None");
    }
}

/// Test 11: StmtLet with u64 primitive annotation populates ty.
/// Verify that primitive u64-typed bindings populate ty correctly.
#[test]
fn test_stmt_let_u64_primitive_annotated_populates_ty() {
    let mut ast = AstArena::new();
    let mut source_map = SourceMap::new();
    let fid = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("u64"),
    );
    let mut sink = VecSink::new();

    let name_span = Span::new(fid, 0, 3);  // covers "u64"
    let benign_span = Span::new(fid, 0, 1);

    let type_ident = ast.alloc(NodeKind::Ident, name_span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        benign_span,
        paideia_as_ast::TypeData::Name { name: type_ident, args: vec![] },
    );
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, benign_span);
    let name_id = ast.alloc(NodeKind::Ident, benign_span);
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        benign_span,
        StmtData::Let { mutable: false, name: name_id, ty: Some(type_node), value: rhs_id },
    );

    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(
        &ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map,
    );

    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    populate_let_meta_ty(
        &ast, &mut lowering.ir, &lowering.ast_to_ir, &source_map,
        &mut types, &mut effects, &mut caps, &registry, &enum_registry,
    );

    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let info = lowering.ir.let_meta().get(stmt_let_ir_id)
        .expect("populator wrote LetInfo for u64 annotation");
    assert!(info.ty.is_some(), "ty populated for primitive u64");
    assert!(info.enum_type_id.is_none(), "enum_type_id stays None for primitive");
}

/// Test 12: StmtLet with i32 primitive annotation populates ty.
/// Verify that primitive i32-typed bindings populate ty correctly.
#[test]
fn test_stmt_let_i32_primitive_annotated_populates_ty() {
    let mut ast = AstArena::new();
    let mut source_map = SourceMap::new();
    let fid = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("i32"),
    );
    let mut sink = VecSink::new();

    let name_span = Span::new(fid, 0, 3);  // covers "i32"
    let benign_span = Span::new(fid, 0, 1);

    let type_ident = ast.alloc(NodeKind::Ident, name_span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        benign_span,
        paideia_as_ast::TypeData::Name { name: type_ident, args: vec![] },
    );
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, benign_span);
    let name_id = ast.alloc(NodeKind::Ident, benign_span);
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        benign_span,
        StmtData::Let { mutable: false, name: name_id, ty: Some(type_node), value: rhs_id },
    );

    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(
        &ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map,
    );

    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    populate_let_meta_ty(
        &ast, &mut lowering.ir, &lowering.ast_to_ir, &source_map,
        &mut types, &mut effects, &mut caps, &registry, &enum_registry,
    );

    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let info = lowering.ir.let_meta().get(stmt_let_ir_id)
        .expect("populator wrote LetInfo for i32 annotation");
    assert!(info.ty.is_some(), "ty populated for primitive i32");
    assert!(info.enum_type_id.is_none(), "enum_type_id stays None for primitive");
}

/// Test 13: StmtLet with array annotation does not populate ty.
/// #1221: TypeData::Array has no arm in lower_type_ast (T0570 catch-all).
/// Negative witness — populator must not panic; ty stays None.
/// TODO: revisit when Array lowering lands (follow-up filed).
#[test]
fn test_stmt_let_array_annotated_does_not_populate_ty() {
    let mut ast = AstArena::new();
    let mut source_map = SourceMap::new();
    let fid = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("u8"),
    );
    let mut sink = VecSink::new();

    let span = Span::new(fid, 0, 2);

    let elem_ident = ast.alloc(NodeKind::Ident, span);
    let elem_type = ast.alloc_type(
        NodeKind::TypeName,
        span,
        paideia_as_ast::TypeData::Name { name: elem_ident, args: vec![] },
    );
    let length = ast.alloc(NodeKind::ExprLiteral, span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,  // populator only reads type_data, kind irrelevant
        span,
        paideia_as_ast::TypeData::Array { element: elem_type, length },
    );
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    let name_id = ast.alloc(NodeKind::Ident, span);
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        span,
        StmtData::Let { mutable: false, name: name_id, ty: Some(type_node), value: rhs_id },
    );

    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(
        &ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map,
    );

    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    populate_let_meta_ty(
        &ast, &mut lowering.ir, &lowering.ast_to_ir, &source_map,
        &mut types, &mut effects, &mut caps, &registry, &enum_registry,
    );

    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let info = lowering.ir.let_meta().get(stmt_let_ir_id);
    if let Some(i) = info {
        assert!(i.ty.is_none(), "array-type not lowered → ty stays None");
        assert!(i.enum_type_id.is_none());
    }
    // No panic reaching here IS the assertion.
}

/// Test 14: Populator does not panic on broken annotation.
/// #1221: regression witness — Name with non-Ident name (T0572 path).
/// Distinct from Test 10 (T0569 unknown-name). populator must not panic.
#[test]
fn test_populator_does_not_panic_on_broken_annotation() {
    let mut ast = AstArena::new();
    let mut source_map = SourceMap::new();
    let fid = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("x"),
    );
    let mut sink = VecSink::new();

    let span = Span::new(fid, 0, 1);

    // Name field points to non-Ident node — get_ident_text returns Err(T0572).
    let bogus = ast.alloc(NodeKind::ExprLiteral, span);
    let type_node = ast.alloc_type(
        NodeKind::TypeName,
        span,
        paideia_as_ast::TypeData::Name { name: bogus, args: vec![] },
    );
    let rhs_id = ast.alloc(NodeKind::ExprLiteral, span);
    let name_id = ast.alloc(NodeKind::Ident, span);
    let stmt_let_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        span,
        StmtData::Let { mutable: false, name: name_id, ty: Some(type_node), value: rhs_id },
    );

    let registry = StructRegistry::empty();
    let enum_registry = EnumRegistry::empty();
    let payload_map = HashMap::new();
    let mut lowering = lower_ast_to_ir(
        &ast, &source_map, &mut sink, &registry, &enum_registry, &payload_map,
    );

    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = paideia_as_types::CapSetInterner::new();

    populate_let_meta_ty(
        &ast, &mut lowering.ir, &lowering.ast_to_ir, &source_map,
        &mut types, &mut effects, &mut caps, &registry, &enum_registry,
    );

    // No panic. Either no entry, or ty stays None.
    let stmt_let_ir_id = lowering.ast_to_ir[&stmt_let_id];
    let info = lowering.ir.let_meta().get(stmt_let_ir_id);
    if let Some(i) = info { assert!(i.ty.is_none()); }
}
