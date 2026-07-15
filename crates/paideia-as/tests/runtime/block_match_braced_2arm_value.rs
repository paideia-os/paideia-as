//! Issue #1208 — smallest braced-block repro for match cascade jne offset bug.
//!
//! Tests that a 2-arm enum match inside braces works correctly.
//! The braced form triggers the Block wrapper, exposing the jne offset bug
//! that was present before the #1208 fix.
//!
//! Expected: 2 (Status::On)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn block_match_braced_2arm_returns_2() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "block_match_braced_2arm.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 2,
    };
    run_and_verify(&case);
}
