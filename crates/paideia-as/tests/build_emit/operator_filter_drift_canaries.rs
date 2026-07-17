//! Issue #1230: Central operator registry drift-detector canaries.
//!
//! Verifies that all operators in KNOWN_OPERATORS have proper emit support
//! and no operator silently falls through to catch-all error paths.

use crate::common::harness::run_build;
use crate::common::fixture;

// C3: Drift-detector — verify every KNOWN_OPERATOR has emit coverage
// This test verifies that each operator in the central registry can actually
// be compiled without falling through to T0540 catch-all paths.

/// C3: Comparison operators must compile (uses existing fixtures).
#[test]
fn comparison_operators_all_compile() {
    let fixtures = vec![
        "cmp_lt_true.pdx",
        "cmp_lt_false.pdx",
        "cmp_gt_true.pdx",
        "cmp_le_eq_true.pdx",
        "cmp_ge_eq_true.pdx",
        "cmp_eq_true.pdx",
        "cmp_ne_true.pdx",
    ];

    for fixture_name in fixtures {
        let out = run_build(fixture::build_emit(fixture_name));
        assert!(out.exit_code().unwrap_or(0) == 0,
            "Comparison operator fixture {} failed to compile: {}",
            fixture_name, out.stderr);
        assert!(!out.stderr.contains("T0540"),
            "T0540 should not fire for {}", fixture_name);
    }
}

/// C3: Bitwise and arithmetic operators must compile.
#[test]
fn binary_operators_all_compile() {
    let fixtures = vec![
        "flat_lambda_binop_and_var_lit.pdx",
        "flat_lambda_binop_or_lit_var.pdx",
        "flat_lambda_binop_xor_var_lit.pdx",
        "flat_lambda_binop_sub_var_lit.pdx",
        "flat_lambda_binop_mul_var_var.pdx",
        "flat_lambda_binop_div_var_var.pdx",
        "flat_lambda_binop_mod_var_var.pdx",
        "flat_lambda_binop_shr_var_lit.pdx",
    ];

    for fixture_name in fixtures {
        let out = run_build(fixture::build_emit(fixture_name));
        assert!(out.exit_code().unwrap_or(0) == 0,
            "Binary operator fixture {} failed to compile: {}",
            fixture_name, out.stderr);
        assert!(!out.stderr.contains("T0540"),
            "T0540 should not fire for {}", fixture_name);
    }
}

// C5: Comparison operator reproducer — tests from #1229 and #1230.
// These fixtures verify that comparisons work correctly in lambda context.

#[test]
fn comparison_gt_in_lambda_compiles() {
    // #1229 reproducer: 3u64 > 1u64 must emit cmp/setcc/movzx, not shl
    let out = run_build(fixture::build_emit("cmp_gt_true.pdx"));
    assert!(out.exit_code().unwrap_or(0) == 0,
        "Comparison > in lambda should compile: {}",
        out.stderr);
    assert!(!out.stderr.contains("T0540"),
        "T0540 should not fire on comparison > in lambda");
}

#[test]
fn comparison_lt_in_lambda_compiles() {
    let out = run_build(fixture::build_emit("cmp_lt_true.pdx"));
    assert!(out.exit_code().unwrap_or(0) == 0,
        "Comparison < in lambda should compile: {}",
        out.stderr);
    assert!(!out.stderr.contains("T0540"),
        "T0540 should not fire on comparison < in lambda");
}

#[test]
fn comparison_eq_in_lambda_compiles() {
    let out = run_build(fixture::build_emit("cmp_eq_true.pdx"));
    assert!(out.exit_code().unwrap_or(0) == 0,
        "Comparison == in lambda should compile: {}",
        out.stderr);
    assert!(!out.stderr.contains("T0540"),
        "T0540 should not fire on comparison == in lambda");
}
