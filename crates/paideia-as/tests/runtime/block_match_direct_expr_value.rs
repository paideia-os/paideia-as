//! Issue #1208 — baseline test for direct-expression match (no braces).
//!
//! Tests that a 3-arm enum match in direct tail position works correctly.
//! This form should work even before the #1208 fix. Serves as baseline
//! for comparison with the braced forms.
//!
//! Expected: 2 (State::Running)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn block_match_direct_expr_returns_2() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "block_match_direct_expr.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 2,
    };
    run_and_verify(&case);
}
