//! #1193: Flat-lambda BinOp handler fix — runtime value verification
//!
//! Tests that runtime canaries produce correct output values via the shared
//! #1181 BinOp lowerer after the #1193 catch-all fix.
//!
//! Note: These tests exercise the (App-op, Var) and (App-op, Literal) shapes which
//! are handled by the #1193 catch-all. Deeper nesting and more complex patterns
//! are covered by the build_emit corpus which just verifies compilation without errors.

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn flat_lambda_binop_op_var_value() {
    // fn (a, b) -> (a >> 4) & b with args (0xFF0, 0xFF)
    // (0xFF0 >> 4) & 0xFF = 0xFF & 0xFF = 0xFF
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "flat_lambda_binop_op_var.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 0xFF,
    };
    run_and_verify(&case);
}

#[test]
fn flat_lambda_binop_op_lit_value() {
    // fn (a) -> (a >> 4) & 0xFF with arg 0xFF0
    // (0xFF0 >> 4) & 0xFF = 0xFF & 0xFF = 0xFF
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "flat_lambda_binop_op_lit.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 0xFF,
    };
    run_and_verify(&case);
}
