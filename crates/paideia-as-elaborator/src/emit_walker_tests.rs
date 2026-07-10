//! Test module for `emit_walker`. Split into topic-focused submodules on 2026-07-08.
//!
//! Previously a single 5976-line file. The split preserves every test verbatim;
//! only the top-level `mod tests { … }` frame was broken into four topical
//! submodules for browse ergonomics. Each file duplicates its own set of `use`
//! statements plus the shared `span()` helper.
//!
//! The explicit `#[path]` attributes are load-bearing: the parent inclusion is
//! itself done via `#[cfg(test)] #[path = "emit_walker_tests.rs"] mod tests;`
//! in `emit_walker.rs`, so Rust's default child-module lookup would search
//! under `emit_walker/`, not `emit_walker_tests/`.

#[path = "emit_walker_tests/setup_and_data.rs"]
mod setup_and_data;

#[path = "emit_walker_tests/layouts_calls.rs"]
mod layouts_calls;

#[path = "emit_walker_tests/scratch_and_ops.rs"]
mod scratch_and_ops;

#[path = "emit_walker_tests/match_nested.rs"]
mod match_nested;

#[path = "emit_walker_tests/scope_guard.rs"]
mod scope_guard;
