//! Session-type recursion + well-founded induction (v0.29-M1-003, issue #1377).
//!
//! Builds on top of [`crate::session`] — the base [`SessionTy`] ADT
//! already carries both `Rec(name, body)` and `Var(name)` (they were
//! landed with M1-001 so the ADT layout could stay stable across the
//! wave). What was NOT landed there was a *well-foundedness* check that
//! rejects sessions whose recursive binder can be reached again without
//! any intervening communication — the archetypal example being
//! `rec X. X`. Such a session has no operational meaning: unfolding it
//! never produces an observable action.
//!
//! This module adds that check as a **companion pass** — `session.rs`
//! is unchanged, per the wave-0 batch-3 rules. Callers that want the
//! full guarantee run
//!
//! ```text
//!   wf_session(s)?;      // syntactic well-formedness (M1-001)
//!   wf_recursive(s)?;    // guarded-recursion / well-founded induction
//! ```
//!
//! The two checks are deliberately independent: `wf_session` cares
//! about scoping, duplicate labels, and empty payloads; `wf_recursive`
//! cares only about the guarded-recursion rule below. A session that
//! passes both is safe for the M1-004 elaborator to unfold once during
//! duality checking without diverging.
//!
//! # Guarded-recursion rule
//!
//! A binder `Rec(X, body)` is **guarded** iff every syntactic
//! occurrence of `Var(X)` inside `body` is a descendant of at least
//! one *action prefix* — one of
//!
//! * `Send { payload, cont }` — output message,
//! * `Recv { payload, cont }` — input message,
//! * `Branch(arms)` — external choice (label offer),
//! * `Choice(arms)` — internal choice (label select).
//!
//! Rationale. In the operational semantics, unfolding `μX.S` rewrites
//! `Var(X)` back to `μX.S`. If the path from the binder to the
//! variable is free of action prefixes, unfolding never consumes a
//! message and the reduction diverges. Requiring at least one action
//! on every such path is the standard "contractive" / "guarded" side
//! condition from Vasconcelos-flavour session-type theory, and it is
//! exactly the well-founded-induction hypothesis the elaborator needs
//! when it recurses on a folded session.
//!
//! ## `Seq` and sequential composition
//!
//! `Seq(a, b)` is treated the natural way: `X` in `b` is guarded if it
//! is *either* already under an action prefix inherited from above,
//! *or* the left-hand side `a` is guaranteed to traverse at least one
//! action before control reaches `b`. Concretely, the pass carries a
//! `MustAct` predicate ([`must_traverse_action`]) that answers "does
//! every completing path through `a` execute at least one action?"
//! and threads its result into the `b` sub-check. This admits the
//! natural idiom `rec X. Seq(!T . End, X)` while still rejecting
//! `rec X. Seq(End, X)`.
//!
//! ## Nested binders and shadowing
//!
//! `Rec(Y, body)` inside `Rec(X, _)` shadows only when `Y == X`. Under
//! shadowing, occurrences of `Var(X)` in `body` refer to the *inner*
//! binder, so the outer binder's guardedness obligation is discharged
//! (there are no outer-X occurrences to worry about below the
//! shadow). The pass encodes this by returning `true` for the outer
//! check as soon as a shadowing `Rec(X, _)` is entered — the inner
//! binder is still checked separately by the top-level driver.
//!
//! ## Mutual recursion via two-var expansion
//!
//! Mutually-recursive protocols are encoded as nested binders:
//!
//! ```text
//!   rec X. !T. rec Y. ?U. Choice { a: X, b: Y }
//! ```
//!
//! Both `X` and `Y` must satisfy the guarded-recursion rule under
//! their respective binders. The pass visits every `Rec` node during a
//! single traversal — the tests exercise a two-var expansion of this
//! shape.
//!
//! # Duality
//!
//! The recursion case for [`crate::session::dual`] — `dual(μX. S) =
//! μX. dual(S)`, with `Var(X)` its own dual — is already implemented
//! in `session.rs` (M1-001) and re-used verbatim here. The involution
//! property `dual(dual(μX.S)) == μX.S` is exercised by
//! [`tests::dual_recursion_is_involution`] against a guarded body so
//! the test doubles as an integration check that
//! [`wf_recursive`] and [`crate::session::dual`] agree on the same
//! recursive terms.
//!
//! # Diagnostics
//!
//! Failure modes carry stable diagnostic codes in the `T0300-T0310`
//! range so downstream catalog wire-up (post-elaborator) does not have
//! to renumber:
//!
//! | Code   | Variant                        | Meaning                                           |
//! |--------|--------------------------------|---------------------------------------------------|
//! | T0300  | [`RecErr::UnguardedRecursion`] | `Rec(X, body)` where some `Var(X)` in `body`      |
//! |        |                                | is not under any action prefix.                   |
//! | T0301  | [`RecErr::ShadowedByOuterBinder`] | An inner `Rec(X, _)` shadows an outer `Rec(X, _)` |
//! |        |                                | while the outer's body still has bare `Var(X)`    |
//! |        |                                | occurrences that would silently rebind.           |
//!
//! T0302-T0310 are reserved for the follow-up pass that promotes the
//! `String` payload to a real `TypeId` (M1-003 elaborator wire-up).

