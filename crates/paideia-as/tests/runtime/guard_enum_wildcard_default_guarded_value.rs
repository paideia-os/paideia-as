//! Issue #1000 — runtime verification for guard expressions in match arms.
//!
//! Tests that guarded wildcard doesn't match (guard false), falls through to unguarded default.
//!
//! Calculation: match Err(0u64) { _ if false_flag => 99u64, _ => 7u64 } -> 7

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn guard_enum_wildcard_default_guarded() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "guard_enum_wildcard_default.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 7,
    };
    run_and_verify(&case);
}
