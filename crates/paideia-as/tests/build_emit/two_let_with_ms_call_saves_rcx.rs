//! Issue #1163 (corrective): scratch binding save/restore ordering around MS-ABI call.
//! Issue #1192 (fix): dynamic MS shadow-space bump for odd scratch-spill count.
//!
//! Fixture: two consecutive calls, first returns into RCX via SysV ABI,
//! second is MS-ABI and will clobber RCX. After fix, RCX is pushed after the MS prelude
//! (before arg marshalling for the MS call) and popped before the MS postlude.
//!
//! With #1192 fix: N=1 (odd scratch), so MS bump is 48 (0x30) not 40 (0x28).
//!
//! The sequence should be:
//! [call helper_a]
//! [mov rax -> rcx, holding the result]
//! [bridge saves: push R14, R15]
//! [MS prelude: sub rsp, 0x30]  ← 48 bytes (40 + 8 for odd N)
//! [push rcx]  ← must push before arg marshalling
//! [arg marshalling MOVs]
//! [call helper_ms]
//! [pop rcx]   ← must pop before MS postlude
//! [MS postlude: add rsp, 0x30]  ← 48 bytes (40 + 8 for odd N)

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn two_let_with_ms_call_saves_rcx_assembles() {
    let out = run_build(build_emit("two_let_with_ms_call_saves_rcx.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // The .text should contain CALL (0xe8) instructions and PUSH/POP RCX (0x51/0x59)
    let call_count = text.iter().filter(|&&b| b == 0xe8).count();
    assert!(
        call_count >= 2,
        ".text must contain at least 2 CALL (0xe8) instructions, found {}",
        call_count
    );

    // Find the CALL instructions
    let mut call_offsets = Vec::new();
    for (i, &b) in text.iter().enumerate() {
        if b == 0xe8 {
            call_offsets.push(i);
        }
    }

    // We expect at least helper_a (first call) and helper_ms (second call)
    // Look for a push rcx that comes after the first call and before the second call
    let mut push_between_calls = None;
    let mut pop_after_second_call = None;

    for (i, &b) in text.iter().enumerate() {
        if b == 0x51 && push_between_calls.is_none() {
            // Check if this push comes after first call and before second call
            if !call_offsets.is_empty() && i > call_offsets[0] && (call_offsets.len() < 2 || i < call_offsets[1]) {
                push_between_calls = Some(i);
            }
        }
    }

    let _push_off = push_between_calls.expect(
        "Must find push rcx (0x51) between first and second CALL instructions"
    );

    // Now find pop rcx after the second call
    if call_offsets.len() >= 2 {
        for (i, &b) in text.iter().enumerate() {
            if b == 0x59 && i > call_offsets[1] {
                pop_after_second_call = Some(i);
                break;
            }
        }
    }

    let pop_off = pop_after_second_call.expect(
        "Must find pop rcx (0x59) after second CALL instruction"
    );

    // Find MS postlude pattern (add rsp, 0x28 = 0x48 0x83 0xc4 0x28)
    let ms_postlude_pattern = [0x48u8, 0x83, 0xc4, 0x28];
    let mut ms_postlude_offset = None;
    for i in 0..text.len().saturating_sub(3) {
        if text[i..i + 4] == ms_postlude_pattern {
            ms_postlude_offset = Some(i);
            break;
        }
    }

    // Pop must come BEFORE MS postlude (this verifies Defect B is fixed)
    if let Some(ms_postlude_off) = ms_postlude_offset {
        assert!(
            pop_off < ms_postlude_off,
            "pop rcx (offset {}) must come BEFORE MS postlude add rsp (offset {}). \
             This verifies the fix for Defect B: pops must happen before RSP is adjusted",
            pop_off,
            ms_postlude_off
        );
    }
}

#[test]
fn two_let_with_ms_call_saves_rcx_prelude_is_48_for_odd_scratch() {
    // #1192 regression fence: verify MS prelude is sub rsp, 48 (0x30) not 40 (0x28)
    // when N=1 live scratch binding (RCX) needs saving.
    // The prelude byte sequence must be 0x48 0x83 0xec 0x30 (not 0x28).
    let out = run_build(build_emit("two_let_with_ms_call_saves_rcx.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Look for MS prelude pattern: sub rsp, 48 = 0x48 0x83 0xec 0x30
    let ms_prelude_48_pattern = [0x48u8, 0x83, 0xec, 0x30];
    let prelude_48_found = text.windows(4).any(|w| w == ms_prelude_48_pattern);
    assert!(
        prelude_48_found,
        "MS prelude must be 'sub rsp, 48' (0x48 0x83 0xec 0x30) for N=1 odd scratch"
    );
}
