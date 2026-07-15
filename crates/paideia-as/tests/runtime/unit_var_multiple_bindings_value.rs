//! Issue #1204 — multiple bare enum literals in module-let bindings.
//!
//! Tests that multiple module-lets with bare enum variants each emit
//! their own correctly-valued, independently-addressed data symbol.
//!
//! c1 = Choice::A and c2 = Choice::B are both module-let bindings; c1
//! is defined first and must still build with its own data symbol, but
//! `entry` matches on c2 — the SECOND binding in program order — to
//! prove that the second `IrKind::Let` rewrite in the same pass isn't
//! aliased/overwritten by the first and reads back its own correct
//! discriminant.
//!
//! c2 = Choice::B => match arm B => 2u64
//!
//! Expected: 2

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn unit_var_multiple_bindings_returns_2() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "unit_var_multiple_bindings.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 2,
    };
    run_and_verify(&case);
}
