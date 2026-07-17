//! Issue #1002 — runtime verification for bind-and-match with enum unit variants.
//!
//! Tests that bind-and-match works with enum unit variants.
//!
//! Calculation: Color::A matched against `x @ A | B => 1u64, C => 0u64` -> 1

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn bind_variant_unit() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "bind_variant_unit.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 1,
    };
    run_and_verify(&case);
}
