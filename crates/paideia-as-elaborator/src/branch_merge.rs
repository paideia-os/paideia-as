//! Branch-merge for substructural use in `if` and `match`.
//!
//! When the elaborator forks the [`LinearityCtx`] for the arms of an
//! `if` / `match`, the resulting per-arm scopes must agree on the use
//! count of every linear (or ordered) binding before they can be
//! joined back. Mismatch emits `S0906` (linearity violation in
//! branch).
//!
//! Phase-1 rule: every arm must have the *same* use-count for each
//! linear binding. Affine bindings tolerate use-count divergence as
//! long as no arm overuses (those checks are the per-arm scope-leave
//! validation in `check_linearity`).
//!
//! [`LinearityCtx`]: crate::linearity_ctx::LinearityCtx

use std::collections::HashMap;

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_ir::LinClass;

use crate::env::Symbol;
use crate::linearity_ctx::Binding;

/// Diagnostic code for branch-use mismatch.
pub const S_BRANCH_MISMATCH: u16 = 906;

/// Compute the joined binding table from `arms` and any diagnostics
/// that arose from disagreement on linear/ordered use counts.
///
/// Each entry in `arms` is the popped scope of one branch (as returned
/// by [`crate::linearity_ctx::LinearityCtx::leave_scope`]). The arms must
/// share the same keyspace — i.e. every symbol bound before the fork
/// appears in every arm. Symbols introduced inside a single arm are
/// ignored by the merge (they go out of scope along with their arm).
///
/// Returns `(joined, diagnostics)`. The joined table uses the maximum
/// use-count across arms for each symbol, so the caller's outer
/// context reflects "at least one branch used it this many times".
#[must_use]
pub fn merge_branches(
    arms: &[HashMap<Symbol, Binding>],
) -> (HashMap<Symbol, Binding>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut joined: HashMap<Symbol, Binding> = HashMap::new();
    if arms.is_empty() {
        return (joined, diags);
    }

    // The first arm seeds the symbol set.
    for (sym, b) in &arms[0] {
        joined.insert(*sym, *b);
    }

    for arm in arms.iter().skip(1) {
        // Drop symbols not present in this arm — the merge must
        // intersect the keyspaces, dropping anything that's
        // out-of-scope along any branch.
        joined.retain(|sym, _| arm.contains_key(sym));

        for (sym, b) in arm {
            if let Some(existing) = joined.get_mut(sym) {
                // Phase-1 rule: linear/ordered bindings must agree
                // on use count across all arms.
                let must_agree = matches!(b.class, LinClass::Linear | LinClass::Ordered);
                if must_agree && existing.uses != b.uses {
                    diags.push(
                        Diagnostic::error(s_code(S_BRANCH_MISMATCH))
                            .message(format!(
                                "branches disagree on uses of {:?} binding: {} vs {}",
                                b.class, existing.uses, b.uses
                            ))
                            .with_span(b.bind_span)
                            .finish(),
                    );
                }
                // Joined count is the max — represents "any branch
                // could have used it this many times".
                if b.uses > existing.uses {
                    existing.uses = b.uses;
                }
            }
            // Symbol only in later arm — dropped by the intersection
            // above, do nothing.
        }
    }

    // Determinism: sort diagnostics by the span byte_start of their
    // primary location.
    diags.sort_by_key(|d| d.primary_span().map(|s| s.byte_start()).unwrap_or(0));
    (joined, diags)
}

