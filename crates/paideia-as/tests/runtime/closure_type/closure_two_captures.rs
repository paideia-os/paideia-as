//! Issue #994 — two-capture closure construction.
//!
//! See `tests/build-emit/closure_type/closure_two_captures.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn closure_two_captures_returns_100() {
    let case = RuntimeCase {
        fixture_pdx: "closure_type/closure_two_captures.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 100,
    };
    run_and_verify(&case);
}