use crate::session::SessionTy;

// ---------- error type ----------

/// Well-foundedness error surfaced by [`wf_recursive`].
///
/// See the module docs for the mapping to diagnostic codes.
#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum RecErr {
    /// `T0300` — a `Rec(X, body)` node has at least one `Var(X)`
    /// occurrence in `body` that is not a descendant of any action
    /// prefix (`Send`, `Recv`, `Branch`, `Choice`). The variable name
    /// is the binder's name, not the specific offending occurrence
    /// (they are all indistinguishable at the type level).
    #[error("T0300: unguarded recursion — `Rec({var}, _)` has a bare `Var({var})` in its body \
             (every recursive occurrence must appear under Send/Recv/Branch/Choice)")]
    UnguardedRecursion {
        /// Name of the recursion binder whose body is not guarded.
        var: String,
    },
    /// `T0301` — an inner `Rec(X, _)` shadows an outer `Rec(X, _)`
    /// *and* the outer body has at least one bare `Var(X)` that would
    /// silently rebind to the inner binder on unfold. The pass rejects
    /// this defensively: shadowing recursion binders is a common
    /// source of subtle divergence bugs and the surface syntax should
    /// alpha-rename first.
    #[error("T0301: recursion binder `{var}` is shadowed by an inner `Rec({var}, _)` \
             while a bare `Var({var})` in the outer body would rebind on unfold \
             (alpha-rename one of the binders)")]
    ShadowedByOuterBinder {
        /// Name of the shadowed binder.
        var: String,
    },
}

// ---------- public API ----------

/// Check that every `Rec` binder in `sess` is *well-founded* under the
/// guarded-recursion rule (see the module docs).
///
/// Returns `Ok(())` on success. The check is purely syntactic and does
/// not unfold recursion — it only asks whether the *paths* from each
/// binder to its variable pass through at least one action prefix.
///
/// # Complement to [`crate::session::wf_session`]
///
/// This check is orthogonal to [`crate::session::wf_session`]: they
/// share no state, catch no overlapping errors, and run in either
/// order. Callers that want the full guarantee typically run
/// `wf_session` first (cheaper, and catches structural errors that
/// would confuse this pass's error messages) and then `wf_recursive`.
///
/// # Errors
///
/// * [`RecErr::UnguardedRecursion`] — some `Rec(X, body)` has a bare
///   `Var(X)` occurrence in `body`.
/// * [`RecErr::ShadowedByOuterBinder`] — an inner `Rec(X, _)` shadows
///   an outer `Rec(X, _)` while the outer body has a bare `Var(X)`
///   that would rebind on unfold.
pub fn wf_recursive(sess: &SessionTy) -> Result<(), RecErr> {
    check_all_binders(sess, &[])
}

