//! Issue #1163 (corrective) — value-level runtime verification for the
//! SysV 4-argument case (arg3 marshals into RCX, which must be saved if a
//! prior binding lives there).
//!
//! Mirrors ms_call_saves_rcx_value.rs but exercises the SysV take4(...) call
//! shape instead of an MS-ABI callee. combo(5) == 15 (helper_a(5) = 15).
use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

// #1190 fix: entry-wrapper SIGSEGV is resolved. This test validates both
// the local_bindings scoping fix and the #1163 corrective fix (Defect A) end-to-end.
#[test]
fn combo_returns_15_4arg_sysv_call() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF05 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF06 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF07 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF08 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "two_let_with_4arg_call_saves_rcx.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 15,
    };
    run_and_verify(&case);
}
