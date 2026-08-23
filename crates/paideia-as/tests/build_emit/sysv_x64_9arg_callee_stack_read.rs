//! v0.22.0 (#1326 phase 3): SysV callee-side 9-arg stack-arg intake.
//!
//! Companion to sysv_x64_7arg_callee_stack_read.rs, exercising a wider
//! stack-arg span: 3 stack-passed params at [rbp+16], [rbp+24], [rbp+32].
//! The callee body reads its LAST parameter (idx=8, the 9th arg) — the
//! shape that motivated #1326 (paideia-os R51.M2 nvme_ns_dual_kind_mint).

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_9arg_callee_stack_read_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_9arg_callee_stack_read.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "9-arg SysV callee stack-read must NOT emit T0521; stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr_contains("T0528"),
        "9-arg SysV callee stack-read must NOT emit T0528 (unresolved binding); stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn sysv_x64_9arg_callee_reads_ninth_param_from_rbp_plus_32() {
    // v0.22.0 (#1326 phase 3): param idx=8 (9th param, 3rd stack-passed) is
    // installed at BindingHome::StackSlot(32) — [rbp+32]. The bare-Var
    // body `-> i` lowers to `mov rax, [rbp+32]` (48 8B 45 20).
    let out = run_build(build_emit("sysv_x64_9arg_callee_stack_read.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "sysv_callee_9arg")
        .expect("sysv_callee_9arg symbol missing from ELF");

    let mov_rax_rbp32 = [0x48u8, 0x8B, 0x45, 0x20];
    assert!(
        callee.windows(mov_rax_rbp32.len()).any(|w| w == mov_rax_rbp32),
        "Expected `mov rax, [rbp+32]` (48 8B 45 20) in sysv_callee_9arg; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_9arg_callee_has_frame_pointer_prologue() {
    let out = run_build(build_emit("sysv_x64_9arg_callee_stack_read.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "sysv_callee_9arg")
        .expect("sysv_callee_9arg symbol missing from ELF");

    let push_rbp_mov_rbp_rsp = [0x55u8, 0x48, 0x89, 0xE5];
    assert!(
        callee.windows(push_rbp_mov_rbp_rsp.len()).any(|w| w == push_rbp_mov_rbp_rsp),
        "Expected `push rbp; mov rbp, rsp` (55 48 89 E5) prologue in sysv_callee_9arg; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}