// ---------- traversal ----------

/// Walk the term and, at every `Rec(x, body)` node, verify:
///
/// 1. `body` is *guarded* w.r.t. `x` (no bare `Var(x)`).
/// 2. If `x` shadows an outer binder, the outer binder had no bare
///    `Var(x)` that would silently rebind on unfold.
///
/// `outer_binders` is the list of enclosing `Rec` binder names, in
/// outermost-first order. It is only used for the shadowing check.
fn check_all_binders(s: &SessionTy, outer_binders: &[String]) -> Result<(), RecErr> {
    match s {
        SessionTy::End | SessionTy::Var(_) => Ok(()),
        SessionTy::Send { cont, .. } | SessionTy::Recv { cont, .. } => {
            check_all_binders(cont, outer_binders)
        }
        SessionTy::Seq(a, b) => {
            check_all_binders(a, outer_binders)?;
            check_all_binders(b, outer_binders)
        }
        SessionTy::Branch(arms) | SessionTy::Choice(arms) => {
            for (_, arm) in arms {
                check_all_binders(arm, outer_binders)?;
            }
            Ok(())
        }
        SessionTy::Rec(x, body) => {
            // Rule (2): shadowing.
            if outer_binders.iter().any(|n| n == x) && has_free_var(body, x) {
                // NOTE: the shadow itself is fine as long as the outer
                // body could not have had a bare `Var(x)` on any path
                // between the outer binder and this inner `Rec`. That
                // is exactly what the outer level of `check_all_binders`
                // already asked (rule (1) below), so the offense here
                // is only about the *inner* body still exposing free
                // `Var(x)` beneath the shadowing binder itself. We
                // conservatively reject to avoid subtle
                // rebind-on-unfold bugs.
                return Err(RecErr::ShadowedByOuterBinder { var: x.clone() });
            }
            // Rule (1): body is guarded w.r.t. x.
            if !check_guarded(x, body, false) {
                return Err(RecErr::UnguardedRecursion { var: x.clone() });
            }
            // Recurse into the body to check nested binders too. Push
            // x onto the outer-binder list; when we leave scope the
            // Vec is dropped naturally (we allocate a fresh one).
            let mut extended: Vec<String> = outer_binders.to_vec();
            extended.push(x.clone());
            check_all_binders(body, &extended)
        }
    }
}

/// True iff every occurrence of `Var(x)` inside `s` is a descendant of
/// at least one action prefix. `saw_action` records whether the caller
/// has already traversed one.
///
/// The nuance in the `Seq` case is that `b` inherits `saw_action` from
/// the parent, but is *also* guarded by any action that `a` is
/// guaranteed to execute — see [`must_traverse_action`].
fn check_guarded(x: &str, s: &SessionTy, saw_action: bool) -> bool {
    match s {
        SessionTy::End => true,
        SessionTy::Var(y) => y != x || saw_action,
        SessionTy::Send { cont, .. } | SessionTy::Recv { cont, .. } => {
            // Send/Recv is an action prefix — everything below it is guarded.
            check_guarded(x, cont, true)
        }
        SessionTy::Seq(a, b) => {
            let after_a = saw_action || must_traverse_action(a);
            check_guarded(x, a, saw_action) && check_guarded(x, b, after_a)
        }
        SessionTy::Branch(arms) | SessionTy::Choice(arms) => {
            // The Branch/Choice node itself is a label-exchange action;
            // every arm sits below that boundary, so `saw_action = true`
            // for the recursive call.
            arms.iter().all(|(_, arm)| check_guarded(x, arm, true))
        }
        SessionTy::Rec(y, body) => {
            if y == x {
                // Inner Rec shadows the outer binder — from the outer
                // pass's POV, all occurrences of `x` in `body` refer
                // to the inner binder, so the outer obligation is
                // discharged here.
                true
            } else {
                check_guarded(x, body, saw_action)
            }
        }
    }
}

