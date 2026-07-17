//! Issue #1002 — runtime verification for bind-and-match with failing guard.
//!
//! Tests that bind-and-match falls through when guard fails.
//!
//! Calculation: match 5u64 { x @ 5u64 if x > 10u64 => x, _ => 42u64 } -> 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn bind_with_guard_false() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "bind_with_guard_false.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
