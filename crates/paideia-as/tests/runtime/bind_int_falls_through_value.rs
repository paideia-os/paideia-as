//! Issue #1002 — runtime verification for bind-and-match with falling through.
//!
//! Tests that bind-and-match falls through to default when pattern doesn't match.
//!
//! Calculation: match 7u64 { x @ 5u64 => x, _ => 99u64 } -> 99

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn bind_int_falls_through() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "bind_int_falls_through.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 99,
    };
    run_and_verify(&case);
}
