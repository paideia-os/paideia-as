//! Issue #994 — single-capture closure construction.
//!
//! See `tests/build-emit/closure_type/closure_value_capture.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn closure_value_capture_returns_50() {
    let case = RuntimeCase {
        fixture_pdx: "closure_type/closure_value_capture.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 50,
    };
    run_and_verify(&case);
}
