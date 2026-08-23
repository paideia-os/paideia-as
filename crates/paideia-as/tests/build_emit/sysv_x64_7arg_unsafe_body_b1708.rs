//! v0.22.0 (#1326 phase 6): B1708 for an unsafe-bodied >6-arg lambda.
//!
//! Companion to sysv_x64_9arg_no_frame_b1708.rs (phase 3), which pinned
//! the `@no_frame` attribute path at 9 args. This fixture pins the other
//! half of the `body_is_unsafe || is_no_frame` condition
//! (`crates/paideia-as-elaborator/src/emit_visit_lambda.rs:417`) at the
//! minimum boundary arity (7 args, idx=6 the first stack-passed param) —
//! an unsafe-bodied lambda never emits the frame-pointer prologue either,
//! so it cannot accept stack-passed params any more than a `@no_frame`
//! one can.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_7arg_unsafe_body_emits_b1708() {
    let out = run_build(build_emit("sysv_x64_7arg_unsafe_body_b1708.pdx"));
    out.assert_diag("B1708");
}

#[test]
fn sysv_x64_7arg_unsafe_body_emits_no_artifact() {
    // B1708 is Severity::Error, so the build must refuse to emit any
    // bytes for this module (mirrors sysv_x64_9arg_no_frame_b1708.rs).
    let out = run_build(build_emit("sysv_x64_7arg_unsafe_body_b1708.pdx"));
    assert!(
        !out.status.success(),
        "expected build to fail for unsafe-bodied + 7-arg SysV lambda, but it succeeded"
    );
}
