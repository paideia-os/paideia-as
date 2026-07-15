//! Issue #1214 — simple 2-arm match.
//!
//! Tests a basic 2-arm match ensuring baseline behavior is unaffected
//! by the fix.
//!
//! Expected: 2

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn nested_match_same_var_returns_5() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "nested_match_same_var.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 2,
    };
    run_and_verify(&case);
}
