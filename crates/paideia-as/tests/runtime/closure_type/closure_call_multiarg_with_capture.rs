//! Issue #995 — multi-arg multi-capture closure invocation.
//!
//! See `tests/build-emit/closure_type/closure_call_multiarg_with_capture.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn closure_call_multiarg_with_capture_returns_42() {
    let case = RuntimeCase {
        fixture_pdx: "closure_type/closure_call_multiarg_with_capture.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 42,
    };
    run_and_verify(&case);
}
