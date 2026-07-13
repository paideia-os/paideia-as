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

    // Expected disassembly: `mov rcx, 5; mov rax, rcx; ret`
    // Possible encodings (encoder may choose different forms):
    // 1. mov r64, imm64: 48 C7 C1 05 00 00 00 (mov rcx, 5 as sign-extended imm32)
    //    mov r64, r64: 48 89 C8 (mov rax, rcx)
    //    ret: C3
    //    Total: 11 bytes [0x48, 0xC7, 0xC1, 0x05, 0x00, 0x00, 0x00, 0x48, 0x89, 0xC8, 0xC3]
    //
    // 2. mov r64, imm32: B9 05 00 00 00 (mov ecx, 5 — zero-extends to rcx)
    //    mov r64, r64: 48 89 C8 (mov rax, rcx)
    //    ret: C3
    //    Total: 9 bytes [0xB9, 0x05, 0x00, 0x00, 0x00, 0x48, 0x89, 0xC8, 0xC3]

    let expected_11_bytes = [0x48u8, 0xC7, 0xC1, 0x05, 0x00, 0x00, 0x00, 0x48, 0x89, 0xC8, 0xC3];
    let expected_9_bytes = [0xB9u8, 0x05, 0x00, 0x00, 0x00, 0x48, 0x89, 0xC8, 0xC3];

    let found_11 = bytes
        .windows(expected_11_bytes.len())
        .any(|w| w == expected_11_bytes);
    let found_9 = bytes
        .windows(expected_9_bytes.len())
        .any(|w| w == expected_9_bytes);

    assert!(
        found_11 || found_9,
        "expected to find either 11-byte or 9-byte pattern (mov 5 into rcx; mov rax, rcx; ret) in .text section"
    );
}
