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
    out.assert_ok();

    // Expected disassembly: `mov $0x1,%rax; ret`
    // In bytes: `48 c7 c0 01 00 00 00 c3`
    // - 48: REX.W prefix
    // - c7 c0: mov r64, imm32 (RAX destination)
    // - 01 00 00 00: immediate value 1 (little-endian)
    // - c3: ret
    let expected_pattern = [0x48u8, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, 0xc3];

    let bytes = out.artifact_bytes();
    assert!(
        bytes.windows(expected_pattern.len()).any(|w| w == expected_pattern),
        "expected to find pattern (mov $0x1,%rax; ret) in .text section, got {} bytes",
        bytes.len()
    );
}
