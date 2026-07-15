//! Issue #1202 — qualified-path enum variant at arg position 0 of 3-arg call — variant B
//!
//! Tests that Tag::B at arg position 0 is correctly marshalled to rdi.
//! The match arm returns 0 for variant B.
//!
//! Expected: 0

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn enum_qual_arg_pos0_3arg_b_returns_0() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "enum_qual_arg_pos0_3arg_b.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 0,
    };
    run_and_verify(&case);
}
