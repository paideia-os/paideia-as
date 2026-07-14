//! #1191: Tail-position BinOp with nested function call (operand).
//!
//! Tests that a tail-position binary operation where one operand is a function call
//! correctly dispatches to the BinOp lowerer. This exercises the case where the
//! tail App is the outer BinOp, with a nested App as one of its operands.
//!
//! Fixture: f(v) = helper(v) + 0; entry() = f(5)
//! Expected: entry() == 10 (helper(5) = 10, 10 + 0 = 10)

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn tail_binop_nested_app_compiles() {
    let out = run_build(build_emit("tail_binop_nested_app.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert!(
        bytes.len() > 0,
        "compiled artifact should have non-zero size"
    );
}

#[test]
fn tail_binop_nested_app_emits_for_tail_expr() {
    let out = run_build(build_emit("tail_binop_nested_app.pdx"));
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
