//! Fixture for indexed accumulator (loop with load-add-store pattern) instruction population.
//!
//! Phase-3-m3+: This test is deferred pending:
//! - AST → IR lowering of loops / accumulator patterns (currently m2-003 only recognizes Load/Store)
//! - Whole-program alias analysis or loop-local escape analysis to prove accumulator safety
//! - IR node kind for accumulator operations or loop-annotated Load/Store chains
//! - Populate path that recognizes the pattern and synthesizes Add instructions
//!
//! Once those components ship, this fixture will construct a synthetic accumulator chain
//! (Load → Add → Store) and verify the instruction table captures the Add mnemonic
//! with the correct operand structure.

#[test]
#[ignore = "phase-3-m3+: populate path for indexed accumulators not yet recognised"]
fn indexed_accumulator_populates_with_add_instruction() {
    // TODO: Construct synthetic IR load-add-store chain.
    // TODO: Run populate_instruction_table.
    // TODO: Assert that the Add node is populated with Mnemonic::Add.
}

#[test]
#[ignore = "phase-3-m3+: populate path for indexed accumulators not yet recognised"]
fn indexed_accumulator_preserves_width_in_add_encoding() {
    // TODO: Verify that Add encoding hint reflects the load/store width.
}
