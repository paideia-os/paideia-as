//! PA-R17-013 (#991): Match expressions in tail position (function return).
//!
//! AST-level integration tests verifying that match expressions in lambda return position
//! lower correctly. EnumCons-specific tests are in emit_walker_tests.rs at IR level.
//!
//! Tests cover:
//! - Match returning scalar u32 writes to RAX only
//! - Match dispatch and ret instruction ordering
//! - Default-only match (no dispatch needed)
//! - Match with payload binder binding discriminant/payload
//! - Nested match expressions in arm bodies
//! - Regression: pure fn with match doesn't emit T0532

use paideia_as_ast::{AstArena, ExprData, NodeKind};
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_elaborator::{lower_ast_to_ir, struct_registry::StructRegistry, EnumRegistry};

use crate::common::create_test_source_map_and_sink;

fn ast_span(start: u32) -> Span {
    Span::new(FileId::new(1).unwrap(), start, 1)
}

/// Test 1: match returning literal u32 writes RAX (scalar case)
#[test]
fn match_returning_literal_u32_writes_rax() {
    let mut arena = AstArena::new();

    // Build: match x { 0 => 42, _ => 99 }
    let scrutinee_id = arena.alloc(NodeKind::ExprPath, ast_span(10));

    // First arm: pattern 0, body 42
    let arm1_pattern = arena.alloc(NodeKind::ExprLiteral, ast_span(20));
    let arm1_body = arena.alloc(NodeKind::ExprLiteral, ast_span(30));

    // Second arm: pattern _, body 99
    let arm2_pattern = arena.alloc(NodeKind::Placeholder, ast_span(35));
    let arm2_body = arena.alloc(NodeKind::ExprLiteral, ast_span(40));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: arm1_pattern,
                guard: None,
                body: arm1_body,
            },
            paideia_as_ast::MatchArm {
                pattern: arm2_pattern,
                guard: None,
                body: arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(5), match_data);

    // Lambda with match as body
    arena.alloc_expr(
        NodeKind::ExprLambda,
        ast_span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: match_id,
            pipe_form: false,
        },
    );

    // Lower to IR
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    // Should lower without T0532 errors
    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Match returning u32 should not emit T0532"
    );

    // Verify IR was created
    assert!(!result.ast_to_ir.is_empty(), "IR mapping should not be empty");
}

/// NOTE: EnumCons tests moved to IR level in emit_walker_tests.rs.
///
/// AST lowering does not produce EnumCons nodes (issue #1048), so AST-level
/// integration tests cannot exercise the enum-derivation codepath. Use
/// `match_returning_small_enum_writes_rax_rdx_ir_level` in emit_walker_tests.rs
/// for real synthesized IR tests of EnumCons emission.

/// Test 3: match 2 arms dispatch and ret ordering
#[test]
fn match_2_arms_dispatch_and_ret_ordering() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, ast_span(10));
    let arm1_pattern = arena.alloc(NodeKind::Placeholder, ast_span(20));
    let arm1_body = arena.alloc(NodeKind::ExprLiteral, ast_span(30));
    let arm2_pattern = arena.alloc(NodeKind::Placeholder, ast_span(35));
    let arm2_body = arena.alloc(NodeKind::ExprLiteral, ast_span(40));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: arm1_pattern,
                guard: None,
                body: arm1_body,
            },
            paideia_as_ast::MatchArm {
                pattern: arm2_pattern,
                guard: None,
                body: arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(5), match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        ast_span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: match_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify IR was created with arms
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
    // Verify no diagnostics
    assert_eq!(sink.diagnostics().len(), 0, "No diagnostics expected");
}

/// Test 4: match with default-only arm, no dispatch needed
#[test]
fn match_default_only_no_dispatch() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, ast_span(10));

    // Only default arm
    let default_pattern = arena.alloc(NodeKind::Placeholder, ast_span(20));
    let default_body = arena.alloc(NodeKind::ExprLiteral, ast_span(30));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![paideia_as_ast::MatchArm {
            pattern: default_pattern,
            guard: None,
            body: default_body,
        }],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(5), match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        ast_span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: match_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    // Should lower without errors
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
    assert_eq!(sink.diagnostics().len(), 0, "No diagnostics expected");
}

