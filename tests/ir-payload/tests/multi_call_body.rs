//! Fixture for multi-call function body instruction population.
//!
//! Phase-3-m3+: This test is deferred pending:
//! - AST → IR lowering of function calls (currently m2-003 only recognizes Load/Store)
//! - IR node kind for function calls (e.g., IrKind::Call or IrKind::App for intrinsics)
//! - Populate path that recognizes intrinsic App nodes and synthesizes Call instructions
//! - Intrinsic function definition and resolution (m2-004)
//! - Calling-convention analysis to set up argument registers and save/restore
//!
//! Once those components ship, this fixture will construct a synthetic function body
//! with multiple call sites and verify each Call instruction has the correct operands
//! and encoding hints.

#[test]
#[ignore = "phase-3-m3+: populate path for calls not yet recognised"]
fn multi_call_body_populates_call_instructions() {
    // TODO: Construct synthetic IR function body with multiple calls.
    // TODO: Run populate_instruction_table.
    // TODO: Assert each call site is populated with Mnemonic::Call.
}

#[test]
#[ignore = "phase-3-m3+: populate path for calls not yet recognised"]
fn multi_call_body_preserves_call_targets() {
    // TODO: Verify each Call instruction encodes the correct target address or register.
}
