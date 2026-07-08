//! PA-R17-012 (#990): Pure functions with control flow.
//!
//! Unit tests verifying that pure function bodies now support if/match/while/loop
//! without emitting T0532 diagnostic. Tests cover:
//! - Basic if-then-else and if-then
//! - Match expressions
//! - While loops
//! - Nested control flow
//! - Break and continue
//! - Control flow in various expression positions
//! - Regression: no T0532 emission

use paideia_as_ast::{AstArena, ExprData, LoopKind, NodeKind};
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_elaborator::{lower_ast_to_ir, struct_registry::StructRegistry, EnumRegistry};

use crate::common::create_test_source_map_and_sink;

fn span(start: u32) -> Span {
    Span::new(FileId::new(1).unwrap(), start, 1)
}

/// Test 1: Pure fn `if x { A } else { B }` lowers without T0532.
#[test]
fn test_pure_if_then_else_lowers_without_t0532() {
    let mut arena = AstArena::new();

    // Build a simple if-then-else structure
    let cond_id = arena.alloc(NodeKind::ExprPath, span(10));
    let then_id = arena.alloc(NodeKind::ExprLiteral, span(20));
    let else_id = arena.alloc(NodeKind::ExprLiteral, span(30));

    let if_data = ExprData::If {
        cond: cond_id,
        then_block: then_id,
        else_block: Some(else_id),
    };
    let if_id = arena.alloc_expr(NodeKind::ExprIf, span(5), if_data);

    // Lambda with if body
    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: if_id,
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
        "Pure fn with if-then-else should not emit T0532, but got {} T0532 diagnostics",
        t0532_diags.len()
    );

    // Verify IR was created
    assert!(!result.ast_to_ir.is_empty(), "IR mapping should not be empty");
}

