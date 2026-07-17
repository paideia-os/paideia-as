//! Issue #1000 — runtime verification for guard expressions in match arms.
//!
//! Tests that pattern binder in guard is NOT leaked to arm body of next arm.
//!
//! Calculation: match Ok(1u64) { Ok(x) if x > 5u64 => x, _ => 42u64 } -> 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn guard_pattern_binder_not_leaked() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "guard_pattern_binder_not_leaked.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
