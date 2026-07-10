//! T0521 fix (partial #1086): non-first-arg Var call marshalling.
//!
//! Fixture: `sum(a, b) = add(a, b)` — the caller `sum` forwards both parameters
//! into the callee `add`. Before this fix, `add(a, b)` would fire T0521 on the
//! `b` argument (arg_idx=1) because non-first-arg Var references were rejected.

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sum_forwards_both_args_to_add_no_t0521() {
    let out = run_build(build_emit("multi_arg_var_call.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr.contains("T0521"),
        "T0521 must not fire for non-first-arg Var after the local-binding lookup fix; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);
    assert!(
        text.iter().any(|&b| b == 0xe8),
        ".text must contain a CALL (0xe8) for add(a, b)"
    );
}
