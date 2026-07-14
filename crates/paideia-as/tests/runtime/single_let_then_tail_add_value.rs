//! Issue #1191 — value-level runtime verification (single let).
//!
//! Tests that a tail-position BinOp after a single let-binding correctly
//! returns the expected value. This is a simpler variant of two_let_with_tail_binop,
//! focusing on the single-let case.

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn single_let_then_tail_add_returns_16() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    // helper(5)=15, tail x+1=16.
    let case = RuntimeCase {
        fixture_pdx: "single_let_then_tail_add.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 16,
    };
    run_and_verify(&case);
}
