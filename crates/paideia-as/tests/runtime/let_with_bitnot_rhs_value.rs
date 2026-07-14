//! Issue #1194 corrective 1 — value-level runtime verification for let-RHS BitNot.
//!
//! Tests that let-bindings with bitwise NOT (~expr) on the RHS correctly emit code
//! and return the expected value.
//!
//! Calculation: f(0x37)
//!   mask = ~15 = 0xFFFFFFFFFFFFFFF0
//!   result = 0x37 & 0xFFFFFFFFFFFFFFF0 = 0x30 = 48

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_with_bitnot_rhs_returns_48() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_with_bitnot_rhs.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 48,
    };
    run_and_verify(&case);
}
