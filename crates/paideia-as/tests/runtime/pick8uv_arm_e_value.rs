//! Issue #1203 corrective 6 — value-level runtime verification for 8-arm enum-match arm E (unit variants).
//!
//! Tests that match expressions with 8 unit-variant enum patterns correctly dispatch
//! to the fifth arm and return the expected value.
//!
//! Expected: 50

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn pick8uv_arm_e_returns_50() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "pick8uv_arm_e.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 50,
    };
    run_and_verify(&case);
}
