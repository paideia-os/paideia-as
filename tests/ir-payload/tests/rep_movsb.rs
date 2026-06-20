//! Fixture for REP MOVSB (bulk copy) instruction population.
//!
//! Phase-3-m3+: This test is deferred pending:
//! - AST → IR lowering of bulk-copy (memcpy) operations (currently m2-003 only recognizes Load/Store)
//! - IR node kind for REP MOVSB or similar string ops (e.g., IrKind::RepMovsb)
//! - Populate path that recognizes memcpy patterns or explicit REP intrinsic calls
//! - Intrinsic App recognition for the `rep_movsb` operation (m2-004)
//!
//! Once those components ship, this fixture will construct a synthetic bulk-copy node
//! and verify the mnemonic is RepMovsb with no operands (implicit rdi/rsi/rcx).

#[test]
#[ignore = "phase-3-m3+: populate path for REP MOVSB not yet recognised"]
fn rep_movsb_populates_as_rep_movsb_mnemonic() {
    // TODO: Construct synthetic IR bulk-copy node.
    // TODO: Run populate_instruction_table.
    // TODO: Assert mnemonic is Mnemonic::RepMovsb.
}

#[test]
#[ignore = "phase-3-m3+: populate path for REP MOVSB not yet recognised"]
fn rep_movsb_has_no_explicit_operands() {
    // TODO: Verify RepMovsb has empty operands list (implicit registers).
}