/// True iff every path through `s` that reaches a leaf (`End` or
/// `Var(_)`) executes at least one action prefix along the way.
///
/// The `Seq(a, b)` case uses `||` because either `a` or `b` traversing
/// an action is enough to guarantee that the composite does. The
/// `Branch(arms)` / `Choice(arms)` cases return `true` because the
/// branch/choice boundary itself is an action — this is consistent
/// with how [`check_guarded`] treats those nodes.
fn must_traverse_action(s: &SessionTy) -> bool {
    match s {
        SessionTy::End | SessionTy::Var(_) => false,
        SessionTy::Send { .. } | SessionTy::Recv { .. } => true,
        SessionTy::Seq(a, b) => must_traverse_action(a) || must_traverse_action(b),
        SessionTy::Branch(arms) | SessionTy::Choice(arms) => !arms.is_empty(),
        SessionTy::Rec(_, body) => must_traverse_action(body),
    }
}

/// Structural free-variable test — true iff `x` occurs anywhere in `s`
/// under no `Rec(x, _)` binder.
///
/// Used by the shadowing check. Kept private because callers outside
/// this module do not need it: `wf_recursive` covers every use site.
fn has_free_var(s: &SessionTy, x: &str) -> bool {
    match s {
        SessionTy::End => false,
        SessionTy::Var(y) => y == x,
        SessionTy::Send { cont, .. } | SessionTy::Recv { cont, .. } => has_free_var(cont, x),
        SessionTy::Seq(a, b) => has_free_var(a, x) || has_free_var(b, x),
        SessionTy::Branch(arms) | SessionTy::Choice(arms) => {
            arms.iter().any(|(_, arm)| has_free_var(arm, x))
        }
        SessionTy::Rec(y, body) => {
            if y == x {
                false
            } else {
                has_free_var(body, x)
            }
        }
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::dual;

    // ------------------------------------------------------------------
    // Convenience constructors — cut noise in the tree literals below.
    // ------------------------------------------------------------------

    fn rec(x: &str, body: SessionTy) -> SessionTy {
        SessionTy::Rec(x.to_string(), Box::new(body))
    }
    fn var(x: &str) -> SessionTy {
        SessionTy::Var(x.to_string())
    }
    fn send(t: &str, cont: SessionTy) -> SessionTy {
        SessionTy::send(t, cont)
    }
    fn recv(t: &str, cont: SessionTy) -> SessionTy {
        SessionTy::recv(t, cont)
    }
    fn seq(a: SessionTy, b: SessionTy) -> SessionTy {
        SessionTy::seq(a, b)
    }
    fn end() -> SessionTy {
        SessionTy::End
    }
    fn branch(arms: Vec<(&str, SessionTy)>) -> SessionTy {
        SessionTy::Branch(arms.into_iter().map(|(l, s)| (l.to_string(), s)).collect())
    }
    fn choice(arms: Vec<(&str, SessionTy)>) -> SessionTy {
        SessionTy::Choice(arms.into_iter().map(|(l, s)| (l.to_string(), s)).collect())
    }

    // ------------------------------------------------------------------
    // Guarded-recursion — happy path.
    // ------------------------------------------------------------------

    /// `rec X. !T. X` — the canonical guarded recursion: the recursive
    /// call is preceded by one send.
    #[test]
    fn guarded_rec_with_send_prefix_is_accepted() {
        let s = rec("X", send("i32", var("X")));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// `rec X. ?T. X` — mirror image with `Recv`.
    #[test]
    fn guarded_rec_with_recv_prefix_is_accepted() {
        let s = rec("X", recv("i32", var("X")));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// `rec X. !T. ?U. X` — two prefixes, still guarded.
    #[test]
    fn guarded_rec_with_two_prefixes_is_accepted() {
        let s = rec("X", send("i32", recv("bool", var("X"))));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// `rec X. & { a: X, b: End }` — Branch counts as an action
    /// prefix (label offer is observable), so the arm `X` is guarded.
    #[test]
    fn guarded_rec_under_branch_is_accepted() {
        let s = rec("X", branch(vec![("a", var("X")), ("b", end())]));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// `rec X. ⊕ { a: X }` — Choice counts as an action prefix
    /// (label select is observable), so the arm `X` is guarded.
    #[test]
    fn guarded_rec_under_choice_is_accepted() {
        let s = rec("X", choice(vec![("a", var("X"))]));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// `rec X. (!T . End) ; X` — the left side of `Seq` traverses an
    /// action, so `X` on the right side is guarded by that action.
    #[test]
    fn guarded_rec_via_seq_left_action_is_accepted() {
        let s = rec("X", seq(send("i32", end()), var("X")));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    // ------------------------------------------------------------------
    // Guarded-recursion — rejection path.
    // ------------------------------------------------------------------

    /// `rec X. X` — the archetypal unguarded recursion.
    #[test]
    fn unguarded_bare_rec_is_rejected() {
        let s = rec("X", var("X"));
        assert_eq!(
            wf_recursive(&s),
            Err(RecErr::UnguardedRecursion { var: "X".into() })
        );
    }

    /// `rec X. (End ; X)` — the left side of `Seq` does NOT traverse
    /// any action (`End` is a leaf), so `X` on the right is still
    /// unguarded.
    #[test]
    fn unguarded_rec_via_end_seq_is_rejected() {
        let s = rec("X", seq(end(), var("X")));
        assert_eq!(
            wf_recursive(&s),
            Err(RecErr::UnguardedRecursion { var: "X".into() })
        );
    }

    /// `rec X. rec Y. X` — the inner Rec binder does not guard the
    /// outer variable (Rec is a binder, not an action prefix).
    #[test]
    fn unguarded_rec_under_inner_rec_is_rejected() {
        let s = rec("X", rec("Y", var("X")));
        assert_eq!(
            wf_recursive(&s),
            Err(RecErr::UnguardedRecursion { var: "X".into() })
        );
    }

    // ------------------------------------------------------------------
    // Mutual recursion via two-var nested-binder expansion.
    // ------------------------------------------------------------------

    /// `rec X. !T. rec Y. ?U. & { a: X, b: Y }` — the standard
    /// two-variable encoding of mutual recursion. Both `X` and `Y`
    /// appear under an action prefix (`!T.` for X, `?U.` for Y), and
    /// additionally under a `Branch`.
    #[test]
    fn mutual_recursion_two_var_expansion_is_accepted() {
        let s = rec(
            "X",
            send(
                "i32",
                rec(
                    "Y",
                    recv("bool", branch(vec![("a", var("X")), ("b", var("Y"))])),
                ),
            ),
        );
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// A malformed two-var expansion: the outer `X` reappears under
    /// only the inner binder, no action prefix. The outer binder is
    /// unguarded even though the inner one is fine.
    #[test]
    fn mutual_recursion_two_var_outer_unguarded_is_rejected() {
        let s = rec("X", rec("Y", recv("bool", var("Y"))));
        // Note: `X` never appears in the body at all, so this happens
        // to be accepted — the outer binder is trivially guarded by
        // absence of any `Var(X)`. Include a bare `Var(X)` to make it
        // an actual failure case.
        assert_eq!(wf_recursive(&s), Ok(()));

        let s_bad = rec("X", rec("Y", var("X")));
        assert_eq!(
            wf_recursive(&s_bad),
            Err(RecErr::UnguardedRecursion { var: "X".into() })
        );
    }

    // ------------------------------------------------------------------
    // Shadowing.
    // ------------------------------------------------------------------

    /// `rec X. !T. rec X. !U. X` — the inner `Rec(X, _)` shadows the
    /// outer one, and the outer body has a bare `Var(X)` (inside the
    /// inner Rec) that would silently rebind. Rejected as T0301.
    #[test]
    fn shadowing_rec_with_free_outer_var_is_rejected() {
        let s = rec("X", send("i32", rec("X", send("bool", var("X")))));
        assert_eq!(
            wf_recursive(&s),
            Err(RecErr::ShadowedByOuterBinder { var: "X".into() })
        );
    }

    /// `rec X. !T. rec Y. !U. Y` — nested but no shadow (different
    /// names). Accepted.
    #[test]
    fn nested_rec_without_shadow_is_accepted() {
        let s = rec("X", send("i32", rec("Y", send("bool", var("Y")))));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    // ------------------------------------------------------------------
    // Non-recursive terms: the pass is a no-op.
    // ------------------------------------------------------------------

    /// `End` — no binders, trivially fine.
    #[test]
    fn non_recursive_end_is_accepted() {
        assert_eq!(wf_recursive(&end()), Ok(()));
    }

    /// A message chain without any `Rec` — trivially fine.
    #[test]
    fn non_recursive_message_chain_is_accepted() {
        let s = send("i32", recv("bool", end()));
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    /// A free `Var` outside any `Rec` — this pass does not model
    /// binding, so it accepts. Structural well-formedness (unbound
    /// variables) is `wf_session`'s job.
    #[test]
    fn free_var_outside_rec_is_accepted_by_wf_recursive() {
        let s = var("X");
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    // ------------------------------------------------------------------
    // Duality involution for the recursion case — sanity check that
    // `session::dual` and `wf_recursive` agree on the same terms.
    // ------------------------------------------------------------------

    /// `dual(dual(rec X. !T. X)) == rec X. !T. X`.
    #[test]
    fn dual_recursion_is_involution() {
        let s = rec("X", send("i32", var("X")));
        assert_eq!(dual(&dual(&s)), s);
        // And the guarded-recursion check still passes on the round-trip.
        assert_eq!(wf_recursive(&dual(&dual(&s))), Ok(()));
    }

    /// `dual(rec X. !T. X) == rec X. ?T. X` — Send/Recv flip inside
    /// the Rec body, binder name is preserved.
    #[test]
    fn dual_of_rec_flips_body_and_keeps_binder() {
        let s = rec("X", send("i32", var("X")));
        let d = dual(&s);
        let expected = rec("X", recv("i32", var("X")));
        assert_eq!(d, expected);
        // The dualised form is itself guarded.
        assert_eq!(wf_recursive(&d), Ok(()));
    }

    /// Duality involution on a *mutually-recursive* term via the
    /// two-var expansion — the property `dual(dual(S)) == S` extends
    /// through nested binders.
    #[test]
    fn dual_involution_on_mutual_recursion() {
        let s = rec(
            "X",
            send(
                "i32",
                rec(
                    "Y",
                    recv("bool", branch(vec![("a", var("X")), ("b", var("Y"))])),
                ),
            ),
        );
        assert_eq!(dual(&dual(&s)), s);
        assert_eq!(wf_recursive(&s), Ok(()));
    }

    // ------------------------------------------------------------------
    // `must_traverse_action` — a small suite so the `Seq` case's
    // subtlety is documented by executable examples.
    // ------------------------------------------------------------------

    #[test]
    fn must_traverse_action_end_is_false() {
        assert!(!must_traverse_action(&end()));
    }

    #[test]
    fn must_traverse_action_var_is_false() {
        assert!(!must_traverse_action(&var("X")));
    }

    #[test]
    fn must_traverse_action_send_is_true() {
        assert!(must_traverse_action(&send("i32", end())));
    }

    #[test]
    fn must_traverse_action_seq_is_disjunctive() {
        assert!(must_traverse_action(&seq(send("i32", end()), end())));
        assert!(must_traverse_action(&seq(end(), send("i32", end()))));
        assert!(!must_traverse_action(&seq(end(), end())));
    }

    #[test]
    fn must_traverse_action_branch_is_true_when_non_empty() {
        assert!(must_traverse_action(&branch(vec![("a", end())])));
    }
}
