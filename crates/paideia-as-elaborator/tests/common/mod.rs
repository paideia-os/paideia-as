//! Shared test harness for `paideia-as-elaborator` integration tests.
//!
//! Consolidated when 16 integration binaries were collapsed into 3 topical
//! binaries (see `.plans/test-restructure-2026-07-08.md`).
//!
//! Every helper here is `pub` so it can be reached from a sub-module of
//! whichever integration binary declared `mod common;` at its crate root.

use paideia_as_diagnostics::{FileId, Span};

/// Canonical dummy span used by tests that don't care about source position.
pub fn test_span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}
