//! Issue #1200 — runtime verification for flat-lambda BinOp with division.
//!
//! Tests that flat-lambda functions with division operator correctly emit code
//! and return the expected value.
//!
//! Calculation: compute(42, 6) -> 42 / 6 = 7

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn flat_lambda_binop_div_var_var_returns_7() {
    let case = RuntimeCase {
        fixture_pdx: "flat_lambda_binop_div_var_var.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 7,
    };
    run_and_verify(&case);
}
