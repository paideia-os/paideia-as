//! Issue #1210 verification 4 — integer-match with literal scrutinee `2u64`
//! that falls through to the default arm (misses `0u64` and `1u64`).
//!
//! Expected: 30

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn intmatch_lit_falls_through_returns_30() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "intmatch_lit_falls_through.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 30,
    };
    run_and_verify(&case);
}
