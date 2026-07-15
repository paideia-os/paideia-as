//! Issue #1200 — runtime verification for division with prior let binding (RDX save/restore test).
//!
//! Tests that division correctly saves and restores RDX when it's live from a prior binding.
//! This exercises the RDX liveness detection and save/restore code paths.
//!
//! Calculation: compute(36, 45, 6) -> (36 & 45) / 6 = 36 / 6 = 6

use super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn flat_lambda_binop_div_var_var_rdx_live_returns_6() {
    let case = RuntimeCase {
        fixture_pdx: "flat_lambda_binop_div_var_var_rdx_live.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 6,
    };
    run_and_verify(&case);
}
