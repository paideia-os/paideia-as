//! Issue #1000 — runtime verification for guard expressions in match arms.
//!
//! Tests that when first guard fails, second guarded arm is checked.
//!
//! Calculation: match 2u64 { N if N > 5u64 => 100u64, N if N > 1u64 => 20u64, _ => 0u64 } -> 20

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn guard_two_arms_first_fails() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "guard_two_arms.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 20,
    };
    run_and_verify(&case);
}
