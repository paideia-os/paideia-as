//! Test for #1094 negative case: assignment with call RHS (documented gap).
//!
//! Tests: `counter = compute(v)` where compute is a function call.
//!
//! This should emit T0540 because App-RHS is a documented gap.
//! visit_var_assign requires RHS to be a Var node per #1135.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Strip ANSI SGR escape sequences for robust diagnostic matching.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c2 in chars.by_ref() {
                if c2 == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Assert that exactly one diagnostic appears in stderr.
fn assert_single_diagnostic(stderr: &str) {
    let plain = strip_ansi(stderr);
    assert_eq!(
        plain.matches("error[").count(),
        1,
        "expected exactly one diagnostic (T0540), got:\n{}",
        stderr
    );
}

/// Test that assignment with call-RHS emits T0540 diagnostic.
///
/// Call expressions on the RHS are not yet supported; this is a documented gap.
/// See follow-up issue on routing call-RHS assignment through scratch materialization.
#[test]
fn call_rhs_assignment_emits_t0540() {
    let input = build_emit("stmt_assign_call_rhs.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert_single_diagnostic(&out.stderr);
}
