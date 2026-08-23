//! v0.22.0 (#1326 phase 5): bridge composition -- paideia caller crossing
//! into an explicit `@abi("sysv")` 7-arg callee. Proves
//! `sysv_stack_arg_bytes + sysv_stack_arg_pad` (the phase 2 stack-arg
//! reservation) composes additively with `sysv_align_pad` (the
//! pre-existing #1195 bridge-parity pad) rather than one overriding the
//! other:
//!
//!   sysv_stack_arg_bytes = 8   (1 stack arg * 8)
//!   sysv_stack_arg_pad   = 8   (odd stack-arg count)
//!   sysv_align_pad       = 8   (bridge_saves non-empty, scratch count 0 -> even)
//!   sysv_bump            = 24
//!
//! Distinguishing evidence versus the no-bridge case
//! (`sysv_x64_7arg_call.pdx`, bump=16): if the two addends did NOT
//! compose (e.g. one silently overrode the other) this fixture would
//! observe `sub rsp, 16` instead of `sub rsp, 24`.

use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind, Register};

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_bridge_stack_compose_compiles_cleanly() {
    let out = run_build(build_emit("sysv_bridge_stack_compose.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "bridge-composition fixture must NOT emit T0521; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn f_emits_bridge_pushes_then_sub_rsp_24() {
    // push r15 (41 57), push r14 (41 56), sub rsp, 24 (48 83 EC 18) --
    // 24 = 8 (stack-arg bytes) + 8 (stack-arg odd-count pad) + 8
    // (#1195 align pad), all three composing into one prelude bump.
    let out = run_build(build_emit("sysv_bridge_stack_compose.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let f_bytes = elf::symbol_bytes(&bytes, "f").expect("f symbol missing from ELF");

    let expected = [
        0x41u8, 0x57, // push r15
        0x41, 0x56, // push r14
        0x48, 0x83, 0xEC, 0x18, // sub rsp, 24
    ];
    assert!(
        f_bytes.windows(expected.len()).any(|w| w == expected),
        "Expected `push r15; push r14; sub rsp, 24` prefix in f; got: {}",
        f_bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn f_writes_single_stack_arg_at_off_0() {
    // 7 args -> 1 stack arg (idx 6) -> stack_off = 8*(6-6) = 0.
    let out = run_build(build_emit("sysv_bridge_stack_compose.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let f_bytes = elf::symbol_bytes(&bytes, "f").expect("f symbol missing from ELF");

    let store_off0_val7 = [0x48u8, 0xC7, 0x04, 0x24, 0x07, 0x00, 0x00, 0x00];
    assert!(
        f_bytes.windows(store_off0_val7.len()).any(|w| w == store_off0_val7),
        "Expected `mov qword ptr [rsp+0], 7` in f; got: {}",
        f_bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn f_postlude_and_bridge_pop_match_prelude() {
    // add rsp, 24 (48 83 C4 18), then pop r14 (41 5E), pop r15 (41 5F) --
    // LIFO relative to the push order in the prelude test above.
    let out = run_build(build_emit("sysv_bridge_stack_compose.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let f_bytes = elf::symbol_bytes(&bytes, "f").expect("f symbol missing from ELF");

    let expected = [
        0x48u8, 0x83, 0xC4, 0x18, // add rsp, 24
        0x41, 0x5E, // pop r14
        0x41, 0x5F, // pop r15
    ];
    assert!(
        f_bytes.windows(expected.len()).any(|w| w == expected),
        "Expected `add rsp, 24; pop r14; pop r15` sequence in f; got: {}",
        f_bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn f_instruction_order_pushes_before_sub_before_store_before_call() {
    // Decode-based ordering proof, complementing the raw-byte checks
    // above: bridge pushes, then the combined sub rsp bump, then the
    // stack-arg store, then CALL -- no reordering across the composed
    // prelude.
    let out = run_build(build_emit("sysv_bridge_stack_compose.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let f_bytes = elf::symbol_bytes(&bytes, "f").expect("f symbol missing from ELF");

    let mut decoder = Decoder::new(64, &f_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    let mut push_r15_idx = None;
    let mut push_r14_idx = None;
    let mut sub_idx = None;
    let mut store_idx = None;
    let mut call_idx = None;

    for (i, instr) in insts.iter().enumerate() {
        match instr.mnemonic() {
            Mnemonic::Push if instr.op0_register() == Register::R15 && push_r15_idx.is_none() => {
                push_r15_idx = Some(i);
            }
            Mnemonic::Push if instr.op0_register() == Register::R14 && push_r14_idx.is_none() => {
                push_r14_idx = Some(i);
            }
            Mnemonic::Sub if sub_idx.is_none() => {
                sub_idx = Some(i);
            }
            Mnemonic::Call if call_idx.is_none() => {
                call_idx = Some(i);
            }
            Mnemonic::Mov
                if instr.op0_kind() == OpKind::Memory
                    && instr.memory_base() == Register::RSP
                    && store_idx.is_none() =>
            {
                store_idx = Some(i);
            }
            _ => {}
        }
    }

    let push_r15_idx = push_r15_idx.expect("push r15 must appear");
    let push_r14_idx = push_r14_idx.expect("push r14 must appear");
    let sub_idx = sub_idx.expect("sub rsp must appear");
    let store_idx = store_idx.expect("stack-arg store must appear");
    let call_idx = call_idx.expect("CALL must appear");

    assert!(push_r15_idx < push_r14_idx, "push r15 must precede push r14");
    assert!(push_r14_idx < sub_idx, "bridge pushes must precede sub rsp");
    assert!(sub_idx < store_idx, "sub rsp must precede the stack-arg store");
    assert!(store_idx < call_idx, "stack-arg store must precede CALL");
}
