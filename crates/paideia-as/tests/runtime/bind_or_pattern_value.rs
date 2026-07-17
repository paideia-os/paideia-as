//! Issue #1002 — runtime verification for bind-and-match with or-patterns.
//!
//! Tests that bind-and-match works with or-patterns.
//!
//! Calculation: match 1u64 { x @ 0u64 | 1u64 | 2u64 => x + 10u64, _ => 0u64 } -> 11

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn bind_or_pattern() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "bind_or_pattern.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 11,
    };
    run_and_verify(&case);
}
