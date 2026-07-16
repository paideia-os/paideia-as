//! Issue #1225 — Nested BinOp arg1 scratch pool must respect pattern-binder
//! live registers. Fixture binds Choice::A(3) to pattern variable v (lands in R10
//! after RCX/R8 are allocated). Outer BinOp (v+1)+v would clobber R10 without the
//! live-reg filter, silently truncating to (3+1)+1=5. With filter, arg1_dest=R11,
//! preserving v in R10 to evaluate outer add correctly as (3+1)+3=7.
//! Poison RCX/R8/R9 to force pattern-binder allocation.
//!
//! Expected: 7

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn binop_arg1_scratch_respects_pattern_binder_returns_7() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "binop_arg1_scratch_respects_pattern_binder.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 7,
    };
    run_and_verify(&case);
}
