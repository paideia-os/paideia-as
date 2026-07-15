//! Issue #1209 corrective — value-level runtime verification for let-chain with call RHS.
//!
//! Tests that sequential let bindings where the second RHS is a function call correctly
//! emit code to materialize RAX into scratch register and return the expected value.
//!
//! Calculation: f()
//!   x = 42
//!   y = identity(x) (call result from RAX to scratch)
//!   return y = 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_chain_2_call_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_chain_2_call.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
