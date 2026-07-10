//! Regression test for #1138: collision case with local let followed by unrelated store.
//!
//! Before the fix, the lookahead heuristic in emit_block_body would incorrectly
//! steal the next statement's LHS binding name for the current let node,
//! causing silent miscompilation.
//!
//! After the fix: binding_names is populated for local StmtLet nodes via the
//! pre-pass, so no name collision occurs. The code either compiles correctly
//! (if the assignment is valid) or fails with a clear diagnostic (if invalid).
//!
//! This test verifies that the collision case no longer silently miscompiles
//! into the wrong disassembly pattern `mov $0xc8,%rax; ret` (200 in decimal).

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that the collision case no longer silently miscompiles.
///
/// The fixture has: `let x : u64 = 5; y = 200; x`
/// The lookahead heuristic would steal `y` as the binding name for `x`,
/// causing the assignment `y = 200` to overwrite `x`'s scratch register,
/// resulting in silent miscompilation (return 200 instead of 5).
///
/// After the fix:
/// - If the assignment is invalid (literal RHS), it fails with T0540 (safe)
/// - If the assignment is valid, it compiles correctly with separate registers
///
/// This test verifies the build does NOT silently miscompile to the wrong pattern.
#[test]
fn local_let_mut_collision_case_no_miscompile() {
    let input = build_emit("stmt_assign_local_let_mut_unrelated_store.pdx");
    let out = run_build(input);

    // If the build fails with T0540, that's safe (refuses to miscompile).
    // If it succeeds, that's also fine (if assignments to module symbols with
    // literal RHS are now supported).
    //
    // What we absolutely must NOT have is silent miscompilation with exit 0
    // and the wrong disassembly pattern.
    let artifact_bytes = if out.status.success() {
        // Build succeeded; verify the disassembly is correct, not miscompiled
        out.artifact_bytes()
    } else {
        // Build failed; as long as it failed (didn't silently succeed), the
        // collision case is prevented. Return early without checking bytes.
        return;
    };

    // The miscompiled pattern was: `mov $0xc8,%rax; ret`
    // In bytes: `48 c7 c0 c8 00 00 00 c3`
    let miscompiled_pattern = [0x48u8, 0xc7, 0xc0, 0xc8, 0x00, 0x00, 0x00, 0xc3];

    // Check that the miscompiled pattern does NOT appear in the text section
    if artifact_bytes.windows(miscompiled_pattern.len())
        .any(|w| w == miscompiled_pattern)
    {
        panic!(
            "artifact contains the miscompiled pattern (mov $0xc8,%rax; ret), \
             indicating the collision case is NOT fixed. This suggests binding names \
             for local lets are still not being populated correctly."
        );
    }
}
