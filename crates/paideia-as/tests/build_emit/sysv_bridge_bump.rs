//! Issue #1195: paideia→SysV cross-ABI callees need RSP alignment padding when
//! scratch_save_set count is even.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn n0_case_emits_sub_rsp_8() {
    let out = run_build(build_emit("sysv_bridge_bump_zero.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    // sub rsp, 8 = 48 83 EC 08
    let seq = [0x48u8, 0x83, 0xEC, 0x08];
    assert!(bytes.windows(4).any(|w| w == seq),
        "expected `sub rsp, 8` (48 83 EC 08) in .text for N=0 paideia→SysV");
}

#[test]
fn n2_case_emits_sub_rsp_8() {
    let out = run_build(build_emit("sysv_bridge_bump_two.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let seq = [0x48u8, 0x83, 0xEC, 0x08];
    assert!(bytes.windows(4).any(|w| w == seq),
        "expected `sub rsp, 8` (48 83 EC 08) in .text for N=2 paideia→SysV");
}

#[test]
fn n1_case_no_sub_rsp_8() {
    let out = run_build(build_emit("sysv_bridge_bump_one.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    // For N=1, no sysv_bump should be added around the sysv_callee call.
    // The bytes 48 83 EC 08 could appear elsewhere in the module though (e.g.,
    // in other helpers). Instead: extract f's symbol bytes and check there's
    // no sub rsp, 8 within f.
    let f_bytes = crate::common::elf::symbol_bytes(&bytes, "f")
        .expect("symbol 'f' should exist in .text");
    let seq = [0x48u8, 0x83, 0xEC, 0x08];
    assert!(!f_bytes.windows(4).any(|w| w == seq),
        "did NOT expect `sub rsp, 8` in f for N=1 paideia→SysV (already aligned by parity)");
}
