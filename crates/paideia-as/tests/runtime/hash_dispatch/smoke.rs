//! Issue #1003 (pa-r18-010) — hash dispatch smoke test.
//!
//! Verifies that four command handlers can be registered in a hashmap and
//! dispatched via indirect call. Dispatches on `echo` and expects exit 3.
//!
//! See `tests/build-emit/hash_dispatch/pa_r18_010_hash_dispatch_smoke.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn pa_r18_010_hash_dispatch_smoke() {
    let case = RuntimeCase {
        fixture_pdx: "hash_dispatch/pa_r18_010_hash_dispatch_smoke.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 3,
    };
    run_and_verify(&case);
}