/// Test 2: Pure fn `if x { A }` (no else) lowers without T0532.
#[test]
fn test_pure_if_then_only_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let cond_id = arena.alloc(NodeKind::ExprPath, span(10));
    let then_id = arena.alloc(NodeKind::ExprLiteral, span(20));

    let if_data = ExprData::If {
        cond: cond_id,
        then_block: then_id,
        else_block: None,
    };
    let if_id = arena.alloc_expr(NodeKind::ExprIf, span(5), if_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: if_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Pure fn with if-then only should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 3: Pure fn `match x { 1 => A, _ => B }` lowers without T0532.
#[test]
fn test_pure_match_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, span(10));
    let arm1_body = arena.alloc(NodeKind::ExprLiteral, span(30));
    let arm2_body = arena.alloc(NodeKind::ExprLiteral, span(40));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: arena.alloc(NodeKind::Placeholder, span(20)),
                guard: None,
                body: arm1_body,
            },
            paideia_as_ast::MatchArm {
                pattern: arena.alloc(NodeKind::Placeholder, span(35)),
                guard: None,
                body: arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, span(5), match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
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

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Pure fn with match should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 4: Pure fn `while x { A }` lowers without T0532.
#[test]
fn test_pure_while_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let cond_id = arena.alloc(NodeKind::ExprPath, span(10));
    let body_id = arena.alloc(NodeKind::ExprLiteral, span(20));

    let loop_data = ExprData::Loop {
        kind: LoopKind::While,
        header: Some(cond_id),
        body: body_id,
    };
    let while_id = arena.alloc_expr(NodeKind::ExprLoop, span(5), loop_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: while_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Pure fn with while should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 5: Pure fn `loop { A }` lowers without T0532.
#[test]
fn test_pure_loop_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let body_id = arena.alloc(NodeKind::ExprLiteral, span(20));

    let loop_data = ExprData::Loop {
        kind: LoopKind::Loop,
        header: None,
        body: body_id,
    };
    let loop_id = arena.alloc_expr(NodeKind::ExprLoop, span(5), loop_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: loop_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Pure fn with infinite loop should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 6: Pure fn `for pattern in iterable { A }` lowers without T0532.
#[test]
fn test_pure_for_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let pattern_id = arena.alloc(NodeKind::Placeholder, span(10));
    let iterable_id = arena.alloc(NodeKind::ExprPath, span(15));
    let body_id = arena.alloc(NodeKind::ExprLiteral, span(25));

    let for_data = ExprData::For {
        pattern: pattern_id,
        iterable: iterable_id,
        body: body_id,
    };
    let for_id = arena.alloc_expr(NodeKind::ExprFor, span(5), for_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: for_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Pure fn with for loop should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 7: Nested `if` in pure fn lowers without T0532.
#[test]
fn test_pure_nested_if_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let outer_cond = arena.alloc(NodeKind::ExprPath, span(10));
    let inner_cond = arena.alloc(NodeKind::ExprPath, span(20));
    let inner_then = arena.alloc(NodeKind::ExprLiteral, span(30));

    // Inner if
    let inner_if_data = ExprData::If {
        cond: inner_cond,
        then_block: inner_then,
        else_block: None,
    };
    let inner_if = arena.alloc_expr(NodeKind::ExprIf, span(15), inner_if_data);

    let else_expr = arena.alloc(NodeKind::ExprLiteral, span(40));

    // Outer if
    let outer_if_data = ExprData::If {
        cond: outer_cond,
        then_block: inner_if,
        else_block: Some(else_expr),
    };
    let outer_if = arena.alloc_expr(NodeKind::ExprIf, span(5), outer_if_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: outer_if,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Nested if in pure fn should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 8: `if` with guard in match arm.
#[test]
fn test_pure_match_with_guard_lowers_without_t0532() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, span(10));
    let guard_id = arena.alloc(NodeKind::ExprPath, span(25));
    let body_id = arena.alloc(NodeKind::ExprLiteral, span(35));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![paideia_as_ast::MatchArm {
            pattern: arena.alloc(NodeKind::Placeholder, span(20)),
            guard: Some(guard_id),
            body: body_id,
        }],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, span(5), match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
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

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Match with guard should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}

/// Test 9: IR nodes have correct children after lowering If.
#[test]
fn test_pure_if_ir_children_transferred() {
    let mut arena = AstArena::new();

    let cond_id = arena.alloc(NodeKind::ExprPath, span(10));
    let then_id = arena.alloc(NodeKind::ExprLiteral, span(20));
    let else_id = arena.alloc(NodeKind::ExprLiteral, span(30));

    let if_data = ExprData::If {
        cond: cond_id,
        then_block: then_id,
        else_block: Some(else_id),
    };
    let if_id = arena.alloc_expr(NodeKind::ExprIf, span(5), if_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: if_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    // Find IR node corresponding to if_id
    let ir_if_id = result.ast_to_ir.get(&if_id).copied();
    assert!(ir_if_id.is_some(), "If node should be in IR mapping");

    let ir_if_id = ir_if_id.unwrap();
    let ir_children = result.ir.children(ir_if_id);

    // If-then-else should have 3 children: condition, then, else
    assert_eq!(
        ir_children.len(),
        3,
        "If-then-else should have 3 children (cond, then, else), got {}",
        ir_children.len()
    );
}

/// Test 10: IR nodes have correct children after lowering Match.
#[test]
fn test_pure_match_ir_children_transferred() {
    let mut arena = AstArena::new();

    let scrutinee_id = arena.alloc(NodeKind::ExprPath, span(10));
    let arm1_body = arena.alloc(NodeKind::ExprLiteral, span(30));
    let arm2_body = arena.alloc(NodeKind::ExprLiteral, span(40));

    let match_data = ExprData::Match {
        scrutinee: scrutinee_id,
        arms: vec![
            paideia_as_ast::MatchArm {
                pattern: arena.alloc(NodeKind::Placeholder, span(20)),
                guard: None,
                body: arm1_body,
            },
            paideia_as_ast::MatchArm {
                pattern: arena.alloc(NodeKind::Placeholder, span(35)),
                guard: None,
                body: arm2_body,
            },
        ],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, span(5), match_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
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

    // Find IR node corresponding to match_id
    let ir_match_id = result.ast_to_ir.get(&match_id).copied();
    assert!(ir_match_id.is_some(), "Match node should be in IR mapping");

    let ir_match_id = ir_match_id.unwrap();
    let ir_children = result.ir.children(ir_match_id);

    // Match with 2 arms should have 3 children: scrutinee + body1 + body2.
    // Patterns are NOT IR children — since #1053 (nested pattern parser +
    // AST->IR lowering), patterns are classified into the MatchArmMeta
    // side-table (see `paideia_as_ir::MatchArmMeta` / `populate_match_arm_meta`
    // in `crates/paideia-as-elaborator/src/lower/match_arm.rs`), keyed by
    // the arm body's IR node id. `lower/children.rs`'s `ExprData::Match` arm
    // builder only pushes the scrutinee and each arm's body.
    assert_eq!(
        ir_children.len(),
        3,
        "Match with 2 arms should have 3 children, got {}",
        ir_children.len()
    );
}

/// Test 11: While loop IR has condition and body as children.
#[test]
fn test_pure_while_ir_children_transferred() {
    let mut arena = AstArena::new();

    let cond_id = arena.alloc(NodeKind::ExprPath, span(10));
    let body_id = arena.alloc(NodeKind::ExprLiteral, span(20));

    let loop_data = ExprData::Loop {
        kind: LoopKind::While,
        header: Some(cond_id),
        body: body_id,
    };
    let while_id = arena.alloc_expr(NodeKind::ExprLoop, span(5), loop_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: while_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    // Find IR node corresponding to while_id
    let ir_while_id = result.ast_to_ir.get(&while_id).copied();
    assert!(ir_while_id.is_some(), "While node should be in IR mapping");

    let ir_while_id = ir_while_id.unwrap();
    let ir_children = result.ir.children(ir_while_id);

    // While should have 2 children: condition + body
    assert_eq!(
        ir_children.len(),
        2,
        "While should have 2 children (cond, body), got {}",
        ir_children.len()
    );
}

/// Test 12: Regression - pure fn with multiple control structures.
#[test]
fn test_pure_complex_control_flow_no_t0532() {
    let mut arena = AstArena::new();

    // Build: if cond { match x { 1 => while y { a } } }
    let cond = arena.alloc(NodeKind::ExprPath, span(10));
    let scrutinee = arena.alloc(NodeKind::ExprPath, span(25));
    let loop_cond = arena.alloc(NodeKind::ExprPath, span(35));
    let loop_body = arena.alloc(NodeKind::ExprLiteral, span(40));

    let loop_data = ExprData::Loop {
        kind: LoopKind::While,
        header: Some(loop_cond),
        body: loop_body,
    };
    let while_id = arena.alloc_expr(NodeKind::ExprLoop, span(32), loop_data);

    let match_data = ExprData::Match {
        scrutinee,
        arms: vec![paideia_as_ast::MatchArm {
            pattern: arena.alloc(NodeKind::Placeholder, span(20)),
            guard: None,
            body: while_id,
        }],
        attrs: Default::default(),
    };
    let match_id = arena.alloc_expr(NodeKind::ExprMatch, span(15), match_data);

    let if_data = ExprData::If {
        cond,
        then_block: match_id,
        else_block: None,
    };
    let if_id = arena.alloc_expr(NodeKind::ExprIf, span(5), if_data);

    arena.alloc_expr(
        NodeKind::ExprLambda,
        span(1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: if_id,
            pipe_form: false,
        },
    );

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let registry = StructRegistry::empty();

    let result = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &EnumRegistry::empty(), &std::collections::HashMap::new());

    let t0532_diags: Vec<_> = sink
        .diagnostics()
        .iter()
        .filter(|d| d.code().number() == 532)
        .collect();
    assert!(
        t0532_diags.is_empty(),
        "Complex nested control flow should not emit T0532"
    );
    assert!(!result.ast_to_ir.is_empty(), "IR should be created");
}
