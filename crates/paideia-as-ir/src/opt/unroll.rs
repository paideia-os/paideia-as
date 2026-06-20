//! Loop unrolling with explicit unroll factor.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;

/// The loop unrolling optimization pass.
pub struct UnrollPass;

/// Trip count for a loop: known constant or symbolic.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TripCount {
    /// Loop has a known constant trip count.
    Known(u32),
    /// Loop has a symbolic or unknown trip count.
    Unknown,
}

/// Whether an unroll factor `n` is safe for a given trip count.
///
/// Known(t): safe iff t % n == 0 AND n <= t. If t % n != 0, a remainder
/// loop is needed (phase-2-m9-009 doesn't emit it — returns false).
/// Unknown: safe only with a runtime remainder dispatch (phase-2 minimum:
/// emit a warning, don't unroll).
pub fn is_unroll_safe(trip: TripCount, factor: u32) -> bool {
    assert!(factor > 0, "unroll factor must be positive");
    match trip {
        TripCount::Known(t) => factor <= t && t % factor == 0,
        TripCount::Unknown => false,
    }
}

impl OptPass for UnrollPass {
    fn name(&self) -> &'static str {
        "unroll"
    }

    fn apply(&self, _arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        sink.emit(
            "unroll",
            "O1511 (would-fire): loop unrolling dispatched".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_unroll_safe_returns_true_for_divisible_known() {
        let trip = TripCount::Known(16);
        let factor = 4;
        assert!(is_unroll_safe(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_false_for_non_divisible_known() {
        let trip = TripCount::Known(10);
        let factor = 3;
        assert!(!is_unroll_safe(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_false_for_unknown() {
        let trip = TripCount::Unknown;
        let factor = 4;
        assert!(!is_unroll_safe(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_false_when_factor_exceeds_trip() {
        let trip = TripCount::Known(3);
        let factor = 5;
        assert!(!is_unroll_safe(trip, factor));
    }

    #[test]
    fn unroll_pass_emits_o1511() {
        let pass = UnrollPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "UnrollPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "unroll");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1511 (would-fire): loop unrolling dispatched")
        );
    }
}
