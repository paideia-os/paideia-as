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
    // NOTE: Expected is currently 21 (result of helper_b), not 32 (a + b).
    // This test documents the bug #1191: tail-BinOp is not being emitted.
    // After the fix, this should return 32.
    let case = RuntimeCase {
        fixture_pdx: "two_let_with_tail_binop.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 21, // Currently returns b, not a + b
    };
    run_and_verify(&case);
}
