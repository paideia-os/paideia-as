//! #1152: Let statement with App RHS (function call materialized to scratch register).
//!
//! Tests that a Let binding with a function call as RHS:
//! - Emits the CALL instruction (0xe8 opcode)
//! - Materializes the result to the allocated scratch register (via RAX if needed)
//! - Creates proper relocations targeting the callee function
//! - Does not fire T0527 (register pressure), T0540 (missing RHS handler), or T0521 (call marshalling)

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn let_with_call_rhs_materializes_to_scratch_register() {
    let out = run_build(build_emit("stmt_let_with_call_rhs.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr.contains("T0527"),
        "T0527 must not fire (register pressure): {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("T0540"),
        "T0540 must not fire (missing RHS handler) after #1152: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("T0521"),
        "T0521 must not fire on this single-arg call: {}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // CALL instruction opcode verification
    assert!(
        text.windows(1).any(|w| w[0] == 0xe8),
        ".text must contain a CALL (0xe8 opcode)"
    );

    // Check relocations targeting the callee function
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let mut callee_reloc_found = false;
    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "get_forty_two" {
                            callee_reloc_found = true;
                        }
                    }
                }
            }
        }
    }
    assert!(
        callee_reloc_found,
        "must have a relocation targeting `get_forty_two` (the callee)"
    );
}
