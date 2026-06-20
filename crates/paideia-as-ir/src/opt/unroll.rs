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

/// Phase-3-m2-004: loop-unroll safety checker using InstructionSideTable.
///
/// Takes an instruction side-table, a loop node ID, and an unroll factor;
/// returns whether the unroll is safe (true) or requires a remainder loop (false).
pub fn is_unroll_safe(
    _side_table: &crate::instruction::InstructionSideTable,
    _loop_id: crate::node::IrNodeId,
    factor: u32,
) -> bool {
    assert!(factor > 0, "unroll factor must be positive");
    // Phase-3-m2-004: TODO extract trip count from the side-table.
    // Placeholder: never unroll (too conservative).
    false
}

/// Internal implementation: unroll safety check on explicit trip count.
///
/// Takes trip count and unroll factor; returns whether the unroll is safe.
/// Helper logic preserved from phase-2-m9-009.
#[doc(hidden)]
pub fn is_unroll_safe_impl(trip: TripCount, factor: u32) -> bool {
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
        assert!(is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_false_for_non_divisible_known() {
        let trip = TripCount::Known(10);
        let factor = 3;
        assert!(!is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_false_for_unknown() {
        let trip = TripCount::Unknown;
        let factor = 4;
        assert!(!is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_false_when_factor_exceeds_trip() {
        let trip = TripCount::Known(3);
        let factor = 5;
        assert!(!is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_with_instruction_side_table() {
        use crate::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand, RegId};
        use crate::node::IrNodeId;
        use smallvec::SmallVec;

        let mut table = InstructionSideTable::new();

        let loop_id = IrNodeId::new(1).unwrap();

        // Populate table with a loop
        table.insert(
            loop_id,
            Instruction {
                mnemonic: Mnemonic::Jcc(crate::instruction::Cond::Ne),
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops
                },
                encoding_hint: None,
            },
        );

        // Call the new signature with unroll factor 4.
        let _result = is_unroll_safe(&table, loop_id, 4);
        // Phase-3-m2-004 stub: currently always returns false (too conservative).
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
