//! Issue #1163 (corrective) — value-level runtime verification for the
//! SysV 4-argument case (arg3 marshals into RCX, which must be saved if a
//! prior binding lives there).
//!
//! Mirrors ms_call_saves_rcx_value.rs but exercises the SysV take4(...) call
//! shape instead of an MS-ABI callee. combo(5) == 15 (helper_a(5) = 15).
use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

// #[ignore]: this fixture's `entry` wrapper currently SIGSEGVs at runtime due
// to #1190 (local_bindings leaks across the App-bodied `entry` lambda
// boundary, producing an unmatched `pop` with no executed `push`). This is a
// regression canary -- remove `#[ignore]` once #1190 is fixed and confirm
// this then also validates the #1163 corrective fix (Defect A) end-to-end.
#[test]
#[ignore = "blocked on #1190: entry-wrapper SIGSEGV (local_bindings leak / unmatched pop)"]
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
