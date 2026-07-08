//! Shared test harness for `paideia-as-elaborator` integration tests.
//!
//! Consolidated when 16 integration binaries were collapsed into 3 topical
//! binaries (see `.plans/test-restructure-2026-07-08.md`).
//!
//! Every helper here is `pub` so it can be reached from a sub-module of
//! whichever integration binary declared `mod common;` at its crate root.

use paideia_as_diagnostics::{FileId, Span, SourceMap, VecSink};

/// Canonical dummy span used by tests that don't care about source position.
pub fn test_span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Creates a test SourceMap and VecSink seeded with a dummy test.pdx file.
///
/// This ensures that FileId(1) resolves properly in tests that need to lower
/// AST to IR. Returns (SourceMap, VecSink) with the SourceMap pre-populated
/// so that diagnostic sinks can safely reference FileId::new(1).
pub fn create_test_source_map_and_sink() -> (SourceMap, VecSink) {
    let mut source_map = SourceMap::new();
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("// test source"),
    );
    let sink = VecSink::new();
    (source_map, sink)
}
