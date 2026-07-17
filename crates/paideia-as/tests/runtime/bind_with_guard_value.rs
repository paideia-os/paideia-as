//! Issue #1002 — runtime verification for bind-and-match with passing guard.
//!
//! Tests that bind-and-match works with guards that pass.
//!
//! Calculation: match 5u64 { x @ 5u64 if x > 3u64 => x, _ => 0u64 } -> 5

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn bind_with_guard() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "bind_with_guard.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 5,
    };
    run_and_verify(&case);
}
