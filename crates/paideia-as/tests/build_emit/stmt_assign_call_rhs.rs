//! #1136: assignment with an App RHS now routes via scratch-materialization.
//!
//! Pre-#1136 this fixture was a documented gap that fired T0540; now the
//! same shape lowers to args + CALL + `mov [rip+counter], rax`.

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn call_rhs_assignment_lowers_via_rax() {
    let out = run_build(build_emit("stmt_assign_call_rhs.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr.contains("T0540"),
        "T0540 must not fire on App RHS after #1136; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);
    assert!(
        text.iter().any(|&b| b == 0xe8),
        ".text must contain a CALL (0xe8) for compute(v)"
    );
    assert!(
        text.windows(3).any(|w| w == [0x48, 0x89, 0x05]),
        ".text must contain `mov [rip+counter], rax` (48 89 05) for the App-RHS store"
    );
}
