//! Issue #1163 (corrective) — value-level runtime verification.
//!
//! Byte-pattern assertions (two_let_with_ms_call_saves_rcx_assembles in
//! build_emit) confirm push/pop ordering around the MS-ABI call, but do not
//! confirm the actual returned VALUE is correct. This test links the compiled
//! fixture against a C driver, poisons RCX/RDX/R8/R9 before the call (as a
//! real caller would leave garbage in caller-save registers), and checks that
//! combo(5) == 15 (helper_a(5) = 5 + 10 = 15) is what actually comes back in
//! RAX -- i.e. Defect A (inverted used_arg_regs filter) is fixed for real,
//! not just at the byte-pattern level.
use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

// #[ignore]: this fixture's `entry` wrapper currently SIGSEGVs at runtime due
// to #1190 (local_bindings leaks across the App-bodied `entry` lambda
// boundary, producing an unmatched `pop` with no executed `push`). This is a
// regression canary -- remove `#[ignore]` once #1190 is fixed and confirm
// this then also validates the #1163 corrective fix (Defect A) end-to-end.
#[test]
#[ignore = "blocked on #1190: entry-wrapper SIGSEGV (local_bindings leak / unmatched pop)"]
fn combo_returns_15_ms_abi_call() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "two_let_with_ms_call_saves_rcx.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 15,
    };
    run_and_verify(&case);
}
