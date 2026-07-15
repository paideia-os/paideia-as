//! Issue #1200 — runtime verification for let-RHS BinOp with division.
//!
//! Tests that let-RHS expressions with division operator correctly emit code
//! and return the expected value.
//!
//! Calculation: let r = 12u64 / 4u64; r -> 12 / 4 = 3

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn let_rhs_binop_div_returns_3() {
    let case = RuntimeCase {
        fixture_pdx: "let_rhs_binop_div.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 3,
    };
    run_and_verify(&case);
}
