//! Issue #1215 — Nested match arithmetic with multiplication (variant of base case).
//!
//! Tests nested match where outer arm binds v1 (5), inner arm binds v2 (2).
//! Verifies register allocation under pool-filter: v1 gets a non-colliding scratch reg.
//! Multiplication instead of addition to distinguish from two_payload_lets_arith.
//! Confirms that live_regs() filter is working correctly for register assignment.
//!
//! Expected: 10 (5 * 2 from v1 * v2)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn nested_match_two_lets_payload_arith_returns_10() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "nested_match_two_lets_payload_arith.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
