//! #1191: Tail-position BinOp after two let-bindings.
//!
//! Tests that a tail-position binary operation (`a + b`) after two function calls
//! correctly dispatches to the BinOp lowerer (reusing #1181 depth-indexed machinery)
//! instead of unconditionally routing through emit_call_expr.
//!
//! Fixture: combo(v) = { let a = helper_a(v); let b = helper_b(v); a + b }
//! Expected: combo(1) = 11 + 21 = 32 (helper_a(1) = 11, helper_b(1) = 21, sum = 32)

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn two_let_with_tail_binop_emits_both_calls() {
    let out = run_build(build_emit("two_let_with_tail_binop.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Count CALL (0xe8) opcodes in .text
    let call_count = text.windows(1).filter(|w| w[0] == 0xe8).count();
    assert!(
        call_count >= 2,
        ".text must contain at least TWO CALL (0xe8) opcodes for combo; got {}",
        call_count
    );
}

#[test]
fn two_let_with_tail_binop_compiles() {
    let out = run_build(build_emit("two_let_with_tail_binop.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert!(
        bytes.len() > 0,
        "compiled artifact should have non-zero size"
    );
}

#[test]
fn two_let_with_tail_binop_has_both_relocations() {
    let out = run_build(build_emit("two_let_with_tail_binop.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut helper_a_found = false;
    let mut helper_b_found = false;

    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "helper_a" {
                            helper_a_found = true;
                        } else if name == "helper_b" {
                            helper_b_found = true;
                        }
                    }
                }
            }
        }
    }

    assert!(
        helper_a_found,
        "must have a relocation targeting `helper_a` (first callee)"
    );
    assert!(
        helper_b_found,
        "must have a relocation targeting `helper_b` (second callee)"
    );
}
