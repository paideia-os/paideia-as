//! Issue #1206 — bare enum literal in function-scope let, unit variant A.
//!
//! Tests that `let c : Choice = Choice::A` in function body emits the code correctly,
//! and the match dispatch returns the correct arm value.
//!
//! Expected: 1

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn func_scope_enum_let_a_returns_1() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "func_scope_enum_let_a.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 1,
    };
    run_and_verify(&case);
}
