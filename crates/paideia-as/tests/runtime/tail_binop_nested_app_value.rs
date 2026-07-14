//! Issue #1191 — value-level runtime verification (nested app).
//!
//! Tests that a tail-position BinOp with a nested function call as one operand
//! correctly returns the expected value.

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn tail_binop_nested_app_returns_10() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "tail_binop_nested_app.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
