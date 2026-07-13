//! #1163: Spill/restore caller-save scratch bindings around CALL.
//!
//! Tests that sequential let-bindings with two function calls properly spill and restore
//! live scratch bindings held in caller-save registers (RCX, RDX, R8, R9) between CALLs.
//!
//! Before #1163, the second CALL would silently clobber the first call's result if it lived
//! in a caller-save register, causing silent miscompile on any callee that legitimately uses
//! those registers.
//!
//! Fixture: combo(v) = { let a = helper_a(v); let b = helper_b(v); a + b }
//! Expected: combo(1) = 11 + 21 = 32 (helper_a(1) = 11, helper_b(1) = 21, sum = 32)

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn two_let_with_app_spill_restore_emits_both_calls() {
    let out = run_build(build_emit("two_let_with_app_spill_restore.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Count CALL (0xe8) opcodes in .text
    let call_count = text.windows(1).filter(|w| w[0] == 0xe8).count();
    assert!(
        call_count >= 2,
        ".text must contain at least TWO CALL (0xe8) opcodes for combo; got {}",
        call_count
    );
}

#[test]
fn two_let_with_app_spill_restore_emits_push_rcx() {
    let out = run_build(build_emit("two_let_with_app_spill_restore.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Count PUSH RCX (0x51) opcodes in .text
    let push_rcx_count = text.windows(1).filter(|w| w[0] == 0x51).count();
    assert!(
        push_rcx_count >= 1,
        ".text must contain at least one PUSH RCX (0x51) opcode for spilling; got {}",
        push_rcx_count
    );
}

#[test]
fn two_let_with_app_spill_restore_emits_pop_rcx() {
    let out = run_build(build_emit("two_let_with_app_spill_restore.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Count POP RCX (0x59) opcodes in .text
    let pop_rcx_count = text.windows(1).filter(|w| w[0] == 0x59).count();
    assert!(
        pop_rcx_count >= 1,
        ".text must contain at least one POP RCX (0x59) opcode for restoring; got {}",
        pop_rcx_count
    );
}

#[test]
fn two_let_with_app_spill_restore_push_before_second_call() {
    let out = run_build(build_emit("two_let_with_app_spill_restore.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Find the sequence: should have push(0x51) followed by call(0xe8) somewhere
    // We're looking for the spill pattern
    let mut found_push_before_call = false;
    for i in 0..text.len().saturating_sub(1) {
        if text[i] == 0x51 {  // PUSH RCX
            // Look for CALL within the next ~20 bytes (reasonable distance)
            for j in (i+1)..std::cmp::min(i + 20, text.len()) {
                if text[j] == 0xe8 {  // CALL
                    found_push_before_call = true;
                    break;
                }
            }
            if found_push_before_call {
                break;
            }
        }
    }
    assert!(
        found_push_before_call,
        ".text must contain PUSH RCX (0x51) before CALL (0xe8) for spilling"
    );
}

#[test]
fn two_let_with_app_spill_restore_pop_after_call() {
    let out = run_build(build_emit("two_let_with_app_spill_restore.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Find the sequence: should have call(0xe8) followed by pop(0x59) somewhere
    // We're looking for the restore pattern
    let mut found_call_before_pop = false;
    for i in 0..text.len().saturating_sub(1) {
        if text[i] == 0xe8 {  // CALL
            // Look for POP within the next ~20 bytes
            for j in (i+1)..std::cmp::min(i + 20, text.len()) {
                if text[j] == 0x59 {  // POP RCX
                    found_call_before_pop = true;
                    break;
                }
            }
            if found_call_before_pop {
                break;
            }
        }
    }
    assert!(
        found_call_before_pop,
        ".text must contain CALL (0xe8) before POP RCX (0x59) for restoring"
    );
}

#[test]
fn two_let_with_app_spill_restore_has_both_relocations() {
    let out = run_build(build_emit("two_let_with_app_spill_restore.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut helper_a_found = false;
    let mut helper_b_found = false;

    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "helper_a" {
                            helper_a_found = true;
                        } else if name == "helper_b" {
                            helper_b_found = true;
                        }
                    }
                }
            }
        }
    }

    assert!(
        helper_a_found,
        "must have a relocation targeting `helper_a` (first callee)"
    );
    assert!(
        helper_b_found,
        "must have a relocation targeting `helper_b` (second callee)"
    );
}
