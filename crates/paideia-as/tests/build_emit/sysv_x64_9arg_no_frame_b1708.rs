//! v0.22.0 (#1326 phase 3): B1708 — `@no_frame` incompatible with >6-param
//! SysV lambda.
//!
//! Same 9-arg shape as sysv_x64_9arg_callee_stack_read.rs, but the callee
//! carries `@no_frame`. Per design/compiler/lambda-arity-stack-spill.md
//! §4.4/§11, this combination must be refused with B1708 rather than
//! silently reading garbage off an uninitialised RBP, since a `@no_frame`
//! lambda never emits the frame-pointer prologue that idx>=6 stack-passed
//! params are read back through.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_9arg_no_frame_emits_b1708() {
    let out = run_build(build_emit("sysv_x64_9arg_no_frame_b1708.pdx"));
    out.assert_diag("B1708");
}

#[test]
fn sysv_x64_9arg_no_frame_emits_no_artifact() {
    // v0.22.0 (#1326 phase 3): B1708 is Severity::Error, so the build must
    // refuse to emit any bytes for this module (mirrors the "preview"
    // short-circuit in cmd_build.rs when any error diagnostic exists).
    let out = run_build(build_emit("sysv_x64_9arg_no_frame_b1708.pdx"));
    assert!(
        !out.status.success(),
        "expected build to fail for @no_frame + 9-arg SysV lambda, but it succeeded"
    );
}
