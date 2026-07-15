//! Issue #1205: Unresolvable Var at position 0 of call must fire T0521.
//!
//! Before this fix, `add(undefined_var, 7u64)` where arg_idx==0 and
//! dest_reg==RDI would silently become a no-op (neither mov emitted nor
//! T0521 diagnostic fired). After the fix, the fallback only preserves
//! the mov-from-RDI case when dest_reg != RDI; all other cases (including
//! arg_idx==0 && dest_reg==RDI) emit T0521.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn call_with_unresolvable_var_pos0_fires_t0521() {
    let out = run_build(build_emit("call_with_unresolvable_var_pos0.pdx"));
    assert!(
        out.stderr.contains("T0521"),
        "T0521 must fire for unresolvable Var at position 0 of call; stderr:\n{}",
        out.stderr
    );
}
