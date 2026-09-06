//! Fixed-point iteration driver for effect-row inference across recursive
//! call graphs (issue #1397, follow-up to v0.25.M2 XRW-06-01 / commit
//! `7bf8935`).
//!
//! [`EffectRowWalker`] (see `effect_walker.rs`) infers a function's effect
//! row by walking its body once, consulting `call_infer_sites` for each
//! `App` node's callee row. That single pass is correct as long as every
//! callee's row is already fully known before the caller is walked — true
//! for non-recursive call graphs processed in dependency order, but not for
//! mutually-recursive functions: if `A` calls `B` and `B` calls `A`, neither
//! function's row is known before the other's walk needs it, so a single
//! pass under-infers (a use of `mem` inside `A` never reaches `B`'s row, or
//! vice versa).
//!
//! This module wraps repeated single-pass walks in a whole-graph iteration:
//! each pass re-walks every function using the *previous* pass's best-known
//! row for every callee, and updates that function's own row from the
//! result. Because [`infer_or_check_call_row`](crate::infer_or_check_call_row)
//! only ever widens an implicit
//! caller's row (union is monotone) and never narrows it, repeating this
//! process is a standard Kleene/Knaster–Tarski iteration over a
//! finite-height lattice (the powerset of the module's finite effect set):
//! it is guaranteed to reach a fixed point, and for a call graph with `e`
//! edges the number of passes needed is bounded by `e` (each pass that
//! changes anything propagates at least one edge's effect one step further
//! around the graph). [`run_fixed_point`] additionally imposes a hard cap
//! (see [`FixedPointConfig::max_passes`]) as a safety net against
//! pathological input, emitting **F1107** if the cap is exceeded rather than
//! looping forever.
//!
//! This is a wrapper around [`EffectRowWalker`], not a replacement: within a
//! single pass, one function's walk is byte-for-byte the same
//! `infer_or_check_call_row` logic XRW-06-01 landed. The only new behavior
//! is *which* callee row gets injected before each pass, and *how many*
//! passes run.
//!
//! # Scope
//!
//! Like the rest of the phase-1/phase-2-m1 effect-row infrastructure, this
//! module operates at the injection-table level: callers supply each
//! function as an already-lowered [`IrArena`] + root, plus an explicit
//! table of which `App` nodes call which other function in the group (a
//! [`FnSlot`]). Resolving real `App` nodes to real function identities from
//! name resolution is future wiring (the same "production will pull this
//! from the IR once function types are threaded through" deferral already
//! documented in `effect_infer.rs` and `effect_walker.rs`).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, SourceMap, Span, VecSink};
use paideia_as_effects::EffectRow;
use paideia_as_ir::{IrArena, IrNodeId, WalkerCtx, walk};

use crate::effect_walker::EffectRowWalker;

/// Diagnostic code for effect-row fixed-point iteration exceeding its
/// safety-net pass cap (issue #1397).
///
/// This should not fire under sound inputs: a call graph with `e` edges
/// converges in at most `e` changed passes (see the module doc). Firing
/// indicates either a pathologically large mutual-recursion cycle or a
/// defect in the caller's supplied graph (e.g. a cycle wired with a much
/// higher edge count than [`FixedPointConfig::max_passes`] assumed).
pub const F_FIXPOINT_DIVERGED: u16 = 1107;

/// Opaque handle identifying one function within a [`run_fixed_point`] call.
///
/// Indexes into the `units` slice passed to [`run_fixed_point`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct FnSlot(pub usize);

/// One `perform <Effect>.<op>` contribution inside a [`FnUnit`]'s own body,
/// mirroring [`EffectRowWalker::inject_perform`].
#[derive(Clone, Debug)]
pub struct FnPerform {
    /// The `Perform` node in this function's own arena.
    pub node: IrNodeId,
    /// The effect name (see [`EffectRowWalker::inject_perform`]).
    pub effect_name: String,
    /// The operation name.
    pub op_name: String,
}

/// One call site inside a [`FnUnit`]'s own body, naming which other unit
/// (by [`FnSlot`]) it calls.
#[derive(Clone, Debug)]
pub struct FnCall {
    /// The `App` node in this function's own arena.
    pub node: IrNodeId,
    /// Which unit in the same [`run_fixed_point`] call this call targets.
    /// May equal the calling unit's own slot (direct recursion).
    pub callee: FnSlot,
}

