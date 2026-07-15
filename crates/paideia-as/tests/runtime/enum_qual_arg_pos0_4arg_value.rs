//! Issue #1202 — qualified-path enum variant at arg position 0 of 4-arg call
//!
//! Tests that Tag::A at arg position 0 is correctly marshalled to rdi in a 4-arg call.
//! The match arm computes (7 * 6) / 1 = 42.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn enum_qual_arg_pos0_4arg_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "enum_qual_arg_pos0_4arg.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
