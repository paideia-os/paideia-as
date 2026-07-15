//! Issue #1209 corrective — value-level runtime verification for let-chain with multiple Var RHS.
//!
//! Tests that three sequential let bindings with Var RHS correctly emit code to copy
//! source registers to scratch registers, and return the expected value.
//!
//! Calculation: f()
//!   x = 42
//!   y = x (copy x to scratch)
//!   z = y (copy y to scratch)
//!   return z = 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_chain_3_of_3_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_chain_3_of_3.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
