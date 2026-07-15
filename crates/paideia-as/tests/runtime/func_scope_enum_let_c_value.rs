//! Issue #1206 — bare enum literal in function-scope let, unit variant C.
//!
//! Tests that `let c : Choice = Choice::C` in function body emits the code correctly,
//! and the match dispatch returns the correct arm value.
//!
//! Expected: 3

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn func_scope_enum_let_c_returns_3() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "func_scope_enum_let_c.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 3,
    };
    run_and_verify(&case);
}
