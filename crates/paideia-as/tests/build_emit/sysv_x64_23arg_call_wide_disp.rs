//! v0.22.0 (#1326 phase 4): encoder-emission ordering + wide-disp golden bytes.
//!
//! Phase 4 adds no production code (design doc §7-8): the caller-side
//! `mov qword ptr [rsp + N], imm` path (#1326 phase 2) and callee-side
//! `mov reg64, qword ptr [rbp + N]` path (#1326 phase 3) already route
//! through the generic MemSib encoder, which picks disp8 vs disp32 purely
//! on whether `N` fits `-128..=127` (see `encode.rs`'s repeated
//! `(-128..=127).contains(&disp)` guard). This suite is the golden-byte +
//! iced-x86 round-trip proof that the boundary behaves as designed once a
//! real arity (23 args: 6 register + 17 stack) pushes a stack-arg offset
//! past +127.
//!
//! `sysv_x64_23arg_call_wide_disp.pdx` calls a 23-arg SysV callee that
//! returns its LAST (23rd, stack-passed) parameter, so both the caller's
//! widest store (`[rsp+128]`) and the callee's read (`[rbp+144]`) land in
//! disp32 territory, while the second-widest store (`[rsp+120]`) is the
//! last one still encodable as disp8 — pinning down the exact transition.

