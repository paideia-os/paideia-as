//! Handler composition for algebraic effects (v0.29-M1-002, issue #1376).
//!
//! Two handlers
//!
//! ```text
//!   h1 : !{ E1 | ρ1 } → !ρ1
//!   h2 : !{ E2 | ρ2 } → !ρ2
//! ```
//!
//! compose to
//!
//! ```text
//!   h1 ∘ h2 : !{ E1, E2 | ρ } → !ρ
//! ```
//!
//! when the two residual rows `ρ1` and `ρ2` unify (per the wave-0 rule
//! set in [`crate::row_poly::unify_rows`]) and the two handled effect
//! sets are disjoint.
//!
//! ## Order convention
//!
//! `compose_handlers(h1, h2)` places `h1` on the OUTSIDE and `h2` on
//! the INSIDE. At the row-typing level composition is commutative for
//! independent effects — the handled and residual rows are the same in
//! either order — but the runtime continuation stack is
//! `outer(h1) . inner(h2) . k`, which matters when the two effect
//! signatures interact (e.g. `h1` catches an operation `h2` re-raises).
//! The composed [`Handler::body`] carries the outer handler's body id,
//! so the order is observable at the type level.
//!
//! ## Coordination
//!
//! * `EffectRow`, `EffectId`, `RowVar`, `unify_rows`, `RowSubst`,
//!   `RowUnification`, and `RowUnifyError` come from the sibling
//!   [`crate::row_poly`] module (b2-07, issue #1375, landed alongside
//!   this one). The two primitives were designed to interlock: this
//!   module never redefines row-shaped types, and never touches the
//!   `paideia-as-effects` runtime interner — the wire-up between the
//!   type-level and runtime representations lands in v0.29-M1-004.
//! * `ExprId` is a local opaque handle for the handler body. It is a
//!   placeholder until v0.29-M1-004 replaces it with the canonical IR
//!   node id from `paideia-as-ir` (`IrNodeId`, or a dedicated `ExprId`
//!   newtype once expression nodes are separated out). The shape —
//!   `Copy + Eq + Hash + Debug` over a `u32` — matches what the
//!   elaborator will supply, so the swap is a re-export, not a
//!   semantic change.

use crate::row_poly::{EffectId, EffectRow, RowUnifyError, unify_rows};

/// Opaque handle to the IR expression backing a handler body.
///
/// See the module docs — this is a placeholder until v0.29-M1-004
/// wires the canonical IR expression id into the types crate.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExprId(pub u32);

impl core::fmt::Display for ExprId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "expr#{}", self.0)
    }
}

/// An effect handler.
///
/// * `handled`  — the effect row consumed on input.
/// * `residual` — the effect row that survives after the handler runs.
/// * `body`     — opaque handle to the IR expression implementing the
///                handler. Composed handlers carry the outer handler's
///                body id (see [`compose_handlers`]).
#[derive(Clone, Debug)]
pub struct Handler {
    /// Effect row consumed by this handler.
    pub handled: EffectRow,
    /// Effect row remaining after this handler runs.
    pub residual: EffectRow,
    /// IR expression backing the handler body.
    pub body: ExprId,
}

impl Handler {
    /// Construct a fresh handler.
    #[must_use]
    pub fn new(handled: EffectRow, residual: EffectRow, body: ExprId) -> Self {
        Self {
            handled,
            residual,
            body,
        }
    }
}

/// Errors surfaced by [`compose_handlers`].
#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum ComposeErr {
    /// The two handlers' residual rows do not unify under the wave-0
    /// rule set (see [`crate::row_poly::unify_rows`]).
    #[error(
        "handler residual rows do not unify ({source}): h1.residual = {h1_residual:?}, \
         h2.residual = {h2_residual:?}"
    )]
    ResidualsDoNotUnify {
        /// `h1.residual` at the composition site.
        h1_residual: EffectRow,
        /// `h2.residual` at the composition site.
        h2_residual: EffectRow,
        /// The underlying row-unification failure.
        #[source]
        source: RowUnifyError,
    },
    /// The two handlers share one or more handled effects, so the
    /// composition is ambiguous — the caller must collapse the
    /// overlapping arms into a single handler first.
    #[error(
        "handlers overlap on effects: {overlap:?} \
         (collapse into a single handler before composing)"
    )]
    OverlappingHandled {
        /// The effects present in both `h1.handled` and `h2.handled`.
        overlap: Vec<EffectId>,
    },
}

