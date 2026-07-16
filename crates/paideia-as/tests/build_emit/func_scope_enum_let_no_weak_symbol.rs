//! Issue #1212: Function-scope enum let bindings must not leak as WEAK OBJECT symbols
//!
//! This test verifies that the fix for #1212 correctly prevents statement-scope
//! (function-local) enum let bindings from appearing in the .symtab.
//!
//! Bug: `let c : Choice = Choice::A` at function scope was emitted as a WEAK OBJECT
//! symbol in the ELF symbol table. Root cause: NodeKind::StmtLet and NodeKind::Let
//! both lowered to IrKind::Let, and three producer sites (emit_walker, data_encoder,
//! cmd_build) treated all Let nodes as module-scope, inserting them into the symbol table.
//!
//! Fix: Track statement-scope Lets via a new `stmt_lets` HashSet in IrArena, then gate
//! symbol insertion and data-section emission at all three producer sites.
//!
//! Verification: binding `c` must not appear in .symtab (check `nm` output).

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Verify that func_scope_enum_let_a.pdx does not leak 'c' as a WEAK OBJECT symbol
#[test]
fn func_scope_enum_let_a_binding_absent_from_symtab() {
    let out = run_build(build_emit("func_scope_enum_let_a.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    assert!(
        !elf::has_symbol(&bytes, "c"),
        "function-scope let 'c' must not appear in .symtab (was leaking as WEAK OBJECT — #1212)"
    );
}
