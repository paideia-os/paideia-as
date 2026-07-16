//! Issue #1156 receiver-side. Driver poisons RAX=0 (Ok disc), RDX=42 (payload)
//! before `call f`. Prologue's insert_pair(e, RAX, RDX) then feeds
//! match e { Ok(x) => x } which returns 42.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn fn_param_enum_scrutinee_returns_42() {
    let poison = [
        Poison { reg: Reg::Rax, value: 0 },     // Ok disc
        Poison { reg: Reg::Rdx, value: 42 },    // payload
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::R8,  value: 0xDEADBEEF02 },
        Poison { reg: Reg::R9,  value: 0xDEADBEEF03 },
    ];
    run_and_verify(&RuntimeCase {
        fixture_pdx: "fn_param_enum_scrutinee.pdx",
        entry: "f",  // enter f directly, NOT a wrapper — driver poison IS the caller
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    });
}
