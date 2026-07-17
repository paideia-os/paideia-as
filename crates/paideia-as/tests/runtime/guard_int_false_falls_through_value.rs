//! Issue #1000 — runtime verification for guard expressions in match arms.
//!
//! Tests that guard condition false falls through to next arm.
//!
//! Calculation: match 2u64 { N if N > 3u64 => 1u64, _ => 0u64 } -> 0

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn guard_int_false_falls_through() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "guard_int_false.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 0,
    };
    run_and_verify(&case);
}
