//! Issue #1208 — 3-arm braced match dispatching on MIDDLE arm.
//!
//! Tests that a 3-arm enum match inside braces correctly computes jne offsets
//! for the middle arm. This tests offset accumulation more rigorously than
//! the 2-arm case, as the offset calculations must be exact for each jne.
//!
//! Expected: 2 (Mode::Write, middle arm)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn block_match_braced_3arm_returns_2() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "block_match_braced_3arm.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 2,
    };
    run_and_verify(&case);
}
