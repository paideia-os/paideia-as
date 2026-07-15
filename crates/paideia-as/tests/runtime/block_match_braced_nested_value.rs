//! Issue #1208 — nested braced blocks with match.
//!
//! Tests that the recursive pre-pass correctly descends through multiple
//! nested Block wrappers to find and mark all Match nodes. This ensures
//! the fix doesn't break on deeper nesting patterns.
//!
//! Expected: 3 (Level::High, innermost arm)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn block_match_braced_nested_returns_3() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "block_match_braced_nested.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 3,
    };
    run_and_verify(&case);
}
