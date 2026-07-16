//! Issue #1203 corrective 4 — value-level runtime verification for 4-arm enum-match arm D (unit variants).
//!
//! Tests that match expressions with 4 unit-variant enum patterns correctly dispatch
//! to the fourth arm and return the expected value.
//!
//! Expected: 40

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn pick4uv_arm_d_returns_40() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "pick4uv_arm_d.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 40,
    };
    run_and_verify(&case);
}
