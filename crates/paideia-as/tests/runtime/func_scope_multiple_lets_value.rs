//! Issue #1206 — multiple function-scope enum lets.
//!
//! Tests that sequential `let` bindings of enum types work correctly
//! and that match dispatch operates on the correct binding.
//!
//! Expected: 3 (c2 is Choice::C, match returns 3)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn func_scope_multiple_lets_returns_3() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "func_scope_multiple_lets.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 3,
    };
    run_and_verify(&case);
}
