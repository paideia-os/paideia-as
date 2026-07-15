//! Issue #1213 — payload-carrying enum variant in function-scope let, variant B(99).
//!
//! Tests payload extraction and pass-through in match arm binding.
//! Verifies that the payload value (99) is correctly loaded into payload_reg
//! and returned from the match arm.
//!
//! Expected: 99

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn payload_let_b_pass_thru_returns_99() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "payload_let_b_pass_thru.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 99,
    };
    run_and_verify(&case);
}
