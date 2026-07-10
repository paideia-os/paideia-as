//! Test for #1135: visit_var_assign diagnostic routing for unsupported shapes.
//!
//! Tests that visit_var_assign correctly routes diagnostics to the typed pipe
//! (T0540) instead of the legacy string vec for non-Var RHS, LHS shadowing, and
//! structural invariant violations.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that literal RHS (counter = 5) emits T0540 diagnostic.
///
/// Issue #1135 case 1: RHS is not a Var node (it's a literal).
#[test]
fn module_let_mut_assign_literal_rhs_emits_t0540() {
    let input = build_emit("module_let_mut_assign_literal_rhs.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert!(
        out.stderr.to_lowercase().contains("rhs") || out.stderr.to_lowercase().contains("var"),
        "T0540 message should mention RHS or Var: {}",
        out.stderr
    );
}

/// Test that field access RHS (counter = p.x) emits T0540 diagnostic.
///
/// Issue #1135 case 3: RHS is not a Var node (it's a FieldAccess).
#[test]
fn module_let_mut_assign_field_rhs_emits_t0540() {
    let input = build_emit("module_let_mut_assign_field_rhs.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert!(
        out.stderr.to_lowercase().contains("rhs") || out.stderr.to_lowercase().contains("var"),
        "T0540 message should mention RHS or Var: {}",
        out.stderr
    );
}

/// Test that shadowed LHS (parameter shadows module let-mut) emits T0540 diagnostic.
///
/// Issue #1135 case 2: LHS is shadowed by a local binding (parameter with same name).
#[test]
fn module_let_mut_assign_shadowed_lhs_emits_t0540() {
    let input = build_emit("module_let_mut_assign_shadowed_lhs.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert!(
        out.stderr.to_lowercase().contains("shadowed") || out.stderr.to_lowercase().contains("module"),
        "T0540 message should mention shadowing or module symbol: {}",
        out.stderr
    );
}
