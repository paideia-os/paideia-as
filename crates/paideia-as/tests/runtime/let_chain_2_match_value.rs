//! Issue #1207 corrective — value-level runtime verification for let-chain with match RHS.
//!
//! Tests that a let binding with a match expression on the RHS correctly emits the match body
//! into RAX with ReturnRax tail, then materializes RAX into the scratch register,
//! and returns the expected value.
//!
//! Calculation: f()
//!   x = match 1 { 0 => 10, 1 => 20 } (match returns 20 in RAX, move to scratch)
//!   return x = 20

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn let_chain_2_match_returns_20() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "let_chain_2_match.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 20,
    };
    run_and_verify(&case);
}
