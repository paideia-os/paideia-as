//! Issue #1199 corrective 1 — value-level runtime verification for 3-arm enum-match arm A.
//!
//! Tests that match expressions with 3 unit-variant enum patterns correctly dispatch
//! to the first arm and return the expected value.
//!
//! Expected: 10

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn pick3_arm_a_returns_10() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "pick3_arm_a.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
