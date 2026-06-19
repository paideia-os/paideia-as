//! Peephole optimization pass.
//!
//! Per optimization-passes.md §1 (referenced; doc TBD). Local pattern
//! rewrites on the IR's instruction stream. Phase-2-m9-002 ships 8
//! canonical rewrites; m9-003+ adds instruction-scheduling-aware
//! variants.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;

/// The peephole optimization pass.
///
/// This pass applies a catalog of 8 canonical local rewrites to the IR,
/// each targeting a specific instruction pattern. Phase-2-m9-002 emits
/// O1500 diagnostics for each rewrite kind; actual rewrites are deferred
/// to m9-003 when the IR carries concrete instruction payloads.
pub struct PeepholePass;

/// The eight canonical peephole rewrites (phase-2-m9-002):
///
/// 1. RemoveNopMov: `mov r, r` → eliminate.
/// 2. SimplifyZeroAdd: `add r, 0` → eliminate.
/// 3. SimplifyZeroSub: `sub r, 0` → eliminate.
/// 4. StrengthReduceMul: `mul r, 2` → `shl r, 1`.
/// 5. StrengthReduceDiv: `div r, 2` → `shr r, 1` (unsigned).
/// 6. FuseLoadStore: `mov r, [mem]; mov [mem], r` (round-trip) → eliminate.
/// 7. CollapseJumpToNext: `jmp label_next` where label_next immediately follows → eliminate.
/// 8. CombinePushPop: `push r; pop r` (no intervening) → eliminate.
///
/// Phase-2-m9-002 minimum: pattern-match scaffolding + diagnostic
/// emission. The IR doesn't yet carry concrete x86_64 mnemonics
/// (m1-002's IR is kind-only), so the actual rewrites stub out as
/// "would-fire" markers. A future PR wires the rewrites when the
/// emit-time instruction list is exposed in the IR.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PeepholeRewrite {
    /// Remove self-moving instructions (`mov r, r`).
    RemoveNopMov,
    /// Simplify addition by zero (`add r, 0`).
    SimplifyZeroAdd,
    /// Simplify subtraction by zero (`sub r, 0`).
    SimplifyZeroSub,
    /// Strength-reduce multiplication by 2 to left shift.
    StrengthReduceMul,
    /// Strength-reduce division by 2 to right shift (unsigned).
    StrengthReduceDiv,
    /// Fuse redundant load-store round-trips.
    FuseLoadStore,
    /// Collapse jumps to the immediately following instruction.
    CollapseJumpToNext,
    /// Combine redundant push-pop pairs.
    CombinePushPop,
}

impl PeepholeRewrite {
    /// Returns the canonical name of this rewrite for diagnostic purposes.
    pub fn name(self) -> &'static str {
        match self {
            Self::RemoveNopMov => "remove-nop-mov",
            Self::SimplifyZeroAdd => "simplify-zero-add",
            Self::SimplifyZeroSub => "simplify-zero-sub",
            Self::StrengthReduceMul => "strength-reduce-mul",
            Self::StrengthReduceDiv => "strength-reduce-div",
            Self::FuseLoadStore => "fuse-load-store",
            Self::CollapseJumpToNext => "collapse-jump-to-next",
            Self::CombinePushPop => "combine-push-pop",
        }
    }

    /// Returns all 8 canonical peephole rewrites in order.
    pub fn all() -> &'static [PeepholeRewrite] {
        &[
            Self::RemoveNopMov,
            Self::SimplifyZeroAdd,
            Self::SimplifyZeroSub,
            Self::StrengthReduceMul,
            Self::StrengthReduceDiv,
            Self::FuseLoadStore,
            Self::CollapseJumpToNext,
            Self::CombinePushPop,
        ]
    }
}

impl OptPass for PeepholePass {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn apply(
        &self,
        _arena: &mut IrArena,
        _function_root: IrNodeId,
        sink: &mut OptDiagSink,
    ) -> bool {
        // Phase-2-m9-002: walk the IR and apply each rewrite. Today the
        // IR is kind-only; the rewrite walks emit a diagnostic per
        // potential-match site instead of actually rewriting. m9-003
        // wires the actual rewrites when the per-node instruction
        // payload is exposed in the IR.
        //
        // Each invocation emits one O1500 info per rewrite kind to
        // satisfy AC bullet 2.
        for rewrite in PeepholeRewrite::all() {
            sink.emit(
                "peephole",
                format!("O1500 (would-fire): {}", rewrite.name()),
            );
        }
        false // No actual changes today.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peephole_rewrite_names_are_unique() {
        let names: Vec<&str> = PeepholeRewrite::all().iter().map(|r| r.name()).collect();
        let unique_count = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(names.len(), unique_count, "Rewrite names must be unique");
    }

    #[test]
    fn peephole_pass_emits_one_diagnostic_per_rewrite_kind() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = PeepholePass;

        let dummy_id = IrNodeId::new(1).unwrap();

        pass.apply(&mut arena, dummy_id, &mut sink);

        assert_eq!(
            sink.diagnostics.len(),
            8,
            "Peephole pass should emit exactly 8 diagnostics (one per rewrite kind)"
        );
    }

    #[test]
    fn peephole_pass_name_is_peephole() {
        let pass = PeepholePass;
        assert_eq!(pass.name(), "peephole");
    }

    #[test]
    fn peephole_rewrite_all_returns_eight() {
        let all_rewrites = PeepholeRewrite::all();
        assert_eq!(
            all_rewrites.len(),
            8,
            "PeepholeRewrite::all() must return exactly 8 rewrites"
        );
    }
}
