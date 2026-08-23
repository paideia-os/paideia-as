//! v0.22.0 (#1326 phase 6): smoke fixture mirroring the actual paideia-os
//! `nvme_ns_dual_kind_mint` 9-arg shape (the signature that motivated
//! this issue — see design/compiler/lambda-arity-stack-spill.md §1 and
//! design/hardware/ahci-substrate.md in the paideia-os sibling repo).
//!
//! Unlike the generic `sysv_x64_9arg_*.pdx` fixtures from phases 2/3
//! (placeholder `a..i` param names), this fixture uses the real field
//! names (`ns_slot`, `blk_slot`, ..., `parent_ctrl_row`) and reads the
//! real last field back through the stack-passed path, so a future
//! reader auditing #1326 can match this fixture directly against the
//! paideia-os call site it unblocks.

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_nvme_ns_dual_kind_mint_shape_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_nvme_ns_dual_kind_mint_shape.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "9-arg nvme_ns_dual_kind_mint-shaped call must NOT emit T0521; stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr_contains("P0276"),
        "9-arg nvme_ns_dual_kind_mint-shaped lambda must NOT emit P0276; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn sysv_x64_nvme_ns_dual_kind_mint_shape_reads_parent_ctrl_row_from_rbp_plus_32() {
    // parent_ctrl_row is param idx=8 (9th param, 3rd stack-passed param),
    // installed at BindingHome::StackSlot(32) — [rbp+32]. The bare-Var
    // body `-> parent_ctrl_row` lowers to `mov rax, [rbp+32]`
    // (48 8B 45 20), same offset as the generic 9-arg fixture but now
    // pinned against the real field name.
    let out = run_build(build_emit("sysv_x64_nvme_ns_dual_kind_mint_shape.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let callee = elf::symbol_bytes(&bytes, "callee_mint").expect("callee_mint symbol missing from ELF");

    let mov_rax_rbp32 = [0x48u8, 0x8B, 0x45, 0x20];
    assert!(
        callee.windows(mov_rax_rbp32.len()).any(|w| w == mov_rax_rbp32),
        "Expected `mov rax, [rbp+32]` (48 8B 45 20) reading parent_ctrl_row in callee_mint; got: {}",
        callee.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_nvme_ns_dual_kind_mint_shape_caller_bump_is_32() {
    // 3 stack args (lba_size, block_count, parent_ctrl_row) => bytes=24;
    // 3 is odd => pad=8 => sysv_bump=32, matching design doc §4.2's
    // worked 9-arg example.
    let out = run_build(build_emit("sysv_x64_nvme_ns_dual_kind_mint_shape.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller =
        elf::symbol_bytes(&bytes, "caller_mint").expect("caller_mint symbol missing from ELF");

    // sub rsp, 32 => 48 83 EC 20
    let sub_rsp_32 = [0x48u8, 0x83, 0xEC, 0x20];
    assert!(
        caller.windows(sub_rsp_32.len()).any(|w| w == sub_rsp_32),
        "Expected `sub rsp, 32` (48 83 EC 20) prelude in caller_mint; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}
