//! Issue #994 runtime canaries — closure type construction.
//! Issue #995 runtime canaries — closure invocation / call-through-fat-pointer.
//!
//! These canaries verify closure construction + invocation end-to-end
//! (elaborate → emit → link → run). See `tests/build-emit/closure_type/*.pdx` for fixtures.

// Issue #994: Closure construction (fat-pointer layout)
mod closure_no_capture;
mod closure_value_capture;
mod closure_two_captures;

// Issue #995: Closure invocation
mod closure_call_zero_capture;
mod closure_call_single_capture;
mod closure_call_two_captures;
mod closure_call_two_args_zero_capture;
mod closure_call_multiarg_with_capture;
mod closure_call_after_intervening_direct_call;
