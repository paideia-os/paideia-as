//! Issue #994 runtime canaries — closure type construction.
//!
//! `#995` (closure invocation / call-through-fat-pointer) has not landed
//! yet, so these canaries verify LAYOUT construction succeeds end-to-end
//! (elaborate → emit → link → run) and that constructing a closure doesn't
//! disturb the surrounding computation — not invocation, which isn't wired
//! yet. See `tests/build-emit/closure_type/*.pdx` for the fixtures.

mod closure_no_capture;
mod closure_value_capture;
mod closure_two_captures;
