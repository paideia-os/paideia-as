//! Issue #1215 — Pattern-binder pool collision with paired-binding registers (base case).
//!
//! Tests nested match arms where first let binds enum with payload to x (RCX+RDX),
//! second let binds enum with payload to y (R8+R9). Nested match: inner match's
//! v1 binder must not collide with outer x's discriminant (RCX).
//! Verifies that live_regs() filters exclude already-live pair-binding registers.
//!
//! Expected: 7 (5 + 2 from v1 + v2 arithmetic)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn two_payload_lets_arith_returns_7() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "two_payload_lets_arith.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 7,
    };
    run_and_verify(&case);
}
