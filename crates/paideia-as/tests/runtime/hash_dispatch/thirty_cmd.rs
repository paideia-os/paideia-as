//! Issue #1003 (pa-r18-010) — hash dispatch 30-command scaffold test.
//!
//! Verifies that hash-based command dispatch scales beyond the density-contract
//! threshold for @jump_table fallback. 30 command handlers are registered in a
//! [u64; 64] hash table using 6-bit masking. We dispatch on cmd14 and expect
//! it to return 14.
//!
//! See `tests/build-emit/hash_dispatch/pa_r18_010_hash_dispatch_30cmd.pdx`.

use super::super::harness::{run_and_verify, RetTy, RuntimeCase};

#[test]
fn pa_r18_010_hash_dispatch_30cmd() {
    let case = RuntimeCase {
        fixture_pdx: "hash_dispatch/pa_r18_010_hash_dispatch_30cmd.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &[],
        expected: 14,
    };
    run_and_verify(&case);
}
