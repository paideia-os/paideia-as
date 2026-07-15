//! Issue #1204 — bare enum literal in module-let binding, unit variant A.
//!
//! Tests that `let c : Choice = Choice::A` emits the data symbol correctly,
//! and the match dispatch returns the correct arm value.
//!
//! Expected: 1

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn unit_var_module_let_matches_returns_1() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "unit_var_module_let_matches.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 1,
    };
    run_and_verify(&case);
}
