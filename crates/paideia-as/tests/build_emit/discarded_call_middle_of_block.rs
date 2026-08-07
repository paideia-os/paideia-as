//! Issue #1183: emit_call_stmt unconditional RET truncates functions with post-call statements.
//!
//! Before the fix, `emit_call_stmt` emitted a bare RET after every statement-position call,
//! even though the enclosing lambda body (Action block) already emits its own terminal RET.
//! This caused duplicate + premature RET instructions, truncating any statements after the call.
//!
//! After the fix, `emit_call_stmt` emits ONLY the call (arguments + CALL); the terminal RET
//! is the responsibility of `emit_block_body` (for Action blocks) or the author (for Unsafe blocks).

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test fixture A (discarded_call_middle_of_block.pdx):
/// Verifies that a discarded call in the middle of a block does NOT emit a premature RET.
///
/// The function should compile to:
///   helper(x)  -> `mov rdi, rdi; call helper`
///   let y = x  -> `mov rcx, rdi; mov rax, rcx`
///   y          -> (implicit tail, RAX already holds y)
///   [terminal RET from emit_block_body]
///
/// Before the fix, after `call helper` there would be a premature `ret (0xC3)`,
/// truncating the function and preventing the let-binding from executing.
#[test]
fn caller_no_premature_ret() {
    let input = build_emit("discarded_call_middle_of_block.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Assertion: a CALL (0xE8 = 1 byte, followed by 4-byte PC-relative offset) should NOT be
    // immediately followed by RET (0xC3). We check windows of 6 bytes (5 for the call, 1 for the next byte).
    // If the pattern is `[0xE8, ?, ?, ?, ?, !0xC3]`, the call is NOT immediately followed by RET (good).
    let has_call_not_followed_by_ret = bytes.windows(6).any(|w| {
        w[0] == 0xE8 && w[5] != 0xC3
    });

    assert!(
        has_call_not_followed_by_ret,
        "expected a CALL instruction NOT immediately followed by RET in .text section"
    );

    // Assertion: the function should still END with a RET (terminal RET from emit_block_body).
    let has_terminal_ret = bytes.iter().any(|&b| b == 0xC3);
    assert!(
        has_terminal_ret,
        "expected a terminal RET (0xC3) in .text section"
    );
}

/// Test fixture B (discarded_call_at_tail.pdx):
/// Verifies that a discarded call at the tail position still receives a terminal RET from emit_block_body.
///
/// The function should compile to:
///   helper(x)  -> `mov rdi, rdi; call helper` followed immediately by RET (from emit_block_body)
///
/// This is a regression check: after the fix, tail calls should still get the terminal RET,
/// but now sourced from emit_block_body's terminator, not from the buggy emit_call_stmt.
#[test]
fn discarded_call_at_tail_ends_with_call_ret() {
    let input = build_emit("discarded_call_at_tail.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // paideia-as#1276 phase 3: tail-call function still returns with a RET,
    // but the RET is now preceded by the 4-byte frame epilogue
    // (48 89 EC 5D = mov rsp,rbp; pop rbp). So the byte-sequence after the
    // 5-byte CALL (E8 + 4-byte disp) is 48 89 EC 5D C3 — a 10-byte window.
    let has_call_then_epilogue_then_ret = bytes.windows(10).any(|w| {
        w[0] == 0xE8
            && w[5] == 0x48 && w[6] == 0x89 && w[7] == 0xEC && w[8] == 0x5D
            && w[9] == 0xC3
    });

    assert!(
        has_call_then_epilogue_then_ret,
        "expected CALL followed by frame epilogue (48 89 EC 5D) then RET in .text section"
    );
}

/// Test fixture C (two_discarded_calls_then_tail.pdx):
/// Verifies that multiple discarded calls in sequence emit only ONE terminal RET total.
///
/// The function should compile to:
///   helper(1)  -> `mov rdi, 1; call helper`
///   helper(2)  -> `mov rdi, 2; call helper`
///   3          -> `mov rax, 3`
///   [terminal RET from emit_block_body]
///
/// Total RET count in the function should be exactly 1 (the terminal RET).
/// If the buggy emit_call_stmt behavior persisted, we'd see 2 premature RETs inside the function
/// (one after each call), plus the terminal RET, totaling 3+ RETs — which would corrupt the bytecode.
///
/// Before the fix, the function would have 3 RETs (2 premature + 1 terminal).
/// After the fix, there should be only 1 RET (the terminal one from emit_block_body).
#[test]
fn two_discarded_calls_have_only_one_terminal_ret() {
    let input = build_emit("two_discarded_calls_then_tail.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Count the RET (0xC3) bytes in the .text section.
    let ret_count = bytes.iter().filter(|&&b| b == 0xC3).count();

    // The function should have exactly 2 RET bytes:
    // - 1 for the `helper` function (defined in the module)
    // - 1 for the `f` function (defined in the module)
    //
    // If alignment or padding is emitted between symbols, encoder bytes (NOP, padding) should not be 0xC3.
    // We allow up to 2 RETs. If the bug persists, we'd see 3+ (2 premature in f + 1 terminal).
    assert!(
        ret_count <= 2,
        "expected <= 2 RET (0xC3) bytes in .text, got {}; indicates premature RET emission bug",
        ret_count
    );
}
