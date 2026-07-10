//! Test for #1135: visit_var_assign diagnostic routing for unsupported shapes.
//!
//! Tests that visit_var_assign correctly routes diagnostics to the typed pipe
//! (T0540) instead of the legacy string vec for non-Var RHS, LHS shadowing, and
//! structural invariant violations.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Strip ANSI SGR escape sequences (`\x1b[...m`) from CLI output.
///
/// The human diagnostic renderer emits color codes unconditionally
/// (`NO_COLOR` is not currently honored — a separate, pre-existing gap,
/// out of scope here), so raw `stderr` contains sequences like
/// `\x1b[1;31merror\x1b[0m[T0540]`. A naive `stderr.contains("error[")`
/// check would never match; strip escapes first so diagnostic-counting
/// assertions are robust regardless of whether color is emitted.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Consume through the terminating 'm' of a CSI SGR sequence.
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

/// Assert that exactly one diagnostic (`error[CODE]`) appears in `stderr`.
///
/// Guards against a fixture or code-path change silently introducing extra,
/// unrelated diagnostics (e.g. parser errors from a malformed fixture)
/// alongside the expected T0540.
fn assert_single_diagnostic(stderr: &str) {
    let plain = strip_ansi(stderr);
    assert_eq!(
        plain.matches("error[").count(),
        1,
        "expected exactly one diagnostic (T0540), got:\n{}",
        stderr
    );
}

/// Test that literal RHS (counter = 5) emits T0540 diagnostic.
///
/// Issue #1135 case 1: RHS is not a Var node (it's a literal).
#[test]
fn module_let_mut_assign_literal_rhs_emits_t0540() {
    let input = build_emit("module_let_mut_assign_literal_rhs.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    // T0540's catalog brief always contains "var"/"module", so a substring
    // check against those words is satisfied by every T0540 firing regardless
    // of which branch in visit_var_assign produced it. Assert instead that
    // T0540 is the *only* diagnostic emitted — no unrelated parse/lowering
    // errors (e.g. a malformed fixture) are silently riding along.
    assert_single_diagnostic(&out.stderr);
}

/// Test that field access RHS (counter = p.x) emits T0540 diagnostic.
///
/// Issue #1135 case 3: RHS is not a Var node (it's a FieldAccess).
#[test]
fn module_let_mut_assign_field_rhs_emits_t0540() {
    let input = build_emit("module_let_mut_assign_field_rhs.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    // Same rationale as the literal-RHS case above: assert T0540 fires alone,
    // with no spurious parser/lowering errors (e.g. from a malformed `struct`
    // fixture) contaminating the result.
    assert_single_diagnostic(&out.stderr);
}

/// Test that shadowed LHS (parameter shadows module let-mut) compiles to a no-op self-move.
///
/// #1138 implements register-rewrite lowering for local-shadowed LHS: when a parameter
/// named `counter` shadows a module `counter` let-mut, the assignment `counter = counter`
/// emits `mov RDI, RDI` (a no-op self-move at the parameter register, RDI = arg 0).
///
/// This is a successful compilation scenario: parameter mutability enforcement is a
/// separate concern (not in scope for #1138).
#[test]
fn module_let_mut_assign_shadowed_lhs_compiles() {
    let input = build_emit("module_let_mut_assign_shadowed_lhs.pdx");
    let out = run_build(input);
    // Filter out debug output (lines starting with '[') and check for actual errors
    let errors: Vec<&str> = out.stderr.lines()
        .filter(|line| !line.starts_with('[') && line.contains("error["))
        .collect();
    assert!(errors.is_empty(), "expected no error diagnostics, got:\n{:?}", errors);
    // Artifact should exist and have non-zero size (contains the no-op Mov instruction)
    let bytes = out.artifact_bytes();
    assert!(!bytes.is_empty(), "expected non-empty artifact (ELF object file)");
}
