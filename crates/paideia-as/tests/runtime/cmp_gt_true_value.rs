//! Issue #1229 — runtime verification for comparison operator >.
//!
//! Tests that comparison operator > correctly emits cmp/setcc/movzx sequence
//! and returns the expected value.
//!
//! Calculation: let x = 3u64 > 1u64; x -> true (1)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn cmp_gt_true_returns_1() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "cmp_gt_true.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 1,
    };
    run_and_verify(&case);
}
