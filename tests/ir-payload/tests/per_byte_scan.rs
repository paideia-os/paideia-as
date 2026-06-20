//! Fixture for per-byte scan (string search) instruction population.
//!
//! Phase-3-m3+: This test is deferred pending:
//! - AST → IR lowering of string-search patterns (currently m2-003 only recognizes Load/Store)
//! - IR node kind for per-byte scan or string-search ops (e.g., IrKind::Scasb)
//! - Populate path that recognizes scan patterns or explicit scan intrinsic calls
//! - Intrinsic App recognition for the scan operation (m2-004)
//!
//! Once those components ship, this fixture will construct a synthetic per-byte scan node
//! and verify the mnemonic encodes the comparison operation (e.g., Cmp or dedicated Scas).

#[test]
#[ignore = "phase-3-m3+: populate path for per-byte scan not yet recognised"]
fn per_byte_scan_populates_with_comparison_instruction() {
    // TODO: Construct synthetic IR per-byte scan node.
    // TODO: Run populate_instruction_table.
    // TODO: Assert mnemonic is Cmp or a scan-specific variant.
}

#[test]
#[ignore = "phase-3-m3+: populate path for per-byte scan not yet recognised"]
fn per_byte_scan_reflects_search_byte_width() {
    // TODO: Verify operand size matches the byte width being scanned.
}
