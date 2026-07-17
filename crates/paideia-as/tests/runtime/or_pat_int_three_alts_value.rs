//! Issue #1001 — runtime verification for or-patterns with multiple alternatives.
//!
//! Tests that or-pattern matching with multiple alternatives across arms works correctly.
//!
//! Calculation: match 3u64 { 0u64 | 1u64 | 2u64 => 1u64, 3u64 | 4u64 => 40u64, _ => 0u64 } -> 40

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn or_pat_int_three_alts() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "or_pat_int_three_alts.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 40,
    };
    run_and_verify(&case);
}
