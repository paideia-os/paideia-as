//! v0.22.0 (#1326 phase 2): SysV x64 caller-side 9-arg call.
//!
//! Companion to sysv_x64_7arg_call.rs: proves the even-plus-pad stack-arg
//! arithmetic (9 args → 3 stack args → bytes=24, pad=8, bump=32) and pins
//! down the exact arity shape that motivated #1326 (paideia-os R51.M2
//! nvme_ns_dual_kind_mint, 9 args).

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_9arg_call_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_9arg_call.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "9-arg SysV call must NOT emit T0521 (stack passing now supported); stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn sysv_x64_9arg_call_prelude_is_32_bytes() {
    // sub rsp, 32 encodes as `48 83 EC 20` (9 args = 3 stack args → 24
    // bytes + 8-byte odd-count alignment pad = 32 = 0x20).
    let out = run_build(build_emit("sysv_x64_9arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg")
        .expect("caller_9arg symbol missing from ELF");

    let sub_rsp_32 = [0x48u8, 0x83, 0xEC, 0x20];
    let has_prelude = caller.windows(4).any(|w| w == sub_rsp_32);
    assert!(
        has_prelude,
        "Expected `sub rsp, 32` (48 83 EC 20) prelude in caller_9arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_9arg_call_writes_three_stack_args() {
    // arg[6]=7 at [rsp+0], arg[7]=8 at [rsp+8], arg[8]=9 at [rsp+16].
    let out = run_build(build_emit("sysv_x64_9arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg")
        .expect("caller_9arg symbol missing from ELF");

    let store_off0 = [0x48u8, 0xC7, 0x04, 0x24, 0x07, 0x00, 0x00, 0x00];
    let store_off8 = [0x48u8, 0xC7, 0x44, 0x24, 0x08, 0x08, 0x00, 0x00, 0x00];
    let store_off16 = [0x48u8, 0xC7, 0x44, 0x24, 0x10, 0x09, 0x00, 0x00, 0x00];

    assert!(
        caller.windows(store_off0.len()).any(|w| w == store_off0),
        "Expected `mov qword ptr [rsp+0], 7` in caller_9arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
    assert!(
        caller.windows(store_off8.len()).any(|w| w == store_off8),
        "Expected `mov qword ptr [rsp+8], 8` in caller_9arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
    assert!(
        caller.windows(store_off16.len()).any(|w| w == store_off16),
        "Expected `mov qword ptr [rsp+16], 9` in caller_9arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_9arg_call_postlude_matches_prelude() {
    // add rsp, 32 encodes as `48 83 C4 20`.
    let out = run_build(build_emit("sysv_x64_9arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg")
        .expect("caller_9arg symbol missing from ELF");

    let add_rsp_32 = [0x48u8, 0x83, 0xC4, 0x20];
    let has_postlude = caller.windows(4).any(|w| w == add_rsp_32);
    assert!(
        has_postlude,
        "Expected `add rsp, 32` (48 83 C4 20) postlude in caller_9arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}
