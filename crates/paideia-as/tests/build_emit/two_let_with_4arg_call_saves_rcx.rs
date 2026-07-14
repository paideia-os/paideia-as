//! Issue #1163 (corrective): scratch binding save/restore for 4-arg SysV call clobbering RCX.
//!
//! Fixture: two calls, first returns value that lives in RAX (will be moved to RCX
//! in a binding), second call has 4 args and will use RCX as arg3 (the 4th argument
//! in SysV ABI). The first call's result must be saved in RCX before marshalling
//! arg3 into RCX for the second call, then restored after.

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn two_let_with_4arg_call_saves_rcx_assembles() {
    let out = run_build(build_emit("two_let_with_4arg_call_saves_rcx.pdx"));
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

    // We expect at least helper_a (first call) and take4 (second call)
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

    let _pop_off = pop_after_second_call.expect(
        "Must find pop rcx (0x59) after second CALL instruction"
    );
}
