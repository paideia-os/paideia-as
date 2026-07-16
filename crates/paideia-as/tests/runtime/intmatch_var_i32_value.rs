//! Issue #1218 verification — value-level runtime verification for variable-scrutinee integer match (i32 width).
//!
//! Tests that match expressions with variable scrutinee (i32 width)
//! correctly classify as integer match and dispatch to the correct arm.
//!
//! Expected: 30

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn intmatch_var_i32_returns_30() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "intmatch_var_i32.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 30,
    };
    run_and_verify(&case);
}
