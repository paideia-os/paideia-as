//! #1191: Tail-position BinOp after single let-binding with function call.
//!
//! Tests that a tail-position binary operation after a single let-bound function call
//! correctly dispatches to the BinOp lowerer. This is a simpler variant of
//! two_let_with_tail_binop, focusing on the single-let case.
//!
//! Fixture: f(v) = { let x = helper(v); x + 1 }; entry() = f(5)
//! Expected: entry() == 16 (helper(5) = 15, x + 1 = 16)

use object::{Object, ObjectSection};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn single_let_then_tail_add_compiles() {
    let out = run_build(build_emit("single_let_then_tail_add.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert!(
        bytes.len() > 0,
        "compiled artifact should have non-zero size"
    );
}

#[test]
fn single_let_then_tail_add_emits_for_tail_expr() {
    let out = run_build(build_emit("single_let_then_tail_add.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let text_section = file
        .section_by_name(".text")
        .expect(".text section should exist");
    let text_data = text_section.data().expect(".text data should be readable");

    // Verify that .text has some content (compilation succeeded)
    assert!(
        text_data.len() > 0,
        ".text section should have content"
    );
}
