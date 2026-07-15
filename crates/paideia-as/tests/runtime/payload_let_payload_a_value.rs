//! Issue #1213 — payload-carrying enum variant in function-scope let, variant A(0).
//!
//! Tests payload enum with multiple arms (unit B, payload A and C), verifies correct
//! discriminant assignment and match dispatch.
//!
//! Expected: 10

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn payload_let_payload_a_returns_10() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "payload_let_payload_a.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 10,
    };
    run_and_verify(&case);
}
