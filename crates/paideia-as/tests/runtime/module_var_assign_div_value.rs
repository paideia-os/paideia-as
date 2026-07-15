//! Issue #1200 — runtime verification for module var-assign BinOp with division.
//!
//! Tests that module-level mutable variable assignments with division operator
//! correctly emit code and return the expected value.
//!
//! Calculation: counter = 42u64 / 6u64; counter -> 42 / 6 = 7

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn module_var_assign_div_returns_7() {
    let case = RuntimeCase {
        fixture_pdx: "module_var_assign_div.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 7,
    };
    run_and_verify(&case);
}
