//! Issue #1195 — value-level runtime verification (SysV sister of #1163's MS test).
//!
//! Byte-pattern assertions (sysv_bridge_bump.rs in build_emit) confirm the
//! alignment-pad prelude/postlude bytes are present/absent for the right N,
//! but do not confirm the actual returned VALUE survives a live scratch
//! binding being spilled/restored around a paideia→SysV cross-ABI CALL.
//!
//! Fixture: f(v) = { let a = helper_a(v); let b = sysv_callee(v); a }
//! `a` (held in RCX, the sole caller-save scratch binding, N=1 — odd parity,
//! so no alignment pad fires per #1195) must survive the push/pop around the
//! bridge-save (R15/R14) + CALL sysv_callee sequence and come back correctly
//! in RAX. entry() = f(5) = helper_a(5) = 15.
use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn f_returns_15_sysv_bridge_call() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "sysv_bridge_bump_value.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 15,
    };
    run_and_verify(&case);
}

/// N=2 (even scratch-save parity, alignment pad `sub rsp, 8` fires per #1195).
/// `a` (RCX) and `b` (RDX) must both survive the push/pop pair around the
/// padded bridge+CALL sequence. entry() = f(5) = a + b = 15 + 25 = 40.
#[test]
fn f_returns_40_sysv_bridge_call_n2_padded() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "sysv_bridge_bump_value_n2.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 40,
    };
    run_and_verify(&case);
}
