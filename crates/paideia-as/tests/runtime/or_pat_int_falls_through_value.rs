//! Issue #1001 — runtime verification for or-patterns with integers.
//!
//! Tests that or-pattern matching falls through to default arm when no alt matches.
//!
//! Calculation: match 2u64 { 0u64 | 1u64 => 10u64, _ => 20u64 } -> 20

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn or_pat_int_falls_through() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "or_pat_int_falls_through.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 20,
    };
    run_and_verify(&case);
}
