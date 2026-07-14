//! Issue #1194 corrective 2 — value-level runtime verification for let-RHS BitNot with literal.
//!
//! Tests that let-bindings with bitwise NOT of a literal value on the RHS
//! correctly emit code and return the expected value. This is an adversarial
//! test using a different literal (~16) to ensure BitNot emission works
//! beyond just ~15.
//!
//! Calculation: f(0xFF)
//!   mask = ~16 = 0xFFFFFFFFFFFFFFEF
//!   result = 0xFF & 0xFFFFFFFFFFFFFFEF = 0xEF = 239

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_with_bitnot_shl_rhs_returns_239() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_with_bitnot_shl_rhs.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 239,
    };
    run_and_verify(&case);
}
