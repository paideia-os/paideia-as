//! Issue #1200 — runtime verification for module var-assign BinOp with modulo.
//!
//! Tests that module-level mutable variable assignments with modulo operator
//! correctly emit code and return the expected value.
//!
//! Calculation: counter = 17u64 % 5u64; counter -> 17 % 5 = 2

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn module_var_assign_mod_returns_2() {
    let case = RuntimeCase {
        fixture_pdx: "module_var_assign_mod.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 2,
    };
    run_and_verify(&case);
}