use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind, Register};

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_23arg_call_wide_disp_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "23-arg SysV call must NOT emit T0521 (stack passing supported since \
         phase 2); stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr_contains("T0528"),
        "23-arg SysV callee stack-read must NOT emit T0528 (unresolved \
         binding); stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn sysv_x64_23arg_call_prelude_uses_disp32_imm_form() {
    // 17 stack args (idx 6..22) → 136 bytes + 8-byte odd-count pad = 144
    // (0x90). 144 > 127, so the `sub rsp, N` immediate itself must widen
    // from the `83 /5 ib` (imm8) form to `81 /5 id` (imm32):
    // `48 81 EC 90 00 00 00`.
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_23arg")
        .expect("caller_23arg symbol missing from ELF");

    let sub_rsp_144 = [0x48u8, 0x81, 0xEC, 0x90, 0x00, 0x00, 0x00];
    assert!(
        caller.windows(sub_rsp_144.len()).any(|w| w == sub_rsp_144),
        "Expected `sub rsp, 144` (48 81 EC 90 00 00 00) prelude in \
         caller_23arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_23arg_call_postlude_matches_prelude() {
    // add rsp, 144 → `48 81 C4 90 00 00 00`.
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_23arg")
        .expect("caller_23arg symbol missing from ELF");

    let add_rsp_144 = [0x48u8, 0x81, 0xC4, 0x90, 0x00, 0x00, 0x00];
    assert!(
        caller.windows(add_rsp_144.len()).any(|w| w == add_rsp_144),
        "Expected `add rsp, 144` (48 81 C4 90 00 00 00) postlude in \
         caller_23arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_23arg_call_stack_off_120_is_last_disp8_form() {
    // idx 21 (v), value 22: `mov qword ptr [rsp+120], 22` — 120 == 0x78
    // fits `mod=01` disp8: `48 C7 44 24 78 16 00 00 00`.
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_23arg")
        .expect("caller_23arg symbol missing from ELF");

    let store_off120 = [0x48u8, 0xC7, 0x44, 0x24, 0x78, 0x16, 0x00, 0x00, 0x00];
    assert!(
        caller.windows(store_off120.len()).any(|w| w == store_off120),
        "Expected `mov qword ptr [rsp+120], 22` (disp8 form) in \
         caller_23arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_23arg_call_stack_off_128_is_first_disp32_form() {
    // idx 22 (w), value 23: `mov qword ptr [rsp+128], 23` — 128 == 0x80
    // no longer fits disp8, so `mod=10` disp32 kicks in:
    // `48 C7 84 24 80 00 00 00 17 00 00 00`.
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_23arg")
        .expect("caller_23arg symbol missing from ELF");

    let store_off128 = [
        0x48u8, 0xC7, 0x84, 0x24, 0x80, 0x00, 0x00, 0x00, 0x17, 0x00, 0x00, 0x00,
    ];
    assert!(
        caller.windows(store_off128.len()).any(|w| w == store_off128),
        "Expected `mov qword ptr [rsp+128], 23` (disp32 form) in \
         caller_23arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_23arg_call_callee_has_frame_pointer_prologue() {
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "sysv_callee_23arg")
        .expect("sysv_callee_23arg symbol missing from ELF");

    let push_rbp_mov_rbp_rsp = [0x55u8, 0x48, 0x89, 0xE5];
    assert!(
        callee.windows(push_rbp_mov_rbp_rsp.len()).any(|w| w == push_rbp_mov_rbp_rsp),
        "Expected `push rbp; mov rbp, rsp` (55 48 89 E5) prologue in \
         sysv_callee_23arg; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_23arg_call_callee_reads_last_param_from_rbp_plus_144_disp32() {
    // param idx=22 (23rd, last), the 17th stack-passed param, lives at
    // BindingHome::StackSlot(16 + 8*(22-6)) = StackSlot(144). The bare-Var
    // body `-> w` lowers to `mov rax, [rbp+144]` — disp32, since 144 > 127:
    // `48 8B 85 90 00 00 00`.
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "sysv_callee_23arg")
        .expect("sysv_callee_23arg symbol missing from ELF");

    let mov_rax_rbp144 = [0x48u8, 0x8B, 0x85, 0x90, 0x00, 0x00, 0x00];
    assert!(
        callee.windows(mov_rax_rbp144.len()).any(|w| w == mov_rax_rbp144),
        "Expected `mov rax, [rbp+144]` (48 8B 85 90 00 00 00) in \
         sysv_callee_23arg; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

/// Compound-pattern round-trip: decode the *entire* `caller_23arg` body with
/// iced-x86 and walk every `mov [rsp + disp], imm` store in emission order,
/// confirming (a) there are exactly 17 of them, (b) their displacements are
/// the exact arithmetic sequence 0, 8, 16, ..., 128 the design's
/// `8 * (arg_idx - 6)` formula predicts — crossing the disp8/disp32 boundary
/// mid-sequence without a gap or reorder — (c) their immediate payloads are
/// the expected 7..23 literal values in the same order, and (d) the last
/// store is immediately followed by the `call` — i.e. every stack-arg write
/// lands before CALL, none straggling after it.
#[test]
fn sysv_x64_23arg_call_all_seventeen_stack_stores_land_at_right_offsets_before_call() {
    let out = run_build(build_emit("sysv_x64_23arg_call_wide_disp.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_23arg")
        .expect("caller_23arg symbol missing from ELF");

    let mut decoder = Decoder::new(64, &caller, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // (disp, imm) for every `mov [rsp+disp], imm` store, in decode order.
    let mut stores: Vec<(i64, u64)> = Vec::new();
    let mut call_idx: Option<usize> = None;
    for (i, instr) in insts.iter().enumerate() {
        if instr.mnemonic() == Mnemonic::Call && call_idx.is_none() {
            call_idx = Some(i);
        }
        if instr.mnemonic() == Mnemonic::Mov
            && instr.op0_kind() == OpKind::Memory
            && instr.memory_base() == Register::RSP
        {
            stores.push((instr.memory_displacement64() as i64, instr.immediate(1)));
        }
    }

    assert_eq!(
        stores.len(),
        17,
        "expected exactly 17 stack-arg stores (idx 6..22), got {}: {:?}",
        stores.len(),
        stores
    );

    let expected: Vec<(i64, u64)> = (0..17).map(|i| (i * 8, (7 + i) as u64)).collect();
    assert_eq!(
        stores, expected,
        "stack-arg stores must appear in strictly increasing-offset order \
         with the exact 8*(idx-6) displacement and idx+1-valued literal \
         payload the design formula predicts"
    );

    let call_idx = call_idx.expect("caller_23arg must contain a CALL instruction");
    // Find the index of the last store in `insts` (same relative order since
    // we only filtered, never reordered).
    let last_store_pos = insts
        .iter()
        .rposition(|instr| {
            instr.mnemonic() == Mnemonic::Mov
                && instr.op0_kind() == OpKind::Memory
                && instr.memory_base() == Register::RSP
        })
        .expect("at least one stack-arg store must be present");
    assert!(
        last_store_pos < call_idx,
        "every stack-arg store must precede CALL: last store at inst {}, \
         call at inst {}",
        last_store_pos,
        call_idx
    );
}
