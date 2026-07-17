//! Issue #1001 — runtime verification for or-patterns with integers.
//!
//! Tests that or-pattern matching with integer literals works correctly (second alt matches).
//!
//! Calculation: match 1u64 { 0u64 | 1u64 => 10u64, _ => 20u64 } -> 10

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn or_pat_int_second() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "or_pat_int_second.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
