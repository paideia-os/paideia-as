//! v0.21-001 (#1277, closes #1011): MS x64 caller-side 5-arg call.
//!
//! Regression fixture for the missing MS x64 stack-passing emit path. This
//! test proves that a paideia-default caller can pass 5+ arguments to an
//! `@abi("ms")` callee, with the 5th argument spilled to the stack slot
//! immediately above the 32-byte shadow area.
//!
//! Prior to v0.21-001 the caller-side arg loop broke out with T0521
//! ("MS x64 ABI: max 4 arguments supported") for arg_idx ≥ 4 and produced
//! an image where the 5th arg was silently dropped — paideia-os R19 UEFI
//! services (GetVariable, SetVariable, OpenProtocol, …) with 5+ args would
//! fault at runtime with garbage in the 5th slot.
//!
//! The tests below assert on the caller symbol `caller_5arg`:
//!
//!   1. `sub rsp, 56` prelude (0x30 == 40 was the ≤4-arg baseline; the
//!      new prelude adds 8 for arg[4] + 8 for odd-count alignment pad).
//!   2. `mov qword ptr [rsp + 32], 5` — arg[4] materialised at caller's
//!      [rsp+32], which the callee sees at [rsp+40] after the CALL push.
//!   3. `add rsp, 56` — postlude matches the prelude.
//!
//! Note: the caller is paideia-default, so paideia→MS bridge saves
//! (push r15; push r14) run BEFORE the sub prelude — the byte scan below
//! searches for the prelude anywhere in the function body, not at a
//! specific offset.

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn ms_x64_5arg_call_compiles_cleanly() {
    let out = run_build(build_emit("ms_x64_5arg_call.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "5-arg MS call must NOT emit T0521 (stack passing now supported); stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr_contains("U1620"),
        "5-arg MS callee/caller must NOT emit U1620; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn ms_x64_5arg_call_prelude_is_56_bytes() {
    // sub rsp, 56 encodes as `48 83 EC 38` (5 args = 1 odd stack arg →
    //   40 baseline + 8 stack-arg-bytes + 8 odd-alignment pad = 56 = 0x38).
    // Regression fence: if the alignment math drifts, this pattern flips
    // to 0x30 (48) or 0x40 (64) and the test catches it before UEFI does.
    let out = run_build(build_emit("ms_x64_5arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_5arg")
        .expect("caller_5arg symbol missing from ELF");

    let sub_rsp_56 = [0x48u8, 0x83, 0xEC, 0x38];
    let has_prelude = caller.windows(4).any(|w| w == sub_rsp_56);
    assert!(
        has_prelude,
        "Expected `sub rsp, 56` (48 83 EC 38) prelude in caller_5arg; got: {}",
        caller.iter().take(64).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn ms_x64_5arg_call_writes_stack_arg_at_rsp_plus_32() {
    // arg[4] = literal 5. The encoder narrows `mov qword ptr [rsp+32], 5`
    // to `48 C7 44 24 20 05 00 00 00` (48 C7 = REX.W + mov rm64 imm32;
    // 44 = ModRM disp8 with SIB; 24 = SIB base=RSP; 20 = disp8 (32);
    // 05 00 00 00 = imm32 value 5).
    let out = run_build(build_emit("ms_x64_5arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_5arg")
        .expect("caller_5arg symbol missing from ELF");

    let stack_store = [0x48u8, 0xC7, 0x44, 0x24, 0x20, 0x05, 0x00, 0x00, 0x00];
    let has_store = caller.windows(9).any(|w| w == stack_store);
    assert!(
        has_store,
        "Expected `mov qword ptr [rsp+32], 5` (48 C7 44 24 20 05 00 00 00) in caller_5arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn ms_x64_5arg_call_postlude_matches_prelude() {
    // add rsp, 56 encodes as `48 83 C4 38`. The postlude MUST match the
    // prelude bump exactly (0x38 both sides); a drift here is the canary
    // for a stack corruption bug in the caller-restore path.
    let out = run_build(build_emit("ms_x64_5arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_5arg")
        .expect("caller_5arg symbol missing from ELF");

    let add_rsp_56 = [0x48u8, 0x83, 0xC4, 0x38];
    let has_postlude = caller.windows(4).any(|w| w == add_rsp_56);
    assert!(
        has_postlude,
        "Expected `add rsp, 56` (48 83 C4 38) postlude in caller_5arg; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn ms_x64_5arg_call_register_args_use_ms_regs() {
    // First 4 args go to RCX, RDX, R8, R9 per MS x64 convention.
    // - arg[0] = base (in RDI as SysV arg 0 of the caller) → RCX
    //   `mov rcx, rdi` = 48 89 F9
    // - arg[1] = 2  → `mov rdx, 2` = 48 C7 C2 02 00 00 00
    // - arg[2] = 3  → `mov r8, 3` = 49 C7 C0 03 00 00 00
    // - arg[3] = 4  → `mov r9, 4` = 49 C7 C1 04 00 00 00
    //
    // We only check the RCX ← RDI move (identity of arg[0]) and the
    // presence of R8/R9 encoding-prefix bytes (49) — enough to prove the
    // MS register mapping fired without over-pinning the encoding choice
    // (the encoder may pick a different imm form for small constants).
    let out = run_build(build_emit("ms_x64_5arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_5arg")
        .expect("caller_5arg symbol missing from ELF");

    // Prove arg[0] passes via RCX by scanning for `mov rcx, rdi` (48 89 F9).
    // This proves the ABI selection worked: SysV would leave arg[0] in RDI
    // untouched.
    let mov_rcx_rdi = [0x48u8, 0x89, 0xF9];
    assert!(
        caller.windows(3).any(|w| w == mov_rcx_rdi),
        "Expected `mov rcx, rdi` (48 89 F9) marshalling arg[0] to MS RCX slot; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );

    // Prove R8 and R9 are targeted by the arg[2]/arg[3] MOVs — REX.WB
    // prefix (0x49) is required to encode a 64-bit dest in R8/R9. If the
    // caller wrote to R11 by mistake (also 0x49-prefixed) we'd still see
    // 0x49 bytes, so this is a coarser check than the arg[0] one — but
    // combined with the shadow-space and stack-arg assertions above, the
    // MS ABI shape is pinned.
    let ms_extended_reg_prefix = caller.iter().filter(|&&b| b == 0x49).count();
    assert!(
        ms_extended_reg_prefix >= 2,
        "Expected at least 2 REX.WB-prefixed (0x49) instructions in caller_5arg \
         (arg[2]→R8 and arg[3]→R9); got {} instances in: {}",
        ms_extended_reg_prefix,
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}
