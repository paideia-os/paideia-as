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

    // paideia-as#1276 phase 3: the body pattern is bracketed by prologue at
    // the function head and epilogue right before `ret`. Because this fixture
    // also has an initial `let mut x = 0` before the reassignment, the emitted
    // stream is:
    //   push rbp; mov rbp, rsp        (55 48 89 E5)              — prologue
    //   mov $0x0, %rcx                (48 c7 c1 00 00 00 00)     — let mut x = 0
    //   mov $0x1, %rcx                (48 c7 c1 01 00 00 00)     — x = 1
    //   mov %rcx, %rax                (48 89 c8)                 — tail Var move
    //   mov %rsp, %rbp; pop %rbp; ret (48 89 EC 5D C3)           — epilogue + ret
    //
    // The witness looks for the reassignment tail (mov $0x1,%rcx onwards + the
    // epilogue+ret trailer) via a substring search; the leading prologue and
    // initial `let mut x = 0` mov exist earlier in .text but are not part of
    // the pattern we're guarding here.
    let expected_pattern = [
        0x48u8, 0xc7, 0xc1, 0x01, 0x00, 0x00, 0x00, // mov $0x1, %rcx
        0x48, 0x89, 0xc8,                            // mov %rcx, %rax
        0x48, 0x89, 0xEC, 0x5D, 0xC3,                // mov rsp,rbp; pop rbp; ret
    ];

    let bytes = out.artifact_bytes();
    assert!(
        bytes.windows(expected_pattern.len()).any(|w| w == expected_pattern),
        "expected to find pattern (mov $0x1,%rcx; mov %rcx,%rax; epilogue; ret) in .text section, got {} bytes",
        bytes.len()
    );
}
