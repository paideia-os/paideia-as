//! Issue #1100: Close two byte-order coverage gaps from #1099 debugger pass.
//!
//! **Gap 1 (safe call with literal args):**
//! When a function calls another with literal arguments, verifies:
//! - First byte is MOV (0x48 prefix for REX.W)
//! - CALL opcode (0xE8) appears after at least 7 bytes of MOV
//! - Last byte is RET (0xC3)
//!
//! **Gap 2 (indirect call via RIP-relative symbol):**
//! Verifies that indirect calls (FF 15 form) emit with proper byte-order:
//! - First byte is MOV (0x48 prefix for REX.W)
//! - MOV byte pattern exists before the FF 15 indirect CALL opcode
//! - Last byte is RET (0xC3)
//!
//! **Current limitations:**
//! - Gap 2 via_reg variant (indirect call via register, FF D0+ forms) cannot be tested
//!   because the current language doesn't support calling function parameters (U1614).
//! - Gap 2 via_mem_base_disp variant (indirect call via record field, FF 5X forms) cannot
//!   be tested due to U1614 limitations with unsafe blocks.
//!
//! These tests catch regressions in the CALL/RET ID scheme that ensures arg MOVs
//! sort before CALL and RET is last (issue #1099).

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Gap 1: caller must start with MOV, have CALL at offset ≥7, and end with RET
#[test]
fn caller_bytes_start_with_mov_when_unsafe_wraps_safe_call_with_args() {
    let out = run_build(build_emit("pa_r19_1100_gap1_unsafe_call_args.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    let caller = elf::symbol_bytes(&bytes, "caller").expect("caller in .text");
    assert!(!caller.is_empty(), "caller symbol must have non-zero size");

    // Gap 1 witness: first byte must be MOV (REX.W prefix)
    assert_eq!(
        caller.first().copied(),
        Some(0x48),
        "caller must start with REX.W MOV, not CALL/RET"
    );

    // Gap 1 witness: CALL opcode (0xE8) must exist after at least 7 bytes of MOV
    let call_pos = caller
        .iter()
        .position(|&b| b == 0xE8)
        .expect("caller must contain 0xE8 direct CALL");
    assert!(
        call_pos >= 7,
        "CALL opcode at offset {call_pos} must come after at least one MOV (7 or 10 bytes)"
    );

    // Gap 1 witness: RET must be last byte (no dead code after RET)
    assert_eq!(
        caller.last().copied(),
        Some(0xC3),
        "caller must end with RET; no dead-code past RET"
    );
}

/// Gap 2: entry must start with MOV, have MOV pattern before FF 15, and end with RET
#[test]
fn entry_bytes_place_mov_before_ff15_on_fnptr_indirect_call() {
    let out = run_build(build_emit("pa_r19_1100_gap2_fnptr_indirect_args.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    let entry = elf::symbol_bytes(&bytes, "entry").expect("entry in .text");
    assert!(!entry.is_empty(), "entry symbol must have non-zero size");

    // Gap 2 witness: first byte must be MOV (REX.W prefix)
    assert_eq!(
        entry.first().copied(),
        Some(0x48),
        "entry must start with REX.W MOV prefix"
    );

    // Gap 2 witness: FF 15 (indirect CALL via RIP-relative) must exist
    let ff15_pos = entry
        .windows(2)
        .position(|w| w == b"\xFF\x15")
        .expect("FF 15 indirect CALL missing");
    assert!(
        ff15_pos > 0,
        "FF 15 cannot be first byte — MOV must precede"
    );

    // Gap 2 tight witness: verify MOV byte pattern in entry[0..ff15_pos]
    let before_call = &entry[0..ff15_pos];
    let has_mov_c7_form = before_call.windows(3).any(|w| {
        // C7 /0 forms: 48 C7 C7 (mov r64, imm32) and similar
        w[0] == 0x48 && w[1] == 0xC7 && (w[2] & 0xF8 == 0xC0 || w[2] & 0xF8 == 0xC8)
    });
    let has_mov_bf_form = before_call
        .windows(2)
        .any(|w| w[0] == 0x48 && w[1] == 0xBF); // movabs rax, imm64

    assert!(
        has_mov_c7_form || has_mov_bf_form,
        "entry[0..{ff15_pos}] must contain a REX.W MOV imm form (C7 or BF), not just REX.W prefix"
    );

    // Gap 2 witness: RET must be last byte
    assert_eq!(entry.last().copied(), Some(0xC3), "entry must end with RET");
}

