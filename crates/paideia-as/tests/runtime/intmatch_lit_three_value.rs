//! Issue #1210 verification 3 — value-level runtime verification for integer-match 3-arm.
//!
//! Tests that match expressions with 3 arms correctly dispatch to the
//! second arm when scrutinee is 1u64.
//!
//! Expected: 20

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn intmatch_lit_three_returns_20() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "intmatch_lit_three.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 20,
    };
    run_and_verify(&case);
}
