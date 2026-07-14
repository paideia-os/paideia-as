//! Issue #1191 corrective 2 — value-level runtime verification for let-RHS operator App.
//!
//! Tests that let-bindings with operator App on the RHS correctly emit code
//! and return the expected value. This verifies the fix for the Placeholder-vs-Var
//! dead-code bug at let-RHS statement position.
//!
//! Calculation: f(0x37)
//!   x = 0x37 & 15 = 7
//!   y = 0x37 >> 4 = 3
//!   x + y = 10

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_with_binop_rhs_returns_10() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_with_binop_rhs.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
