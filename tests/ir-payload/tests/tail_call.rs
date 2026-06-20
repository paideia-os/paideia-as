//! Fixture for tail call instruction population.
//!
//! Phase-3-m3+: This test is deferred pending:
//! - AST → IR lowering of tail calls (currently m2-003 only recognizes Load/Store)
//! - IR node kind for tail calls (e.g., IrKind::TailCall or a Call variant)
//! - Populate path that recognizes tail-call markers and synthesizes Jmp (not Call)
//! - Calling-convention analysis to determine that a call is safe to tail-call
//!
//! Once those components ship, this fixture will construct a synthetic tail call
//! node and verify the mnemonic is Jmp (not Call).

#[test]
#[ignore = "phase-3-m3+: populate path for tail calls not yet recognised"]
fn tail_call_populates_as_jmp() {
    // TODO: Construct synthetic IR tail call node.
    // TODO: Run populate_instruction_table.
    // TODO: Assert mnemonic is Jmp (not Call).
}

#[test]
#[ignore = "phase-3-m3+: populate path for tail calls not yet recognised"]
fn tail_call_has_correct_operand_structure() {
    // TODO: Verify operand structure for tail call (target address).
}
