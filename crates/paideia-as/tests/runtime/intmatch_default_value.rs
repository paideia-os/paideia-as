//! Issue #1210 verification 5 — value-level runtime verification for integer-match default arm.
//!
//! Tests that match expressions correctly dispatch to the default arm when
//! scrutinee matches no specific pattern.
//!
//! Expected: 99

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn intmatch_default_returns_99() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "intmatch_default.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 99,
    };
    run_and_verify(&case);
}
