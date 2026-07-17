//! Issue #995 — closure survives intervening direct call.
//!
//! See `tests/build-emit/closure_type/closure_call_after_intervening_direct_call.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn closure_call_after_intervening_direct_call_returns_42() {
    let case = RuntimeCase {
        fixture_pdx: "closure_type/closure_call_after_intervening_direct_call.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 42,
    };
    run_and_verify(&case);
}
