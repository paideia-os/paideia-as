//! Issue #1224 — bare-tail enum-constructor lambda body `fn(x: T) -> Enum::Variant(x)`.
//!
//! Tests that a lambda returning an enum constructor result is properly wrapped
//! with function entry/exit (record_lambda_entry and ret), not left stranded
//! outside the symbol range causing SIGSEGV on control fall-through.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn bare_enum_cons_lambda_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "bare_enum_cons_lambda.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