/// Compose two handlers `h1` (outer) and `h2` (inner).
///
/// # Semantics
///
/// Given
///
/// * `h1 : !{ E1 | ρ } → !ρ`
/// * `h2 : !{ E2 | ρ } → !ρ`
///
/// with disjoint fixed `E1`, `E2` and residuals that unify per
/// [`crate::row_poly::unify_rows`], the result is
/// `!{ E1, E2 | ρ } → !ρ` where `ρ` is the row produced by
/// residual unification (either side if they were syntactically
/// equal, otherwise the concrete row that the bare-tail variable
/// absorbed).
///
/// # Order preservation
///
/// The composed [`Handler::body`] mirrors `h1.body`, so
/// `compose_handlers(h1, h2)` and `compose_handlers(h2, h1)` produce
/// the same `handled` and `residual` rows for independent effects but
/// carry different bodies — reflecting the different continuation
/// stack that interacting effects would see at runtime.
///
/// # Errors
///
/// * [`ComposeErr::ResidualsDoNotUnify`] — the two residual rows fail
///   row unification.
/// * [`ComposeErr::OverlappingHandled`] — the two handled effect sets
///   share one or more effects.
pub fn compose_handlers(h1: &Handler, h2: &Handler) -> Result<Handler, ComposeErr> {
    // 1. Residuals must unify under the row_poly wave-0 rule set.
    let unification = unify_rows(&h1.residual, &h2.residual).map_err(|source| {
        ComposeErr::ResidualsDoNotUnify {
            h1_residual: h1.residual.clone(),
            h2_residual: h2.residual.clone(),
            source,
        }
    })?;

    // 2. Handled fixed sets must be disjoint. (Row-poly tails on the
    //    handled sides are not part of the algebraic-effect story: a
    //    handler names the concrete effects it dispatches on, and any
    //    row-variable tail there would already be part of `residual`.)
    let overlap: Vec<EffectId> = h1
        .handled
        .fixed
        .iter()
        .copied()
        .filter(|e| h2.handled.fixed.contains(e))
        .collect();
    if !overlap.is_empty() {
        return Err(ComposeErr::OverlappingHandled { overlap });
    }

    // 3. Compose: union the handled rows; take the unified residual;
    //    keep the OUTER handler's body id as the order witness.
    let residual = match unification.subst {
        // The unifier bound a bare tail variable to the other row; the
        // concrete row it was bound to IS the unified residual.
        Some(subst) => subst.row,
        // Rows were syntactically equal — either side is the same row.
        None => h1.residual.clone(),
    };

    Ok(Handler {
        handled: h1.handled.union(&h2.handled),
        residual,
        body: h1.body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row_poly::RowVar;

    fn eid(n: u32) -> EffectId {
        EffectId::new(n).expect("non-zero effect id")
    }

    fn rvar(n: u32) -> RowVar {
        RowVar::new(n).expect("non-zero row var")
    }

    fn expr(n: u32) -> ExprId {
        ExprId(n)
    }

    /// `handle E1 then handle E2` and `handle E2 then handle E1` yield
    /// the same residual row (and the same combined handled row) for
    /// INDEPENDENT effects — composition is commutative at the row-
    /// typing level. See issue #1376 AC.
    #[test]
    fn independent_effects_commute_at_row_level() {
        // ρ is `!{ | ρ0}` — a bare tail variable, so residuals unify
        // trivially with themselves.
        let residual = EffectRow::from_ids([], Some(rvar(1)));
        let h1 = Handler::new(
            EffectRow::from_ids([eid(1)], Some(rvar(1))),
            residual.clone(),
            expr(101),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2)], Some(rvar(1))),
            residual,
            expr(202),
        );

        let a = compose_handlers(&h1, &h2).expect("h1 ∘ h2 composes");
        let b = compose_handlers(&h2, &h1).expect("h2 ∘ h1 composes");

        assert_eq!(a.handled, b.handled);
        assert_eq!(a.residual, b.residual);

        // The combined handled row is exactly {E1, E2 | ρ}.
        let expected_handled = EffectRow::from_ids([eid(1), eid(2)], Some(rvar(1)));
        assert_eq!(a.handled, expected_handled);
    }

    /// Handler order is observable in the composed body — swapping the
    /// arguments to `compose_handlers` yields the same row-level types
    /// but a distinct body identity. This mirrors the runtime
    /// continuation stack difference for interacting effects.
    #[test]
    fn handler_order_is_preserved_in_body() {
        let residual = EffectRow::from_ids([], Some(rvar(1)));
        let h1 = Handler::new(
            EffectRow::from_ids([eid(1)], Some(rvar(1))),
            residual.clone(),
            expr(101),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2)], Some(rvar(1))),
            residual,
            expr(202),
        );

        let outer_h1 = compose_handlers(&h1, &h2).unwrap();
        let outer_h2 = compose_handlers(&h2, &h1).unwrap();

        assert_eq!(outer_h1.body, h1.body);
        assert_eq!(outer_h2.body, h2.body);
        assert_ne!(
            outer_h1.body, outer_h2.body,
            "handler order must be observable via the composed body"
        );
    }

    /// Overlapping handled sets are rejected — the caller must
    /// collapse the ambiguous arms first.
    #[test]
    fn rejects_overlapping_handled_effects() {
        let residual = EffectRow::from_ids([], Some(rvar(1)));
        let h1 = Handler::new(
            EffectRow::from_ids([eid(1), eid(3)], Some(rvar(1))),
            residual.clone(),
            expr(1),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2), eid(3)], Some(rvar(1))),
            residual,
            expr(2),
        );

        match compose_handlers(&h1, &h2) {
            Err(ComposeErr::OverlappingHandled { overlap }) => {
                assert_eq!(overlap, vec![eid(3)]);
            }
            other => panic!("expected OverlappingHandled, got {other:?}"),
        }
    }

    /// Mismatched residuals surface as `ResidualsDoNotUnify` and
    /// preserve the underlying `RowUnifyError`.
    #[test]
    fn rejects_mismatched_residuals() {
        let h1 = Handler::new(
            EffectRow::from_ids([eid(1)], None),
            EffectRow::from_ids([eid(9)], None), // closed row with E9
            expr(1),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2)], None),
            EffectRow::from_ids([eid(8)], None), // closed row with E8 — mismatch
            expr(2),
        );

        match compose_handlers(&h1, &h2) {
            Err(ComposeErr::ResidualsDoNotUnify { source, .. }) => {
                assert_eq!(source, RowUnifyError::FixedMismatch);
            }
            other => panic!("expected ResidualsDoNotUnify, got {other:?}"),
        }
    }

    /// The combined `handled` row is the union of the two inputs, and
    /// the combined `residual` threads through the unified row.
    #[test]
    fn combines_handled_rows_as_union() {
        let residual = EffectRow::from_ids([], Some(rvar(2)));
        let h1 = Handler::new(
            EffectRow::from_ids([eid(1)], Some(rvar(2))),
            residual.clone(),
            expr(1),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2)], Some(rvar(2))),
            residual.clone(),
            expr(2),
        );

        let composed = compose_handlers(&h1, &h2).unwrap();
        let handled: Vec<EffectId> = composed.handled.fixed.iter().copied().collect();
        assert_eq!(handled, vec![eid(1), eid(2)]);
        assert_eq!(composed.residual, residual);
    }

    /// Closed residuals compose exactly like open ones when the fixed
    /// sets match.
    #[test]
    fn composes_with_closed_residuals() {
        let residual = EffectRow::empty();
        let h1 = Handler::new(
            EffectRow::from_ids([eid(1)], None),
            residual.clone(),
            expr(1),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2)], None),
            residual,
            expr(2),
        );

        let composed = compose_handlers(&h1, &h2).expect("closed residuals compose");
        assert_eq!(composed.handled, EffectRow::from_ids([eid(1), eid(2)], None));
        assert!(composed.residual.is_empty());
    }

    /// Bare-tail residual on one side absorbs a concrete residual on
    /// the other side, and the composed handler carries the concrete
    /// row as its residual.
    #[test]
    fn bare_tail_residual_absorbs_concrete_residual() {
        let concrete_residual = EffectRow::from_ids([eid(7)], None);
        let bare_residual = EffectRow::from_ids([], Some(rvar(5)));

        let h1 = Handler::new(
            EffectRow::from_ids([eid(1)], None),
            concrete_residual.clone(),
            expr(1),
        );
        let h2 = Handler::new(
            EffectRow::from_ids([eid(2)], None),
            bare_residual,
            expr(2),
        );

        let composed = compose_handlers(&h1, &h2)
            .expect("bare-tail residual on h2 absorbs h1's concrete residual");
        assert_eq!(composed.residual, concrete_residual);
    }
}