fn s_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::S, Severity::Error, n).expect("valid S code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    fn binding(class: LinClass, uses: u32) -> Binding {
        Binding {
            class,
            uses,
            bind_span: span(0),
        }
    }

    fn one_arm(sym: Symbol, b: Binding) -> HashMap<Symbol, Binding> {
        let mut m = HashMap::new();
        m.insert(sym, b);
        m
    }

    // ── AC bullets ────────────────────────────────────────────────────

    #[test]
    fn linear_used_in_both_branches_joins_clean() {
        // if cond { consume(cap) } else { consume(cap) }
        let cap: Symbol = 7;
        let then_scope = one_arm(cap, binding(LinClass::Linear, 1));
        let else_scope = one_arm(cap, binding(LinClass::Linear, 1));
        let (joined, diags) = merge_branches(&[then_scope, else_scope]);
        assert!(diags.is_empty());
        assert_eq!(joined[&cap].uses, 1);
    }

    #[test]
    fn linear_used_in_one_branch_only_emits_s0906() {
        // if cond { consume(cap) } else { /* nothing */ }
        let cap: Symbol = 7;
        let then_scope = one_arm(cap, binding(LinClass::Linear, 1));
        let else_scope = one_arm(cap, binding(LinClass::Linear, 0));
        let (joined, diags) = merge_branches(&[then_scope, else_scope]);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 906);
        // Joined uses = max(1, 0) = 1 (so the outer context sees it
        // as used; only the mismatch diagnostic fires).
        assert_eq!(joined[&cap].uses, 1);
    }

    #[test]
    fn match_with_three_arms_uniformly_consumes() {
        // match x { 0 => consume(cap), 1 => consume(cap), _ => consume(cap) }
        let cap: Symbol = 7;
        let arms = vec![
            one_arm(cap, binding(LinClass::Linear, 1)),
            one_arm(cap, binding(LinClass::Linear, 1)),
            one_arm(cap, binding(LinClass::Linear, 1)),
        ];
        let (_joined, diags) = merge_branches(&arms);
        assert!(diags.is_empty());
    }

    #[test]
    fn nested_if_composes_via_pre_join() {
        // Outer if A else B; the A arm itself is a join of (inner-then,
        // inner-else). Phase-1: simulate by joining the inner arms,
        // feeding the joined result as the outer "then" scope.
        let cap: Symbol = 7;
        let inner_then = one_arm(cap, binding(LinClass::Linear, 1));
        let inner_else = one_arm(cap, binding(LinClass::Linear, 1));
        let (inner_joined, inner_diags) = merge_branches(&[inner_then, inner_else]);
        assert!(inner_diags.is_empty());

        let outer_else = one_arm(cap, binding(LinClass::Linear, 1));
        let (outer_joined, outer_diags) = merge_branches(&[inner_joined, outer_else]);
        assert!(outer_diags.is_empty());
        assert_eq!(outer_joined[&cap].uses, 1);
    }

    // ── Class-specific behavior ───────────────────────────────────────

    #[test]
    fn affine_disagreement_is_not_an_error() {
        // Affine: 1 use in one branch, 0 in another — fine.
        let cap: Symbol = 7;
        let arms = vec![
            one_arm(cap, binding(LinClass::Affine, 1)),
            one_arm(cap, binding(LinClass::Affine, 0)),
        ];
        let (joined, diags) = merge_branches(&arms);
        assert!(diags.is_empty());
        assert_eq!(joined[&cap].uses, 1);
    }

    #[test]
    fn unrestricted_disagreement_is_not_an_error() {
        let cap: Symbol = 7;
        let arms = vec![
            one_arm(cap, binding(LinClass::Unrestricted, 3)),
            one_arm(cap, binding(LinClass::Unrestricted, 0)),
        ];
        let (joined, diags) = merge_branches(&arms);
        assert!(diags.is_empty());
        assert_eq!(joined[&cap].uses, 3);
    }

    #[test]
    fn ordered_disagreement_emits_s0906_like_linear() {
        let cap: Symbol = 7;
        let arms = vec![
            one_arm(cap, binding(LinClass::Ordered, 1)),
            one_arm(cap, binding(LinClass::Ordered, 0)),
        ];
        let (_joined, diags) = merge_branches(&arms);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 906);
    }

    #[test]
    fn empty_arms_list_returns_empty_result() {
        let (joined, diags) = merge_branches(&[]);
        assert!(joined.is_empty());
        assert!(diags.is_empty());
    }

    #[test]
    fn arm_only_symbols_are_dropped_from_join() {
        // Symbol bound only in one branch goes out of scope at the
        // join point — it does NOT appear in the joined map.
        let cap: Symbol = 7;
        let inner_only: Symbol = 99;
        let then_scope = {
            let mut m = HashMap::new();
            m.insert(cap, binding(LinClass::Linear, 1));
            m.insert(inner_only, binding(LinClass::Linear, 1));
            m
        };
        let else_scope = one_arm(cap, binding(LinClass::Linear, 1));
        let (joined, diags) = merge_branches(&[then_scope, else_scope]);
        assert!(diags.is_empty());
        assert!(joined.contains_key(&cap));
        assert!(!joined.contains_key(&inner_only));
    }
}
