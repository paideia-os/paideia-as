//! v0.22.0 (#1326 phase 2): SysV x64 caller-side 7-arg call.
//!
//! Regression fixture for the missing SysV stack-passing emit path. This
//! test proves that a paideia-default caller can pass 7+ arguments to a
//! (default, unannotated) SysV callee, with the 7th argument spilled to
//! the stack slot immediately above the return address.
//!
//! Prior to v0.22.0 the caller-side arg loop broke out with T0521
//! ("SysV ABI: max 6 arguments supported") for arg_idx ≥ 6 — any paideia
//! caller passing 7+ args (e.g. paideia-os R51.M2 nvme_ns_dual_kind_mint)
//! was forced into a struct-pointer packing workaround.
//!
//! The tests below assert on the caller symbol `caller_7arg`:
//!
//!   1. `sub rsp, 16` prelude (1 stack arg = 8 bytes + 1 odd-count
//!      alignment pad = 8 bytes).
//!   2. `mov qword ptr [rsp + 0], 7` — arg[6] materialised at caller's
//!      [rsp+0], which the callee sees at [rsp+8] after the CALL push.
//!   3. `add rsp, 16` — postlude matches the prelude.

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_7arg_call_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_7arg_call.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "7-arg SysV call must NOT emit T0521 (stack passing now supported); stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn sysv_x64_7arg_call_prelude_is_16_bytes() {
    // sub rsp, 16 encodes as `48 83 EC 10` (7 args = 1 stack arg → 8 bytes
    // + 8-byte odd-count alignment pad = 16 = 0x10).
    let out = run_build(build_emit("sysv_x64_7arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_7arg")
        .expect("caller_7arg symbol missing from ELF");

    let sub_rsp_16 = [0x48u8, 0x83, 0xEC, 0x10];
    let has_prelude = caller.windows(4).any(|w| w == sub_rsp_16);
    assert!(
        has_prelude,
        "Expected `sub rsp, 16` (48 83 EC 10) prelude in caller_7arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_7arg_call_writes_stack_arg_at_rsp_plus_0() {
    // arg[6] = literal 7. The encoder narrows `mov qword ptr [rsp+0], 7`
    // to `48 C7 04 24 07 00 00 00` (48 C7 = REX.W + mov rm64 imm32;
    // 04 = ModRM disp0 with SIB, mod=00; 24 = SIB base=RSP;
    // 07 00 00 00 = imm32 value 7). No disp byte when disp == 0 and
    // base != RBP/R13.
    let out = run_build(build_emit("sysv_x64_7arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_7arg")
        .expect("caller_7arg symbol missing from ELF");

    let stack_store = [0x48u8, 0xC7, 0x04, 0x24, 0x07, 0x00, 0x00, 0x00];
    let has_store = caller.windows(8).any(|w| w == stack_store);
    assert!(
        has_store,
        "Expected `mov qword ptr [rsp+0], 7` in caller_7arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_7arg_call_postlude_matches_prelude() {
    // add rsp, 16 encodes as `48 83 C4 10`. The postlude MUST match the
    // prelude bump exactly (0x10 both sides).
    let out = run_build(build_emit("sysv_x64_7arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_7arg")
        .expect("caller_7arg symbol missing from ELF");

    let add_rsp_16 = [0x48u8, 0x83, 0xC4, 0x10];
    let has_postlude = caller.windows(4).any(|w| w == add_rsp_16);
    assert!(
        has_postlude,
        "Expected `add rsp, 16` (48 83 C4 10) postlude in caller_7arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn sysv_x64_7arg_call_register_args_use_sysv_regs() {
    // arg[0] = base (Var, already in RDI) → no-op MOV.
    // arg[1..5] = literals 2..6 → RSI, RDX, RCX, R8, R9.
    // We check for at least 2 REX.WB-prefixed (0x49) instructions, proving
    // R8/R9 (arg[4], arg[5]) were targeted — the SysV-specific extended
    // register pair that MS's RCX/RDX/R8/R9 pool does not exclusively
    // require in the same combination.
    let out = run_build(build_emit("sysv_x64_7arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_7arg")
        .expect("caller_7arg symbol missing from ELF");

    let sysv_extended_reg_prefix = caller.iter().filter(|&&b| b == 0x49).count();
    assert!(
        sysv_extended_reg_prefix >= 2,
        "Expected at least 2 REX.WB-prefixed (0x49) instructions in caller_7arg \
         (arg[4]→R8 and arg[5]→R9); got {} instances in: {}",
        sysv_extended_reg_prefix,
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}
