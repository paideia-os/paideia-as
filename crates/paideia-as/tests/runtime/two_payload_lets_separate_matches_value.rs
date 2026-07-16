//! Issue #1215 — Pool-filter doesn't over-conservatively exhaust on nested matches.
//!
//! Tests two sequential payload lets (x, y) with nested match expressions where
//! the inner match in the arm body binds v2 (2). This tests that pool reuse works
//! correctly when v1 binding exits scope. Verifies that live_regs() correctly
//! reflects scope-exited bindings.
//!
//! Expected: 2 (from v2)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn two_payload_lets_separate_matches_returns_2() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "two_payload_lets_separate_matches.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 2,
    };
    run_and_verify(&case);
}
