//! Issue #1186: emit_block_body_arm silently dropped bare-App children (tail-position calls)
//! in match-arm bodies. Sibling to #1183.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn tail_call_in_each_arm_emits_call() {
    let input = build_emit("match_arm_tail_call.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let call_count = bytes.iter().filter(|&&b| b == 0xE8).count();
    assert!(
        call_count >= 2,
        "expected >= 2 CALL (0xE8) bytes for 2 arm-body calls, got {}; silent-drop regression",
        call_count
    );
}

#[test]
fn discarded_and_tail_call_both_emit() {
    let input = build_emit("match_arm_mixed_calls.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let call_count = bytes.iter().filter(|&&b| b == 0xE8).count();
    assert!(
        call_count >= 4,
        "expected >= 4 CALL (0xE8) bytes for 2 arms x 2 calls, got {}",
        call_count
    );
}
