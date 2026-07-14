//! Issue #1191 — value-level runtime verification.
//!
//! Byte-pattern assertions in build_emit verify that a tail-position BinOp emits
//! ADD, but do not confirm the actual returned VALUE is correct. This test links
//! the compiled fixture against a C driver, poisons caller-save registers before
//! the call, and checks that entry() == 32 is what actually comes back in RAX.

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn two_let_with_tail_binop_returns_32() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    // helper_a(1)=11, helper_b(1)=21, tail a+b=32.
    let case = RuntimeCase {
        fixture_pdx: "two_let_with_tail_binop.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 32,
    };
    run_and_verify(&case);
}
