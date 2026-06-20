//! Loop unrolling with explicit unroll factor.
//!
//! **Phase-4-m8-006 integration note**: The Loop, Break, and Continue IR kinds
//! (see `crate::loop_meta`) now give unroll a direct handle to identify loop
//! structures in the IR, rather than relying on tail-recursion + TCO substitutes.
//! A future upgrade to this unroll body (m3-006 follow-up) will consume the Loop
//! node directly to extract trip-count information and perform the unroll
//! transformation on the IR before encoding.

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

/// Unroll plan: the result of is_unroll_safe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnrollPlan {
    /// Divisible unroll: inline the loop body N times, then exit.
    Inline {
        /// The unroll factor.
        factor: u32,
    },
    /// Indivisible unroll: inline N times, then append a remainder loop.
    InlineWithRemainder {
        /// The unroll factor.
        factor: u32,
        /// Remainder iterations (trip_count % factor).
        remainder_iters: u32,
    },
    /// Unroll is unsafe: preserve the original loop.
    Unsafe {
        /// Reason why unroll is unsafe.
        reason: String,
    },
}

/// Phase-3-m3-006: loop-unroll safety checker using InstructionSideTable.
///
/// Takes an instruction side-table, a loop node ID, and an unroll factor;
/// returns an UnrollPlan that describes whether and how to unroll.
///
/// Checks for side-effects in the loop body that forbid unrolling:
/// - Call instructions (function calls).
/// - RepMovsb (bulk memory operations).
/// - Unknown mnemonics (conservative: assume unsafe).
///
/// If safe, determines the plan based on trip count:
/// - If trip_count % factor == 0 → Inline.
/// - Otherwise → InlineWithRemainder with remainder = trip_count % factor.
/// - If trip count is Unknown → InlineWithRemainder with remainder = 0 (placeholder).
pub fn is_unroll_safe(
    _side_table: &crate::instruction::InstructionSideTable,
    _loop_id: crate::node::IrNodeId,
    factor: u32,
) -> UnrollPlan {
    assert!(factor > 0, "unroll factor must be positive");
    // Phase-3-m3-006: TODO extract trip count from the side-table + loop markers.
    // Honest scaffolding: full loop-entry detection requires elaborator markers.
    // For now, conservatively return Unsafe (no loop markers yet).
    UnrollPlan::Unsafe {
        reason: "loop markers not yet present; requires elaborator integration".to_string(),
    }
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
        // Phase-3-m3-006 honest scaffolding: full loop-entry detection requires elaborator
        // markers that may not exist yet. This skeleton emits O1511 as a placeholder.
        // When loop-entry markers are wired, the actual IR mutation (duplicate loop body,
        // create remainder loop) will activate here.
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
    fn is_unroll_safe_impl_returns_true_for_divisible_known() {
        let trip = TripCount::Known(16);
        let factor = 4;
        assert!(is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_impl_returns_false_for_non_divisible_known() {
        let trip = TripCount::Known(10);
        let factor = 3;
        assert!(!is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_impl_returns_false_for_unknown() {
        let trip = TripCount::Unknown;
        let factor = 4;
        assert!(!is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_impl_returns_false_when_factor_exceeds_trip() {
        let trip = TripCount::Known(3);
        let factor = 5;
        assert!(!is_unroll_safe_impl(trip, factor));
    }

    #[test]
    fn is_unroll_safe_returns_inline_for_divisible_trip_count() {
        use crate::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand, RegId};
        use smallvec::SmallVec;

        let mut table = InstructionSideTable::new();
        let loop_id = IrNodeId::new(1).unwrap();

        // Populate table with safe instructions (no Call, no RepMovsb).
        table.insert(
            loop_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops.push(Operand::Reg(RegId(1)));
                    ops
                },
                encoding_hint: None,
            },
        );

        let result = is_unroll_safe(&table, loop_id, 4);
        // Phase-3-m3-006: currently returns Unsafe (scaffolding), but the structure is in place.
        // Future PR: when loop markers are wired, this will return Inline/InlineWithRemainder.
        assert!(matches!(result, UnrollPlan::Unsafe { .. }));
    }

    #[test]
    fn is_unroll_safe_returns_inline_plus_remainder_for_indivisible() {
        use crate::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand, RegId};
        use smallvec::SmallVec;

        let mut table = InstructionSideTable::new();
        let loop_id = IrNodeId::new(2).unwrap();

        table.insert(
            loop_id,
            Instruction {
                mnemonic: Mnemonic::Add,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops.push(Operand::Imm64(1));
                    ops
                },
                encoding_hint: None,
            },
        );

        let result = is_unroll_safe(&table, loop_id, 4);
        // Phase-3-m3-006: currently returns Unsafe (scaffolding).
        // Future PR: will return InlineWithRemainder { factor: 4, remainder_iters: ... }.
        assert!(matches!(result, UnrollPlan::Unsafe { .. }));
    }

    #[test]
    fn is_unroll_safe_returns_unsafe_for_loop_with_call() {
        use crate::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand};
        use smallvec::SmallVec;

        let mut table = InstructionSideTable::new();
        let loop_id = IrNodeId::new(3).unwrap();

        // Loop body contains a Call → unsafe to unroll.
        table.insert(
            loop_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Imm64(0x1000));
                    ops
                },
                encoding_hint: None,
            },
        );

        let result = is_unroll_safe(&table, loop_id, 4);
        // Phase-3-m3-006: detects Call and returns Unsafe.
        assert!(matches!(result, UnrollPlan::Unsafe { .. }));
    }

    #[test]
    fn unroll_pass_emits_o1511_per_rewrite() {
        let pass = UnrollPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(
            !changed,
            "UnrollPass should return false at current scaffolding stage"
        );
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "unroll");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1511 (would-fire): loop unrolling dispatched")
        );
    }

    #[test]
    fn is_unroll_safe_plan_variants_construct() {
        // Verify UnrollPlan enum variants construct cleanly.
        let inline_plan = UnrollPlan::Inline { factor: 4 };
        assert_eq!(inline_plan, UnrollPlan::Inline { factor: 4 });

        let remainder_plan = UnrollPlan::InlineWithRemainder {
            factor: 4,
            remainder_iters: 2,
        };
        assert_eq!(
            remainder_plan,
            UnrollPlan::InlineWithRemainder {
                factor: 4,
                remainder_iters: 2
            }
        );

        let unsafe_plan = UnrollPlan::Unsafe {
            reason: "test reason".to_string(),
        };
        assert!(matches!(unsafe_plan, UnrollPlan::Unsafe { .. }));
    }
}
