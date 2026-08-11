//! #1290: 2-arg trait-method call at lambda-body position must not fire T0540.
//!
//! `fn (a, b) -> Foo::bar(a, b)` — the exact shape paideia-os per-CPU wrappers
//! (issue #767) need. Before the fix, the pre-emit `call_sites` populator
//! rejected `PerCpuOps::write_u64` as not a valid identifier, leaving
//! `arena.call_sites().get(body_id)` empty; visit_lambda's App arm then fell
//! through to the operator fallback and dispatched into
//! `emit_var_assign_expr_to_rax`, which fires T0540.
//!
//! Fix: `is_valid_qualified_identifier` recognises `Foo::bar`-shaped names so
//! qualified callees populate call_sites and route through emit_function_call
//! (which knows how to resolve `TraitName::method` via stdlib_lowering).

use crate::common::harness::run_build;
use crate::common::fixture;

#[test]
fn trait_call_lambda_body_no_t0540_1290() {
    let out = run_build(fixture::build_emit("trait_call_lambda_body.pdx"));
    // Before the fix: fails with T0540 ("var_assign accepts only Var-to-Var").
    // After the fix: build succeeds and no T0540 appears anywhere in stderr.
    assert!(
        !out.stderr.contains("T0540"),
        "T0540 must not fire on 2-arg trait-method call at lambda-body position (issue #1290). stderr:\n{}",
        out.stderr
    );
    // Also make sure no P0100 parse error slips in from a syntax typo.
    assert!(
        !out.stderr.contains("P0100"),
        "fixture must not have parse errors. stderr:\n{}",
        out.stderr
    );
    out.assert_ok();
}
