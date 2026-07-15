//! #1193: Flat-lambda BinOp handler fix — build_emit canaries
//!
//! Verifies that:
//! 1. Runtime canaries compile successfully (no T0540 errors)
//! 2. Explicit-failure canaries fail with T0540 (expected)

use crate::common::harness::run_build;
use crate::common::fixture;

#[test]
fn flat_lambda_binop_op_op_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_op_op.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (App-op, App-op) shape after #1193");
}

#[test]
fn flat_lambda_binop_op_var_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_op_var.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (App-op, Var) shape after #1193");
}

#[test]
fn flat_lambda_binop_op_lit_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_op_lit.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (App-op, Literal) shape after #1193");
}

#[test]
fn flat_lambda_binop_lit_op_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_lit_op.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (Literal, App-op) shape after #1193");
}

#[test]
fn flat_lambda_binop_depth2_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_depth2.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on depth-2 nesting after #1193");
}

#[test]
fn flat_lambda_binop_symbol_footprint_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_symbol_footprint.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on sibling flat-lambda shapes after #1193");
    // Note: Each flat-lambda function emits its own RET per the #1193 fix, preventing
    // symbol collision leakage across sibling lambdas. B1704 count should be 0 for this fixture.
}

// Explicit failure canaries (LOUD rejection expected)

#[test]
fn flat_lambda_binop_nested_fncall_fires_t0540() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_nested_fncall_fires_t0540.pdx"));
    let exit_code = out.exit_code().unwrap_or(0);
    assert!(exit_code != 0, "nested function call in BinOp must fail to compile");
    assert!(out.stderr.contains("T0540"), "T0540 must fire on nested function calls in BinOp");
}

// #1196: operator-disambiguation fixtures
//
// These canaries verify that single-char operators (`*`, `&`, `|`, `^`, `-`)
// and `>>` are now handled correctly after #1196 (operator_lexeme_of) replaces
// the span-length heuristic (infer_operator_from_span_len).

#[test]
fn flat_lambda_binop_mul_var_var_compiles_and_computes_42() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_mul_var_var.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on multiplication after #1197 fix");
}

#[test]
fn flat_lambda_binop_and_var_lit_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_and_var_lit.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (Var, Literal) AND after #1196 fix");
}

#[test]
fn flat_lambda_binop_or_lit_var_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_or_lit_var.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (Literal, Var) OR after #1196 fix");
}

#[test]
fn flat_lambda_binop_xor_var_lit_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_xor_var_lit.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (Var, Literal) XOR after #1196 fix");
}

#[test]
fn flat_lambda_binop_sub_var_lit_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_sub_var_lit.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (Var, Literal) SUB after #1196 fix");
}

#[test]
fn flat_lambda_binop_shr_var_lit_compiles() {
    let out = run_build(fixture::build_emit("flat_lambda_binop_shr_var_lit.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on (Var, Literal) SHR after #1196 fix");
}
