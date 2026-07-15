//! Issue #1206 — function-scope enum let with shadow guard.
//!
//! Tests that a local binding named `North : u64 = 42` shadows the enum variant `Direction::North`.
//! The second let-RHS should read the binding value (42), not be rewritten to EnumCons.
//! Shadow check MUST prevent rewrite when bare name collides with local binding.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn func_scope_enum_let_shadow_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "func_scope_enum_let_shadow.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
