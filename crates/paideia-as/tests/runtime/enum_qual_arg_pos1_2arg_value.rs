//! Issue #1202 — qualified-path enum variant at arg position 1 of 2-arg call
//!
//! Tests that Tag::A at arg position 1 (rsi) is correctly marshalled.
//! The match arm returns the first argument (7).
//!
//! Expected: 7

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn enum_qual_arg_pos1_2arg_returns_7() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "enum_qual_arg_pos1_2arg.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 7,
    };
    run_and_verify(&case);
}
