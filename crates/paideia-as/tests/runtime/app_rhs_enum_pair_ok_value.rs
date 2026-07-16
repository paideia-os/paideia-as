//! Issue #1178 — App-RHS producer captures RDX payload into a scratch register
//! before the discriminant-to-scratch move clobbers it.
//!
//! Fixture returns 42 by reading the Ok payload. Poison guarantees the payload
//! must survive through the CALL boundary and into the match dispatch.
//!
//! Expected: 42

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn app_rhs_enum_pair_ok_returns_42() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "app_rhs_enum_pair_ok.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
