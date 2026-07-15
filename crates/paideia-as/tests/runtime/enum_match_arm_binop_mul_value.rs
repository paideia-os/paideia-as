//! Issue #1196 deferred: enum-match arm-body fixture with payload variants and BinOp
//!
//! Tests that an enum match with payload variants correctly executes multiplication
//! in the arm body.
//! Pattern: pick(Tag::A(7u64, 6u64)) → A arm returns x * y = 42
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn enum_match_arm_binop_mul_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "enum_match_arm_binop_mul.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
