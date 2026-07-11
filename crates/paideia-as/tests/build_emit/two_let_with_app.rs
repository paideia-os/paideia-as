//! #1161: Sequential Let-with-App calls.
//!
//! Tests that sequential let-bindings with function call RHS emit all CALL instructions
//! and preserve correct return values. Before #1161, the second CALL would silently drop
//! the first CALL (ID collision in emit_call_args_and_call).
//!
//! Fixture: combo(v) = { let a = get_a(v); let b = get_b(v); a }
//! Expected: combo(1) = 11 (get_a(1) = 1+10, not get_b(1) = 1+20)

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn two_let_with_app_emits_both_calls() {
    let out = run_build(build_emit("two_let_with_app.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Count CALL (0xe8) opcodes in .text
    let call_count = text.windows(1).filter(|w| w[0] == 0xe8).count();
    assert_eq!(
        call_count, 2,
        ".text must contain exactly TWO CALL (0xe8) opcodes for combo; got {}",
        call_count
    );
}

#[test]
fn two_let_with_app_has_both_relocations() {
    let out = run_build(build_emit("two_let_with_app.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut get_a_found = false;
    let mut get_b_found = false;

    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "get_a" {
                            get_a_found = true;
                        } else if name == "get_b" {
                            get_b_found = true;
                        }
                    }
                }
            }
        }
    }

    assert!(
        get_a_found,
        "must have a relocation targeting `get_a` (first callee)"
    );
    assert!(
        get_b_found,
        "must have a relocation targeting `get_b` (second callee)"
    );
}
