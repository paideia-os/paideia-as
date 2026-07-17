//! Issue #1001 — runtime verification for or-patterns with enum unit variants.
//!
//! Tests that or-pattern matching with enum unit variants works correctly (second alt matches).
//!
//! Calculation: enum Choice { A, B, C }; scrut Choice::B; A | B => 1, C => 2 -> 1

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn or_pat_enum_unit_second() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "or_pat_enum_unit_second.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 1,
    };
    run_and_verify(&case);
}