/// One function participating in fixed-point effect-row inference.
///
/// Each unit owns its own small IR tree, walked independently every pass —
/// the same shape the existing `effect_walker` unit tests already build by
/// hand, just threaded through repeated walks instead of one.
pub struct FnUnit {
    /// This function's own IR arena.
    pub arena: IrArena,
    /// The root node to walk within `arena`.
    pub root: IrNodeId,
    /// `None`: this function leaves its effect row implicit, to be inferred
    /// from its body and callees. `Some(row)`: the function declares
    /// `!{...}` explicitly — a fixed upper bound that never changes across
    /// passes (see [`infer_or_check_call_row`](crate::infer_or_check_call_row)).
    pub explicit_row: Option<EffectRow>,
    /// This function's own `perform` contributions.
    pub performs: Vec<FnPerform>,
    /// This function's own call sites into other units in the group.
    pub calls: Vec<FnCall>,
}

impl FnUnit {
    /// Construct a unit with no performs or calls yet (a leaf, until
    /// `calls`/`performs` are populated).
    #[must_use]
    pub fn new(arena: IrArena, root: IrNodeId, explicit_row: Option<EffectRow>) -> Self {
        Self {
            arena,
            root,
            explicit_row,
            performs: Vec::new(),
            calls: Vec::new(),
        }
    }
}

/// Tuning knobs for [`run_fixed_point`].
#[derive(Clone, Debug)]
pub struct FixedPointConfig {
    /// Hard cap on the number of whole-graph passes. Exceeding it emits
    /// **F1107** instead of looping forever. A safe default is
    /// `unit_count * 4` (see [`FixedPointConfig::for_unit_count`]) — the
    /// module doc's O(edges) convergence bound with headroom for the
    /// mandatory confirmation pass every run needs (a pass that computes
    /// the correct fixed point still shows "changed" relative to the
    /// pessimistic empty seed, so one further no-op pass is required to
    /// detect convergence).
    pub max_passes: u32,
    /// Span used for the F1107 diagnostic if the cap is exceeded.
    pub diverged_span: Span,
}

impl FixedPointConfig {
    /// A safe default cap for a call graph of `unit_count` functions:
    /// `unit_count * 4`, floored at 4 so tiny graphs still get a
    /// confirmation pass plus headroom.
    #[must_use]
    pub fn for_unit_count(unit_count: usize, diverged_span: Span) -> Self {
        Self {
            max_passes: (unit_count as u32 * 4).max(4),
            diverged_span,
        }
    }
}

/// Outcome of a full [`run_fixed_point`] run.
pub struct FixedPointOutcome {
    /// Final effect row per unit, parallel to the input `units` slice.
    pub rows: Vec<EffectRow>,
    /// Diagnostics from the final pass (the pass whose rows are reported
    /// above) plus, if the cap was exceeded, one trailing F1107.
    ///
    /// Earlier passes' diagnostics are intentionally discarded: they are
    /// computed against not-yet-converged callee rows and can be transiently
    /// wrong in either direction. Only the settled (or cap-exceeded) pass's
    /// diagnostics reflect the graph's actual semantics.
    pub diagnostics: Vec<Diagnostic>,
    /// How many whole-graph passes actually ran (including the final
    /// confirmation pass).
    pub passes_run: u32,
    /// `true` if the cap was hit before the rows stabilized.
    pub diverged: bool,
}

/// Run fixed-point effect-row inference over a group of (possibly mutually
/// recursive) functions.
///
/// Each pass walks every unit in `units` order with a fresh
/// [`EffectRowWalker`], injecting the *current* best-known row for each
/// call site's callee (Gauss–Seidel style: a callee processed earlier in
/// the same pass contributes its freshly-updated row to a caller processed
/// later in that same pass — supplying `units` in roughly call order,
/// callees before callers, minimizes pass count, though the algorithm is
/// correct in any order). A pass in which no unit's row changed relative to
/// the row used to seed it is the fixed point; iteration stops there, or at
/// `config.max_passes` with an **F1107** appended to the returned
/// diagnostics.
///
/// Preserves XRW-06-01's per-call semantics exactly: within one pass, one
/// unit's walk runs [`EffectRowWalker`] /
/// [`infer_or_check_call_row`](crate::infer_or_check_call_row) unchanged.
/// This function only decides which callee row to inject before each pass
/// and when to stop.
#[must_use]
pub fn run_fixed_point(units: &[FnUnit], config: &FixedPointConfig) -> FixedPointOutcome {
    let mut rows: Vec<EffectRow> = units
        .iter()
        .map(|u| u.explicit_row.clone().unwrap_or_else(EffectRow::empty))
        .collect();

    let mut passes_run = 0u32;
    let mut final_diags: Vec<Diagnostic> = Vec::new();
    let mut diverged = false;

    loop {
        passes_run += 1;
        let mut changed = false;
        let mut pass_diags = Vec::new();

        for (idx, unit) in units.iter().enumerate() {
            let mut walker = EffectRowWalker::new();
            for perform in &unit.performs {
                walker.inject_perform(perform.node, perform.effect_name.clone(), perform.op_name.clone());
            }
            for call in &unit.calls {
                let callee_row = rows[call.callee.0].clone();
                walker.inject_call_for_inference(call.node, callee_row, unit.explicit_row.clone());
            }

            let source_map = SourceMap::new();
            let mut sink = VecSink::new();
            {
                let mut ctx = WalkerCtx::new(&source_map, &mut sink);
                walk(&mut walker, &unit.arena, unit.root, &mut ctx);
            }

            let new_row = unit
                .explicit_row
                .clone()
                .unwrap_or_else(|| walker.current_row().clone());

            if new_row != rows[idx] {
                changed = true;
            }
            rows[idx] = new_row;
            pass_diags.extend(sink.diagnostics().iter().cloned());
        }

        final_diags = pass_diags;

        if !changed {
            break;
        }
        if passes_run >= config.max_passes {
            diverged = true;
            final_diags.push(fixpoint_diverged_diag(config.max_passes, config.diverged_span));
            break;
        }
    }

    FixedPointOutcome {
        rows,
        diagnostics: final_diags,
        passes_run,
        diverged,
    }
}

