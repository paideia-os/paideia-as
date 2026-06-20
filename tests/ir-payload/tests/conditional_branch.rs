//! Fixture for conditional branch instruction population.
//!
//! Phase-3-m3+: This test is deferred pending:
//! - AST → IR lowering of conditional branches (currently m2-003 only recognizes Load/Store)
//! - IR node kind for conditional branches (e.g., IrKind::Jcc or similar)
//! - Populate path that synthesizes Jcc(Cond::Eq) / Jcc(Cond::Ne) / etc. from branch metadata
//!
//! Once those components ship, this fixture will construct a synthetic conditional
//! branch node and verify the mnemonic is Jcc with the correct condition code embedded.

#[test]
#[ignore = "phase-3-m3+: populate path for conditional branches not yet recognised"]
fn conditional_branch_populates_as_jcc() {
    // TODO: Construct synthetic IR conditional branch node.
    // TODO: Run populate_instruction_table.
    // TODO: Assert mnemonic is Jcc with the correct condition code.
}

#[test]
#[ignore = "phase-3-m3+: populate path for conditional branches not yet recognised"]
fn conditional_branch_eq_vs_ne_differs_condition_code() {
    // TODO: Test that a branch on equality vs inequality produces different Cond variants.
}
