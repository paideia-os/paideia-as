//! Issue #1003 (pa-r18-010) — hash dispatch collision test.
//!
//! Verifies that linear probing works correctly when two command names hash to
//! the same 5-bit slot. The cd and mkdir commands both hash to slot 16; mkdir
//! probes and lands at slot 17. On lookup, mkdir is found at slot 17, and its
//! handler returns 20.
//!
//! See `tests/build-emit/hash_dispatch/pa_r18_010_hash_dispatch_collision.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn pa_r18_010_hash_dispatch_collision() {
    let case = RuntimeCase {
        fixture_pdx: "hash_dispatch/pa_r18_010_hash_dispatch_collision.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 20,
    };
    run_and_verify(&case);
}
