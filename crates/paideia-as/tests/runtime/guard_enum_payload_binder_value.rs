//! Issue #1000 — runtime verification for guard expressions in match arms.
//!
//! Tests that pattern binder is available in guard expression.
//!
//! Calculation: match Ok(5u64) { Ok(x) if x > 3u64 => x, _ => 0u64 } -> 5

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn guard_enum_payload_binder() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "guard_enum_payload_binder.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 5,
    };
    run_and_verify(&case);
}
