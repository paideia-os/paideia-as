//! Issue #1197 corrective — runtime verification for module-level let-mut assignment with multiplication.
//!
//! Tests that module-level let-mut assignments with multiplication operator on the RHS
//! correctly emit code and return the expected value.
//!
//! Calculation: entry() -> counter = 7 * 6; counter = 42

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn module_var_assign_mul_returns_42() {
    let case = RuntimeCase {
        fixture_pdx: "module_var_assign_mul.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 42,
    };
    run_and_verify(&case);
}