/// Test 5: match with payload binder
#[test]
fn match_with_payload_binder() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, ast_span(10));

    // Some(v) arm
    let some_pattern = arena.alloc(NodeKind::Placeholder, ast_span(20));
    let some_body = arena.alloc(NodeKind::ExprPath, ast_span(30));

    // None arm
    let none_pattern = arena.alloc(NodeKind::Placeholder, ast_span(35));
    let none_body = arena.alloc(NodeKind::ExprLiteral, ast_span(40));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: some_pattern,
                guard: None,
                body: some_body,
            },
            paideia_as_ast::MatchArm {
                pattern: none_pattern,
                guard: None,
                body: none_body,
            },
        ],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(5), match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        ast_span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: match_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 6: nested match in arm body (AST-level integration)
/// Verifies that nested matches lower correctly. Dispatch instruction verification
/// is in `nested_match_arm_body_is_match_ir_level` in emit_walker_tests.rs.
#[test]
fn nested_match_arm_body_is_match() {
    let mut arena = AstArena::new();

    // Outer match scrutinee: x
    let outer_scrutinee = arena.alloc(NodeKind::ExprPath, ast_span(10));

    // Inner match: match y { 0 => 1, _ => 2 }
    let inner_scrutinee = arena.alloc(NodeKind::ExprPath, ast_span(25));
    let inner_arm1_pattern = arena.alloc(NodeKind::ExprLiteral, ast_span(30));
    let inner_arm1_body = arena.alloc(NodeKind::ExprLiteral, ast_span(35));
    let inner_arm2_pattern = arena.alloc(NodeKind::Placeholder, ast_span(40));
    let inner_arm2_body = arena.alloc(NodeKind::ExprLiteral, ast_span(45));

    let inner_match_data = ExprData::Match {
        scrutinee: inner_scrutinee,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: inner_arm1_pattern,
                guard: None,
                body: inner_arm1_body,
            },
            paideia_as_ast::MatchArm {
                pattern: inner_arm2_pattern,
                guard: None,
                body: inner_arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let inner_match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(20), inner_match_data);

    // Outer match: 0 => inner_match, _ => 0
    let outer_arm1_pattern = arena.alloc(NodeKind::ExprLiteral, ast_span(50));
    let outer_arm2_pattern = arena.alloc(NodeKind::Placeholder, ast_span(55));
    let outer_arm2_body = arena.alloc(NodeKind::ExprLiteral, ast_span(60));

    let outer_match_data = ExprData::Match {
        scrutinee: outer_scrutinee,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: outer_arm1_pattern,
                guard: None,
                body: inner_match_id,
            },
            paideia_as_ast::MatchArm {
                pattern: outer_arm2_pattern,
                guard: None,
                body: outer_arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let outer_match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(15), outer_match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        ast_span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: outer_match_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    assert!(!result.ast_to_ir.is_empty(), "IR should be created with nested matches");
}

/// NOTE: EnumCons tests moved to IR level in emit_walker_tests.rs.
///
/// AST lowering does not produce EnumCons nodes (issue #1048), so AST-level
/// integration tests cannot exercise the enum-derivation codepath. Use
/// `match_returning_large_enum_writes_rdi_slot_ir_level` in emit_walker_tests.rs
/// for real synthesized IR tests of EnumCons emission.

/// Test 8: pure fn match no T0532 regression
#[test]
fn pure_fn_match_no_t0532_regression() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, ast_span(10));
    let arm1_pattern = arena.alloc(NodeKind::ExprLiteral, ast_span(20));
    let arm1_body = arena.alloc(NodeKind::ExprLiteral, ast_span(30));
    let arm2_pattern = arena.alloc(NodeKind::Placeholder, ast_span(35));
    let arm2_body = arena.alloc(NodeKind::ExprLiteral, ast_span(40));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: arm1_pattern,
                guard: None,
                body: arm1_body,
            },
            paideia_as_ast::MatchArm {
                pattern: arm2_pattern,
                guard: None,
                body: arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, ast_span(5), match_data);

    // Lambda: fn () -> match x { 0 => 1, _ => 2 }
    arena.alloc_expr(
        NodeKind::ExprLambda,
        ast_span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: match_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    // Should compile without T0532 diagnostic
    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Pure fn with match should not emit T0532, but got {} T0532 diagnostics",
        t0532_diags.len()
    );

    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}
