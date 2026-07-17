//! Issue #994 — zero-capture closure construction.
//!
//! See `tests/build-emit/closure_type/closure_no_capture.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn closure_no_capture_returns_42() {
    let case = RuntimeCase {
        fixture_pdx: "closure_type/closure_no_capture.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 42,
    };
    run_and_verify(&case);
}
