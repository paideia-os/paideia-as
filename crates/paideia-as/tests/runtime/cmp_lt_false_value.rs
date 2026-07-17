//! Issue #1229 — runtime verification for comparison operator < (false case).
//!
//! Tests that comparison operator < correctly emits cmp/setcc/movzx sequence
//! and returns the expected value.
//!
//! Calculation: let x = 3u64 < 1u64; x -> false (0)

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn cmp_lt_false_returns_0() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "cmp_lt_false.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 0,
    };
    run_and_verify(&case);
}
