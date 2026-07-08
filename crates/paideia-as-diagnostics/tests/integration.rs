//! Consolidated integration-test entry for `paideia-as-diagnostics`.
//!
//! Each `mod` below pulls in a topical test module from a sibling
//! `tests/<topic>.rs` file. Cargo builds a single integration binary
//! (`integration`) instead of one binary per file, which cuts link overhead
//! on the workspace-wide test cycle. See
//! `.plans/test-restructure-2026-07-08.md` for the rationale.
//!
//! Behavior-preserving: each leaf test keeps its original name and source
//! location, so `insta` snapshots continue to resolve to their existing
//! paths under `tests/snapshots/`.

mod m0307_retired;
mod opt_codes_present;
mod proptest_codes;
mod render_human;
mod sarif;
mod sink;
