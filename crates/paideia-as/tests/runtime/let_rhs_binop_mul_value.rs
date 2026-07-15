//! Issue #1197 corrective — runtime verification for let-RHS BinOp with multiplication.
//!
//! Tests that let-bindings with multiplication operator on the RHS correctly emit code
//! and return the expected value.
//!
//! Calculation: entry() -> let r = 7 * 6; r = 42

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn let_rhs_binop_mul_returns_42() {
    let case = RuntimeCase {
        fixture_pdx: "let_rhs_binop_mul.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 42,
    };
    run_and_verify(&case);
}
