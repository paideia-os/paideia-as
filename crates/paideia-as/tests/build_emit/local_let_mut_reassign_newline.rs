//! Issue #1180: newline-separated local let mut + reassign silently miscompiles.
//!
//! Fixture: `let mut x = 0 \n x = K \n x` — the assignment (Store) becomes orphaned
//! in the AST arena when the parser overwrites `tail` on each iteration without
//! checking whether more content follows. This cascades to a phantom module symbol
//! emission via emit_walker's parent-less Store sweep, causing a silent miscompile.
//!
//! Before #1180, this fixture compiles and emits stray bytes before the function body.
//! After #1180, the parser wraps the assignment in StmtExpr, and the elaborator
//! correctly rejects it via T0540 (var assignment not supported at local scope).

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Strip ANSI SGR escape sequences (`\x1b[...m`) from CLI output.
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

/// Assert that exactly one diagnostic (`error[CODE]`) appears in `stderr`.
fn assert_single_diagnostic(stderr: &str) {
    let plain = strip_ansi(stderr);
    assert_eq!(
        plain.matches("error[").count(),
        1,
        "expected exactly one diagnostic (T0540), got:\n{}",
        stderr
    );
}

/// Test that newline-separated local let mut + reassign fires T0540.
///
/// Issue #1180 primary bug shape: `let mut x = 0 \n x = K \n x`.
/// Before the fix, the assignment was orphaned and miscompiled silently.
/// After the fix, the parser wraps the assignment in StmtExpr, and the
/// elaborator fires T0540 (var assignment not supported at local scope).
#[test]
fn newline_reassign_fires_t0540() {
    let input = build_emit("local_let_mut_reassign_newline.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert_single_diagnostic(&out.stderr);
}

/// Test that tail-drop variant (reassign without returning the variable) fires T0540.
///
/// Issue #1180 variant: `let mut y = 0 \n y = K \n 42`.
/// Same fix applies: the assignment becomes a StmtExpr, triggering T0540.
#[test]
fn newline_missing_tail_fires_t0540() {
    let input = build_emit("local_let_mut_reassign_newline_missing_tail.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert_single_diagnostic(&out.stderr);
}

/// Test that semicolon-terminated variant (regression barrier) fires T0540.
///
/// Control: `let mut x = 0; x = K; x` with semicolons throughout.
/// This should also fire T0540 (matching pre-fix behavior), ensuring the fix
/// doesn't weaken the semicolon path.
#[test]
fn semicolon_reassign_fires_t0540() {
    let input = build_emit("local_let_mut_reassign_semicolon.pdx");
    let out = run_build(input);
    out.assert_diag("T0540");
    assert_single_diagnostic(&out.stderr);
}

/// Test that positive control (newline-separated let without reassign) compiles.
///
/// Control: `let x = 5 \n x` — no assignment, just a let binding and tail expression.
/// This should compile successfully and emit `mov rcx, 5; mov rax, rcx; ret`.
#[test]
fn positive_newline_let_compiles() {
    let input = build_emit("local_let_newline_positive.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Expected body: `mov rcx, 5; mov rax, rcx; ret`
    // paideia-as#1276 phase 3: preceded by prologue (55 48 89 E5) and
    // succeeded by epilogue (48 89 EC 5D) before the terminal ret. We
    // widen the search to the mid-body reg-move (48 89 C8) which is
    // stable across both encoder-imm-form variants; the presence of the
    // 4-byte epilogue before RET is a stronger phase-3 witness anyway.
    let has_mov_rax_rcx = bytes.windows(3).any(|w| w == [0x48u8, 0x89, 0xC8]);
    let has_frame_epilogue_then_ret = bytes
        .windows(5)
        .any(|w| w == [0x48u8, 0x89, 0xEC, 0x5D, 0xC3]);

    assert!(
        has_mov_rax_rcx && has_frame_epilogue_then_ret,
        "expected to find `mov rax, rcx` (48 89 C8) and frame epilogue+ret (48 89 EC 5D C3) in .text"
    );
}
