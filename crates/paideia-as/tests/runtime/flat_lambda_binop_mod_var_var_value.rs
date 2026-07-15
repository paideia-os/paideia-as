//! Issue #1200 — runtime verification for flat-lambda BinOp with modulo.
//!
//! Tests that flat-lambda functions with modulo operator correctly emit code
//! and return the expected value.
//!
//! Calculation: compute(17, 5) -> 17 % 5 = 2

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn flat_lambda_binop_mod_var_var_returns_2() {
    let case = RuntimeCase {
        fixture_pdx: "flat_lambda_binop_mod_var_var.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 2,
    };
    run_and_verify(&case);
}
