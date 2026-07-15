//! Issue #1213 — payload-carrying enum variant in function-scope let, variant C(42).
//!
//! Tests that `let c : Choice = Choice::C(42u64)` in function body emits the code correctly,
//! allocates paired scratch registers (discriminant + payload), and the match dispatch
//! extracts and returns the payload.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn payload_let_payload_c_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "payload_let_payload_c.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
