//! Phase 7 m4-007 (#1211): Function-scope EnumCons orphan byte assertion
//!
//! This test verifies that the fix for #1211 (order bug in emit_walker.rs)
//! correctly prevents orphan bytes before the entry symbol in function-scope
//! enum let bindings.
//!
//! Bug: flat walker's EnumCons arm (line 660) fired BEFORE Lambda arm (line 559),
//! causing orphan disc/payload movs to be emitted before entry point.
//!
//! Fix: Drop the `if is_data_let` guard on mark_enum_cons_handled in the
//! pre-pass at emit_walker.rs:335, ensuring function-scope lets are marked
//! BEFORE the flat walker reaches them.
//!
//! Verification: entry symbol must be at offset 0x0 in .text (no orphan bytes).

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Verify that func_scope_enum_let_a.pdx has entry symbol at offset 0x0
#[test]
fn func_scope_enum_let_a_entry_at_zero() {
    let out = run_build(build_emit("func_scope_enum_let_a.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    // Find entry symbol and verify its offset is 0x0
    if let Some((_section_idx, addr)) = elf::symbol_section_and_addr(&bytes, "entry") {
        assert_eq!(
            addr, 0,
            "entry symbol must be at offset 0x0 in .text (no orphan bytes before); got 0x{:x}",
            addr
        );
    } else {
        panic!("entry symbol not found in ELF file");
    }
}

/// Verify that payload_let_payload_a.pdx has entry symbol at offset 0x0
/// (amplified #1213 case with payload-carrying enum variants)
#[test]
fn payload_let_payload_a_entry_at_zero() {
    let out = run_build(build_emit("payload_let_payload_a.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    // Find entry symbol and verify its offset is 0x0
    if let Some((_section_idx, addr)) = elf::symbol_section_and_addr(&bytes, "entry") {
        assert_eq!(
            addr, 0,
            "entry symbol must be at offset 0x0 in .text (no orphan bytes before); got 0x{:x}",
            addr
        );
    } else {
        panic!("entry symbol not found in ELF file");
    }
}
