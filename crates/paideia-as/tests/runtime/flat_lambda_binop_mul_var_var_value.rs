//! Issue #1196 + #1197 — runtime verification for flat-lambda BinOp with multiplication.
//!
//! Tests that flat-lambda functions with multiplication operator correctly emit code
//! and return the expected value.
//!
//! Calculation: compute(7, 6) -> 7 * 6 = 42

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn flat_lambda_binop_mul_var_var_returns_42() {
    let case = RuntimeCase {
        fixture_pdx: "flat_lambda_binop_mul_var_var.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 42,
    };
    run_and_verify(&case);
}
