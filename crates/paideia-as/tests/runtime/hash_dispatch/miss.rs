//! Issue #1003 (pa-r18-010) — hash dispatch miss test.
//!
//! Verifies that a lookup miss (key absent from table) correctly returns None,
//! triggering the dispatch_miss path and exiting with code 999.
//!
//! See `tests/build-emit/hash_dispatch/pa_r18_010_hash_dispatch_miss.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn pa_r18_010_hash_dispatch_miss() {
    let case = RuntimeCase {
        fixture_pdx: "hash_dispatch/pa_r18_010_hash_dispatch_miss.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 200,
    };
    run_and_verify(&case);
}
