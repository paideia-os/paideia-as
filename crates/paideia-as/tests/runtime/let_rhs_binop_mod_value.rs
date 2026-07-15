//! Issue #1200 — runtime verification for let-RHS BinOp with modulo.
//!
//! Tests that let-RHS expressions with modulo operator correctly emit code
//! and return the expected value.
//!
//! Calculation: let r = 13u64 % 5u64; r -> 13 % 5 = 3

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn let_rhs_binop_mod_returns_3() {
    let case = RuntimeCase {
        fixture_pdx: "let_rhs_binop_mod.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 3,
    };
    run_and_verify(&case);
}
