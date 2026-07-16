//! Issue #1220 — Pair-binding payload_reg must be saved across intervening CALL.
//! Fixture binds Result::Ok(42) to pair (RCX, RDX), then calls make_err which
//! materializes Err(99) — clobbering RDX with 99. Without #1220, the match
//! dispatch would read RDX=99 and return wrong value. Poison guarantees
//! the clobber would be detectable if RDX isn't pushed/popped around make_err.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn pair_binding_call_preserve_value_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "pair_binding_call_preserve.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
