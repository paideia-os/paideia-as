//! Test for #1185: nested field access in write (assignment) context.
//!
//! Nested field access (a.b.c) is not yet supported. The classifier in
//! store_lvalue.rs::is_lvalue_infix_assignment rejects nested-FA LHS,
//! causing the Store node to never be built and the assignment to silently
//! drop through emit_block_body's App arm. This test verifies T0541 fires
//! cleanly instead of silent miscompilation.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn nested_write_fires_t0541() {
    let input = build_emit("nested_module_qualification_write.pdx");
    let out = run_build(input);
    out.assert_diag("T0541");
}
