//! Issue #1210 verification 1 — value-level runtime verification for integer-match literal zero.
//!
//! Tests that match expressions over integer literals correctly dispatch to the
//! appropriate arm based on the scrutinee value.
//!
//! Expected: 10

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn intmatch_lit_zero_returns_10() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "intmatch_lit_zero.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
