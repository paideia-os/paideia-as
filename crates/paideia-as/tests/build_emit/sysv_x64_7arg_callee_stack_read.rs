//! v0.22.0 (#1326 phase 3): SysV callee-side 7-arg stack-arg intake.
//!
//! Companion to sysv_x64_7arg_call.rs (phase 2), which exercised only the
//! caller-side stack-spill. This exercises the phase-3 callee-side path:
//! the callee body is a bare reference to its 7th parameter (idx=6, the
//! first stack-passed param), which `register_nested_lambda_params` now
//! installs as `BindingHome::StackSlot(16)` — read back via
//! `mov rax, [rbp+16]`.

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_7arg_callee_stack_read_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_7arg_callee_stack_read.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "7-arg SysV callee stack-read must NOT emit T0521; stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr_contains("T0528"),
        "7-arg SysV callee stack-read must NOT emit T0528 (unresolved binding); stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn sysv_x64_7arg_callee_reads_seventh_param_from_rbp_plus_16() {
    // v0.22.0 (#1326 phase 3): param idx=6 (7th param, first stack-passed)
    // is installed at BindingHome::StackSlot(16) — [rbp+16]. The bare-Var
    // body `-> g` lowers to `mov rax, [rbp+16]` (48 8B 45 10).
    let out = run_build(build_emit("sysv_x64_7arg_callee_stack_read.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "sysv_callee_7arg")
        .expect("sysv_callee_7arg symbol missing from ELF");

    let mov_rax_rbp16 = [0x48u8, 0x8B, 0x45, 0x10];
    assert!(
        callee.windows(mov_rax_rbp16.len()).any(|w| w == mov_rax_rbp16),
        "Expected `mov rax, [rbp+16]` (48 8B 45 10) in sysv_callee_7arg; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_7arg_callee_has_frame_pointer_prologue() {
    // The callee is not `@no_frame` and not unsafe-bodied, so it must get
    // the default frame-pointer prologue (`push rbp; mov rbp, rsp` = 55 48
    // 89 E5) — required for the [rbp+16] read above to be meaningful.
    let out = run_build(build_emit("sysv_x64_7arg_callee_stack_read.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "sysv_callee_7arg")
        .expect("sysv_callee_7arg symbol missing from ELF");

    let push_rbp_mov_rbp_rsp = [0x55u8, 0x48, 0x89, 0xE5];
    assert!(
        callee.windows(push_rbp_mov_rbp_rsp.len()).any(|w| w == push_rbp_mov_rbp_rsp),
        "Expected `push rbp; mov rbp, rsp` (55 48 89 E5) prologue in sysv_callee_7arg; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}
