//! Test for #1094 negative case: local let mut assignment (documented gap).
//!
//! Tests: `fn () -> { let mut x : u64 = 0; x = 1; x }`
//!
//! This should emit T0540 because x is a local (stack) binding, not module-level.
//! visit_var_assign explicitly rejects local-shadowed LHS per #1135.

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

/// Test that local let mut assignment emits T0540 diagnostic.
///
/// Local let mut bindings are not yet supported for assignment; this is a documented gap.
#[test]
fn local_let_mut_assignment_emits_t0540() {
    let input = build_emit("stmt_assign_local_let_mut.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert_single_diagnostic(&out.stderr);
}
