//! Issue #1214 — three levels of nested matches with wildcard label resolution.
//!
//! Tests that deeply nested matches all with wildcard arms correctly
//! resolve labels at each cascade level independently.
//!
//! Expected: 7

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn nested_match_three_deep_returns_7() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "nested_match_three_deep.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 7,
    };
    run_and_verify(&case);
}
