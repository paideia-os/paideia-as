//! Fuel + stack-depth accounting tests for the term evaluator.
//!
//! Split out from the tests module inside the original single-file
//! `term_eval.rs` in the phase-2 God-file refactor (issue #1412 under
//! umbrella #1405). Preserves the exact scenarios, assertions, and
//! diagnostic-code expectations (M0311 fuel + depth) of the prior tests
//! unchanged.

use paideia_as_ast::{AstArena, ExprData, NodeKind, StmtData};
use paideia_as_diagnostics::{FileId, Span};

use super::{eval, Env, Value, DEFAULT_FUEL, DEFAULT_STACK_DEPTH};

fn test_span(byte_start: u32, byte_len: u32) -> Span {
    Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
}

#[test]
fn eval_consumes_fuel_per_step() {
    let mut arena = AstArena::new();

    // Build 1 + 1
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0)); // + operator

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit2_placeholder,
        },
    );

    let infix_id = arena.alloc_expr(
        NodeKind::ExprInfix,
        test_span(0, 3),
        ExprData::Infix {
            lhs: lit1_id,
            op: op_id,
            rhs: lit2_id,
        },
    );

    let initial_fuel = 1000u64;
    let mut env = Env::with_limits(initial_fuel, DEFAULT_STACK_DEPTH);
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    // The evaluation should have consumed some fuel.
    // We expect to visit: infix node, lhs literal, rhs literal = 3 nodes.
    // Each costs 1 fuel, so remaining should be initial - 3 or so.
    // (The actual count depends on how the evaluator walks the AST.)
    assert!(env.fuel < initial_fuel);
}

#[test]
fn eval_exhausts_fuel_emits_m0311() {
    let mut arena = AstArena::new();

    // Build 1 + 1
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0)); // + operator

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit2_placeholder,
        },
    );

    let infix_id = arena.alloc_expr(
        NodeKind::ExprInfix,
        test_span(0, 3),
        ExprData::Infix {
            lhs: lit1_id,
            op: op_id,
            rhs: lit2_id,
        },
    );

    // Set fuel to 1 (only enough for one eval step, not three)
    let mut env = Env::with_limits(1, DEFAULT_STACK_DEPTH);
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    // Should fail with fuel exhausted (M0311)
    assert!(result.is_err());
    let diag = result.unwrap_err();
    assert_eq!(diag.code().number(), 311);
    assert!(diag.message().contains("fuel"));
}

#[test]
fn eval_depth_limit_emits_m0311() {
    let mut arena = AstArena::new();

    // Build a 3-deep nested let: let x = 1 in let y = 1 in let z = 1 in z
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let x_name = arena.alloc(NodeKind::Ident, test_span(100, 0));
    let let_stmt_x = arena.alloc_stmt(
        NodeKind::StmtLet,
        test_span(0, 10),
        StmtData::Let {
            mutable: false,
            name: x_name,
            ty: None,
            value: lit1_id,
            atomic: None,
        },
    );

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit2_placeholder,
        },
    );

    let y_name = arena.alloc(NodeKind::Ident, test_span(101, 0));
    let let_stmt_y = arena.alloc_stmt(
        NodeKind::StmtLet,
        test_span(10, 10),
        StmtData::Let {
            mutable: false,
            name: y_name,
            ty: None,
            value: lit2_id,
            atomic: None,
        },
    );

    let lit3_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit3_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit3_placeholder,
        },
    );

    let z_name = arena.alloc(NodeKind::Ident, test_span(102, 0));
    let let_stmt_z = arena.alloc_stmt(
        NodeKind::StmtLet,
        test_span(20, 10),
        StmtData::Let {
            mutable: false,
            name: z_name,
            ty: None,
            value: lit3_id,
            atomic: None,
        },
    );

    let z_segment = arena.alloc(NodeKind::Ident, test_span(102, 0));
    let z_path = arena.alloc_expr(
        NodeKind::ExprPath,
        test_span(102, 0),
        ExprData::Path {
            segments: vec![z_segment],
        },
    );

    // Build nested blocks with max_depth = 2
    // The chain will be: block(let_stmt_z, tail=z_path) inside block(let_stmt_y, tail=...) inside block(let_stmt_x, tail=...)
    let inner_block = arena.alloc_expr(
        NodeKind::ExprBlock,
        test_span(20, 15),
        ExprData::Block {
            stmts: vec![let_stmt_z],
            tail: Some(z_path),
        },
    );

    let middle_block = arena.alloc_expr(
        NodeKind::ExprBlock,
        test_span(10, 20),
        ExprData::Block {
            stmts: vec![let_stmt_y],
            tail: Some(inner_block),
        },
    );

    let outer_block = arena.alloc_expr(
        NodeKind::ExprBlock,
        test_span(0, 30),
        ExprData::Block {
            stmts: vec![let_stmt_x],
            tail: Some(middle_block),
        },
    );

    // Set max_depth to 2 so the third eval call (z_path) will hit the limit
    let mut env = Env::with_limits(DEFAULT_FUEL, 2);
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, outer_block, &mut env, &mut type_cache);

    // Should fail with depth exceeded (M0311)
    assert!(result.is_err());
    let diag = result.unwrap_err();
    assert_eq!(diag.code().number(), 311);
    assert!(diag.message().contains("depth"));
}

#[test]
fn eval_with_limits_passes_within_budget() {
    let mut arena = AstArena::new();

    // Build 1 + 2
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0)); // + operator

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(2, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(2, 0),
        ExprData::Literal {
            lit: lit2_placeholder,
        },
    );

    let infix_id = arena.alloc_expr(
        NodeKind::ExprInfix,
        test_span(0, 3),
        ExprData::Infix {
            lhs: lit1_id,
            op: op_id,
            rhs: lit2_id,
        },
    );

    // Generous budget: 100 fuel, 256 depth
    let mut env = Env::with_limits(100, 256);
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::Int(3));
}

#[test]
fn eval_diagnostic_distinguishes_fuel_vs_depth() {
    let mut arena = AstArena::new();

    // Build a simple literal
    let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    // Test 1: fuel exhausted
    let mut env_fuel = Env::with_limits(0, DEFAULT_STACK_DEPTH);
    let mut type_cache_fuel = crate::reflect_api::TypeCache::new();
    let result_fuel = eval(&arena, lit_id, &mut env_fuel, &mut type_cache_fuel);

    assert!(result_fuel.is_err());
    let diag_fuel = result_fuel.unwrap_err();
    assert_eq!(diag_fuel.code().number(), 311);
    assert!(diag_fuel.message().contains("fuel"));

    // Test 2: depth exceeded
    let mut env_depth = Env::with_limits(DEFAULT_FUEL, 0);
    let mut type_cache_depth = crate::reflect_api::TypeCache::new();
    let result_depth = eval(&arena, lit_id, &mut env_depth, &mut type_cache_depth);

    assert!(result_depth.is_err());
    let diag_depth = result_depth.unwrap_err();
    assert_eq!(diag_depth.code().number(), 311);
    assert!(diag_depth.message().contains("depth"));
}
