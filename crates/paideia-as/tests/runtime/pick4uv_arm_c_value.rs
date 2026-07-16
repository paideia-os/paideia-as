//! Issue #1203 corrective 3 — value-level runtime verification for 4-arm enum-match arm C (unit variants).
//!
//! Tests that match expressions with 4 unit-variant enum patterns correctly dispatch
//! to the third arm and return the expected value.
//!
//! Expected: 30

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn pick4uv_arm_c_returns_30() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "pick4uv_arm_c.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 30,
    };
    run_and_verify(&case);
}
