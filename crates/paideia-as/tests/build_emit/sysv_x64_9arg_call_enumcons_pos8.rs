//! v0.22.0 (#1326 phase 5): SysV stack arg -- bare enum-variant literal
//! (EnumCons, nullary) at position 8 (0-indexed arg 7, the SECOND
//! stack-passed slot) of a 9-arg SysV call.
//!
//! Companion to `sysv_x64_9arg_call_object_const.pdx` (position 7):
//! proves the EnumCons stack-arg branch (emit_call.rs) composes at a
//! non-first stack slot, writing the variant index as an immediate via
//! `emit_mov_stack_slot_imm`, exactly like a literal argument would.

use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind, Register};

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_9arg_call_enumcons_pos8_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_9arg_call_enumcons_pos8.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "9-arg SysV call with EnumCons at pos 8 must NOT emit T0521; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn caller_9arg_enumcons_pos8_writes_variant_index_at_stack_off_8() {
    // B is the second variant of `Choice` (A=0, B=1); arg_idx=7 ->
    // stack_off = 8*(7-6) = 8.
    let out = run_build(build_emit("sysv_x64_9arg_call_enumcons_pos8.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg_enumcons_pos8")
        .expect("caller_9arg_enumcons_pos8 symbol missing from ELF");

    let store_off8_val1 = [0x48u8, 0xC7, 0x44, 0x24, 0x08, 0x01, 0x00, 0x00, 0x00];
    assert!(
        caller.windows(store_off8_val1.len()).any(|w| w == store_off8_val1),
        "Expected `mov qword ptr [rsp+8], 1` (Choice::B variant index) in \
         caller_9arg_enumcons_pos8; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn caller_9arg_enumcons_pos8_flanking_literals_and_ordering() {
    // arg[6]=7 at [rsp+0], arg[7]=B(1) at [rsp+8], arg[8]=9 at [rsp+16] --
    // all three stores must appear, in increasing-offset order, strictly
    // before CALL (mirrors the phase 4 compound-pattern check, scoped to
    // 3 stores instead of 17).
    let out = run_build(build_emit("sysv_x64_9arg_call_enumcons_pos8.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg_enumcons_pos8")
        .expect("caller_9arg_enumcons_pos8 symbol missing from ELF");

    let mut decoder = Decoder::new(64, &caller, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    let mut stores: Vec<(i64, u64)> = Vec::new();
    let mut call_idx: Option<usize> = None;
    let mut last_store_idx: Option<usize> = None;
    for (i, instr) in insts.iter().enumerate() {
        if instr.mnemonic() == Mnemonic::Call && call_idx.is_none() {
            call_idx = Some(i);
        }
        if instr.mnemonic() == Mnemonic::Mov
            && instr.op0_kind() == OpKind::Memory
            && instr.memory_base() == Register::RSP
        {
            stores.push((instr.memory_displacement64() as i64, instr.immediate(1)));
            last_store_idx = Some(i);
        }
    }

    assert_eq!(
        stores,
        vec![(0i64, 7u64), (8i64, 1u64), (16i64, 9u64)],
        "expected exactly the 3 stack-arg stores in increasing-offset order \
         with values [7 (literal), 1 (Choice::B), 9 (literal)]; got {:?}",
        stores
    );

    let call_idx = call_idx.expect("caller must contain a CALL instruction");
    let last_store_idx = last_store_idx.expect("at least one stack-arg store must be present");
    assert!(
        last_store_idx < call_idx,
        "every stack-arg store must precede CALL: last store at {}, call at {}",
        last_store_idx,
        call_idx
    );
}
