//! Test for #1185: nested field access in read context.
//!
//! Nested field access (a.b.c) is not yet supported. Lower's populate_field_access_info
//! should reject this case and fire T0541 before the silent-miscompile hazard
//! (wrong flat symbol name in module_field_refs) occurs.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn nested_read_fires_t0541() {
    let input = build_emit("nested_module_qualification_read.pdx");
    let out = run_build(input);
    out.assert_diag("T0541");
}
