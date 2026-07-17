//! Issue #1001 — runtime verification for or-patterns with enum payload variants.
//!
//! Tests that or-pattern matching with enum payload variants works correctly.
//!
//! Calculation: enum R { Ok(u64), Err(u64) }; scrut R::Ok(5u64); Ok(x) | Err(x) => x -> 5

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn or_pat_enum_payload_ok() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "or_pat_enum_payload_ok.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 5,
    };
    run_and_verify(&case);
}