fn fixpoint_diverged_diag(cap: u32, span: Span) -> Diagnostic {
    Diagnostic::error(f_code(F_FIXPOINT_DIVERGED))
        .message(format!(
            "effect-row inference over a recursive call graph did not converge after {cap} \
             passes; this indicates a pathologically large mutual-recursion cycle rather than \
             normal recursive code"
        ))
        .with_span(span)
        .finish()
}

fn f_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::F, Severity::Error, n).expect("valid F code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;
    use paideia_as_effects::EffectId;
    use paideia_as_ir::IrKind;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn eff(n: u32) -> EffectId {
        EffectId::new(n).expect("effect id")
    }

    /// Build a one-function unit: `Action → [Perform(mem)?, App(call target)?]`.
    /// `has_perform` controls whether the function itself performs `mem`
    /// (effect id 1); `call_target`, if set, is the slot this function's
    /// single call site targets.
    ///
    /// The root is `Action`, not `Module`: `Module`'s `post_visit` runs
    /// `check_no_unhandled` (F1100) against whatever row remains at that
    /// node — correct for the whole *program's* boundary, but wrong for one
    /// *function's* own body root, which should propagate its residual row
    /// to its callers rather than being flagged unhandled at its own exit.
    /// `Action` (a generic effectful sequence) has no such special-casing in
    /// [`EffectRowWalker`], so each unit's own row settles cleanly.
    fn build_unit(
        has_perform: bool,
        call_target: Option<FnSlot>,
        explicit_row: Option<EffectRow>,
    ) -> FnUnit {
        let mut arena = IrArena::new();
        let s = span();
        let mut children = Vec::new();

        let perform_id = if has_perform {
            let id = arena.alloc(IrKind::Perform, s);
            children.push(id);
            Some(id)
        } else {
            None
        };

        let app_id = if call_target.is_some() {
            let id = arena.alloc(IrKind::App, s);
            children.push(id);
            Some(id)
        } else {
            None
        };

        let root = arena.alloc_with_children(IrKind::Action, s, children);
        let mut unit = FnUnit::new(arena, root, explicit_row);

        if let Some(pid) = perform_id {
            unit.performs.push(FnPerform {
                node: pid,
                effect_name: "Mem".to_string(),
                op_name: "read".to_string(),
            });
        }
        if let (Some(aid), Some(target)) = (app_id, call_target) {
            unit.calls.push(FnCall {
                node: aid,
                callee: target,
            });
        }

        unit
    }

    fn mem_row() -> EffectRow {
        EffectRow::from_ids(vec![eff(1)], None)
    }

    // ── (a) Direct recursion: fn a() { perform mem; a() } ──────────────

    #[test]
    fn direct_recursion_converges_to_own_perform() {
        // fn a (slot 0): performs mem, then calls itself.
        let a = build_unit(true, Some(FnSlot(0)), None);
        let units = [a];
        let config = FixedPointConfig::for_unit_count(units.len(), span());

        let outcome = run_fixed_point(&units, &config);

        assert_eq!(outcome.rows[0], mem_row(), "a's row must include mem");
        assert!(!outcome.diverged, "direct recursion must not hit the cap");
        // The self-call's contribution is already folded in via a's own
        // Perform node in the very first pass (the effect does not depend
        // on the self-referential callee-row injection at all); only one
        // further confirmation pass is needed to detect no further change.
        assert!(
            outcome.passes_run <= 2,
            "direct recursion should settle almost immediately, got {} passes",
            outcome.passes_run
        );
    }

    // ── (b) Mutual 2-cycle: a → b → a, only a performs mem ──────────────

    #[test]
    fn mutual_two_cycle_converges_effect_across_both() {
        // a (slot 0): performs mem, calls b (slot 1).
        // b (slot 1): implicit, calls a (slot 0). No perform of its own.
        let a = build_unit(true, Some(FnSlot(1)), None);
        let b = build_unit(false, Some(FnSlot(0)), None);
        let units = [a, b];
        let config = FixedPointConfig::for_unit_count(units.len(), span());

        let outcome = run_fixed_point(&units, &config);

        assert_eq!(outcome.rows[0], mem_row(), "a's row must include mem");
        assert_eq!(
            outcome.rows[1],
            mem_row(),
            "b's row must pick up mem transitively through the cycle (issue #1397's gap)"
        );
        assert!(!outcome.diverged);
        assert!(
            outcome.passes_run <= 3,
            "a 2-cycle should settle within a few passes, got {}",
            outcome.passes_run
        );
    }

    // ── (c) Mutual 3-cycle: a → b → c → a, only a performs mem ──────────

    #[test]
    fn mutual_three_cycle_converges_effect_around_the_ring() {
        // a (slot 0): performs mem, calls b (slot 1).
        // b (slot 1): implicit, calls c (slot 2).
        // c (slot 2): implicit, calls a (slot 0).
        let a = build_unit(true, Some(FnSlot(1)), None);
        let b = build_unit(false, Some(FnSlot(2)), None);
        let c = build_unit(false, Some(FnSlot(0)), None);
        let units = [a, b, c];
        let config = FixedPointConfig::for_unit_count(units.len(), span());

        let outcome = run_fixed_point(&units, &config);

        assert_eq!(outcome.rows[0], mem_row());
        assert_eq!(
            outcome.rows[1],
            mem_row(),
            "b must see mem transitively (a -> b -> c -> a ring)"
        );
        assert_eq!(
            outcome.rows[2],
            mem_row(),
            "c must see mem transitively (a -> b -> c -> a ring)"
        );
        assert!(!outcome.diverged);
        assert!(
            outcome.passes_run <= 5,
            "a 3-cycle should settle within a handful of passes, got {}",
            outcome.passes_run
        );
    }

    // ── (d) Non-recursive baseline: no regression from XRW-06-01 ────────

    #[test]
    fn non_recursive_chain_converges_without_regression() {
        // b (slot 1): performs mem, no calls (leaf).
        // a (slot 0): implicit, calls b.
        // Units are supplied callee-first so a single pass already sees
        // b's settled row when computing a's.
        let b = build_unit(true, None, None);
        let a = build_unit(false, Some(FnSlot(0)), None);
        let units = [b, a];
        let config = FixedPointConfig::for_unit_count(units.len(), span());

        let outcome = run_fixed_point(&units, &config);

        assert_eq!(outcome.rows[0], mem_row(), "b (leaf) infers its own perform");
        assert_eq!(outcome.rows[1], mem_row(), "a inherits b's row through the call");
        assert!(!outcome.diverged);
        assert!(
            outcome.passes_run <= 2,
            "a non-recursive DAG must not need extra passes beyond the confirmation pass, got {}",
            outcome.passes_run
        );
    }

    #[test]
    fn single_pure_function_is_trivially_stable() {
        let a = build_unit(false, None, None);
        let units = [a];
        let config = FixedPointConfig::for_unit_count(units.len(), span());

        let outcome = run_fixed_point(&units, &config);

        assert!(outcome.rows[0].is_empty());
        assert!(!outcome.diverged);
        assert_eq!(outcome.passes_run, 1, "an empty row never changes from its own empty seed");
    }

    #[test]
    fn explicit_row_function_never_changes_but_is_rechecked_each_pass() {
        // a (slot 0): explicit row {mem}, calls b.
        // b (slot 1): implicit, performs mem.
        let explicit = mem_row();
        let a = build_unit(false, Some(FnSlot(1)), Some(explicit.clone()));
        let b = build_unit(true, None, None);
        let units = [a, b];
        let config = FixedPointConfig::for_unit_count(units.len(), span());

        let outcome = run_fixed_point(&units, &config);

        assert_eq!(outcome.rows[0], explicit, "explicit row is a fixed upper bound");
        assert_eq!(outcome.rows[1], mem_row());
        assert!(outcome.diagnostics.is_empty(), "b's mem fits within a's explicit {{mem}}");
        assert!(!outcome.diverged);
    }

    #[test]
    fn cap_exceeded_emits_f1107() {
        // A 2-cycle with a cap too small to ever settle, forcing divergence.
        let a = build_unit(true, Some(FnSlot(1)), None);
        let b = build_unit(false, Some(FnSlot(0)), None);
        let units = [a, b];
        let config = FixedPointConfig {
            max_passes: 1,
            diverged_span: span(),
        };

        let outcome = run_fixed_point(&units, &config);

        assert!(outcome.diverged);
        assert_eq!(outcome.passes_run, 1);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|d| d.code().number() == F_FIXPOINT_DIVERGED),
            "cap-exceeded must emit F1107"
        );
    }
}
