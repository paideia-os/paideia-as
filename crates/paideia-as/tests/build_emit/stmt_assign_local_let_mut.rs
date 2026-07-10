//! Test for #1138: local let mut assignment via register rewrite.
//!
//! Tests: `fn () -> { let mut x : u64 = 0; x = 1; x }`
//!
//! #1138 fixes the gap: emit_block_body's Let arm allocates a scratch register for x
//! and records x → scratch_reg in local_bindings. visit_var_assign now checks
//! local_bindings before the module-symbol gate, so `x = 1` emits `mov scratch_reg, 1`.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that local let mut assignment compiles successfully.
///
/// #1138 implements register-rewrite lowering: the assignment x = 1 is rewritten to
/// use the scratch register allocated to x in the Let arm, emitting `mov RAX, 1`.
#[test]
fn local_let_mut_assignment_compiles() {
    let input = build_emit("stmt_assign_local_let_mut.pdx");
    let out = run_build(input);
    // Filter out debug output (lines starting with '[') and check for actual errors
    let errors: Vec<&str> = out.stderr.lines()
        .filter(|line| !line.starts_with('[') && line.contains("error["))
        .collect();
    assert!(errors.is_empty(), "expected no error diagnostics, got:\n{:?}", errors);
    // Artifact should exist and have non-zero size (contains the Mov instructions)
    let bytes = out.artifact_bytes();
    assert!(!bytes.is_empty(), "expected non-empty artifact (ELF object file)");
}
