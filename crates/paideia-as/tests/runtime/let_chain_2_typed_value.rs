//! Issue #1209 corrective — value-level runtime verification for typed let-chain with Var RHS.
//!
//! Tests that sequential let bindings with type annotations and Var RHS correctly emit code
//! to copy source register to scratch register, and return the expected value.
//!
//! Calculation: f()
//!   x : u64 = 42
//!   y : u64 = x (copy x to scratch)
//!   return y = 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_chain_2_typed_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_chain_2_typed.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
