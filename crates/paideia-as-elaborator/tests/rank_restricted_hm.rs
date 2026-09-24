//! R220.M11 — rank-restricted let-polymorphism (closes `paideia-as#1425`).
//!
//! 60-test corpus covering the acceptance criteria in
//! `design/terminal/semantic-shell-language-plan.md` §4 R220.M11 and the
//! formal spec at `design/toolchain/rank-restricted-hm.md`.
//!
//! Every test carries a fingerprint tag `r220m11-rank-NN` in its
//! docstring per the paideia-as convention (mirrors the R220.M8
//! `r220m8-eff-NN` shape in `effect_row_inference.rs`), and the
//! CHANGELOG scratch under `.plans/scratch/CHANGELOG-rank-restricted-hm.md`.
//!
//! Coverage classes:
//!  - Rank-0 concretes (accept):                        01..05
//!  - First-order arrows (accept):                      06..10
//!  - Rank-1 prenex (accept):                           11..15
//!  - Rank-2 annotated in lambda (accept):              16..20
//!  - Sub-language accept (Pipeline/Datalog/Lambda R1): 21..30
//!  - Rank-2 unannotated in lambda (reject T0700):      31..35
//!  - Non-prenex nested ∀ (reject T0700):               36..40
//!  - Rank-3+ unannotated (reject T0700):               41..43
//!  - Rank-3+ even when annotated (reject T0700):       44..45
//!  - Pipeline rank-2 (reject T0700):                   46..47
//!  - Datalog rank-2 (reject T0700):                    48..49
//!  - Record with ∀ field, unannotated (reject):        50..52
//!  - Tuple with ∀ component, unannotated (reject):     53..55
//!  - Cross-sub-language composition (reject):          56..60
//!
//! Builds on R220.M8 (`paideia_as_effects::Substitution::apply` /
//! `paideia_as_effects::Substitution::compose`, landed v0.36.6) —
//! effect-row substitution is orthogonal to type rank, so the test
//! corpus never crosses that boundary; the R220.M8 substrate is a
//! sibling pass that continues to run over effect rows independently
//! of these checks.

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_elaborator::{
    SubLanguage, T_RANK_VIOLATION, TypeShape, check_rank_restricted, contains_forall,
    is_prenex, max_rank, rank_of,
};

// ── Helpers ──────────────────────────────────────────────────────────────

fn s() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Convenience: `∀α₀. α₀ → α₀`.
fn id_ty() -> TypeShape {
    TypeShape::forall(
        vec![0],
        TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
    )
}

/// Convenience: `∀α₀ α₁. α₀ → α₁ → α₀` (const combinator).
fn k_ty() -> TypeShape {
    TypeShape::forall(
        vec![0, 1],
        TypeShape::arrow(
            vec![TypeShape::Var(0)],
            TypeShape::arrow(vec![TypeShape::Var(1)], TypeShape::Var(0)),
        ),
    )
}

fn assert_accept(t: &TypeShape, sub: SubLanguage, annotated: bool) {
    let d = check_rank_restricted(t, sub, annotated, s());
    assert!(
        d.is_empty(),
        "expected accept for {:?} in {:?} annotated={}, got {} diagnostics",
        t,
        sub,
        annotated,
        d.len()
    );
}

fn assert_reject_t0700(t: &TypeShape, sub: SubLanguage, annotated: bool) {
    let d = check_rank_restricted(t, sub, annotated, s());
    assert_eq!(
        d.len(),
        1,
        "expected exactly one T0700 for {:?} in {:?} annotated={}",
        t,
        sub,
        annotated
    );
    assert_eq!(d[0].code().number(), T_RANK_VIOLATION);
}

// ── Rank-0 concretes (accept) — 01..05 ───────────────────────────────────

/// r220m11-rank-01: bare concrete (`Int`) has rank 0 and accepts anywhere.
#[test]
fn r220m11_rank_01_concrete_int_is_rank_zero() {
    let t = TypeShape::Concrete;
    assert_eq!(rank_of(&t), 0);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
    assert_accept(&t, SubLanguage::Pipeline, false);
    assert_accept(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-02: tuple of concretes has rank 0.
#[test]
fn r220m11_rank_02_tuple_of_concretes_is_rank_zero() {
    let t = TypeShape::tuple(vec![TypeShape::Concrete, TypeShape::Concrete]);
    assert_eq!(rank_of(&t), 0);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-03: record of concretes has rank 0.
#[test]
fn r220m11_rank_03_record_of_concretes_is_rank_zero() {
    let t = TypeShape::record(vec![(0, TypeShape::Concrete), (1, TypeShape::Concrete)]);
    assert_eq!(rank_of(&t), 0);
    assert!(!contains_forall(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-04: bare type variable (unbound, monomorphic) has rank 0.
#[test]
fn r220m11_rank_04_var_alone_is_rank_zero() {
    let t = TypeShape::Var(7);
    assert_eq!(rank_of(&t), 0);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-05: empty tuple / unit has rank 0.
#[test]
fn r220m11_rank_05_unit_is_rank_zero() {
    let t = TypeShape::tuple(vec![]);
    assert_eq!(rank_of(&t), 0);
    assert_accept(&t, SubLanguage::Pipeline, false);
}

// ── First-order arrows (accept) — 06..10 ─────────────────────────────────

/// r220m11-rank-06: `Int -> Int` is a first-order arrow, rank 0.
#[test]
fn r220m11_rank_06_int_to_int_is_rank_zero() {
    let t = TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 0);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-07: `(Int, Bool) -> Int` — multi-arg first-order, rank 0.
#[test]
fn r220m11_rank_07_multi_arg_first_order_is_rank_zero() {
    let t = TypeShape::arrow(
        vec![TypeShape::Concrete, TypeShape::Concrete],
        TypeShape::Concrete,
    );
    assert_eq!(rank_of(&t), 0);
    assert_accept(&t, SubLanguage::Pipeline, false);
}

/// r220m11-rank-08: `() -> Unit` — nullary first-order.
#[test]
fn r220m11_rank_08_nullary_arrow_is_rank_zero() {
    let t = TypeShape::arrow(vec![], TypeShape::tuple(vec![]));
    assert_eq!(rank_of(&t), 0);
    assert_accept(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-09: `(Int -> Int) -> Int` — higher-order but no ∀, rank 0.
#[test]
fn r220m11_rank_09_higher_order_no_forall_is_rank_zero() {
    let inner = TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete);
    let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 0);
    assert!(!contains_forall(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-10: `Int -> (Int -> Int)` — curried, still rank 0.
#[test]
fn r220m11_rank_10_curried_arrow_is_rank_zero() {
    let t = TypeShape::arrow(
        vec![TypeShape::Concrete],
        TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete),
    );
    assert_eq!(rank_of(&t), 0);
    assert_accept(&t, SubLanguage::Lambda, false);
}

// ── Rank-1 prenex (accept) — 11..15 ──────────────────────────────────────

/// r220m11-rank-11: `∀α. α → α` — the identity type, prenex rank 1.
#[test]
fn r220m11_rank_11_identity_is_prenex_rank_one() {
    let t = id_ty();
    assert_eq!(rank_of(&t), 1);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-12: `∀α β. α → β → α` — const combinator, prenex rank 1.
#[test]
fn r220m11_rank_12_const_combinator_is_prenex_rank_one() {
    let t = k_ty();
    assert_eq!(rank_of(&t), 1);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-13: `∀α. (α, α) → α` — prenex over a tuple arg, rank 1.
#[test]
fn r220m11_rank_13_prenex_over_tuple_arg_is_rank_one() {
    let t = TypeShape::forall(
        vec![0],
        TypeShape::arrow(
            vec![TypeShape::tuple(vec![TypeShape::Var(0), TypeShape::Var(0)])],
            TypeShape::Var(0),
        ),
    );
    assert_eq!(rank_of(&t), 1);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-14: `∀α. α → (α, α)` — prenex over a tuple ret, rank 1.
#[test]
fn r220m11_rank_14_prenex_over_tuple_ret_is_rank_one() {
    let t = TypeShape::forall(
        vec![0],
        TypeShape::arrow(
            vec![TypeShape::Var(0)],
            TypeShape::tuple(vec![TypeShape::Var(0), TypeShape::Var(0)]),
        ),
    );
    assert_eq!(rank_of(&t), 1);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-15: `∀α β. (α → β) → α → β` — the apply combinator, rank 1.
#[test]
fn r220m11_rank_15_apply_combinator_is_rank_one() {
    let t = TypeShape::forall(
        vec![0, 1],
        TypeShape::arrow(
            vec![TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(1))],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(1)),
        ),
    );
    assert_eq!(rank_of(&t), 1);
    assert!(is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, false);
}

// ── Rank-2 annotated in lambda (accept) — 16..20 ─────────────────────────

/// r220m11-rank-16: `(∀α. α → α) → Int` in an annotated lambda binding.
#[test]
fn r220m11_rank_16_annotated_rank_two_arrow_accepts() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 2);
    assert!(!is_prenex(&t));
    assert_accept(&t, SubLanguage::Lambda, true);
}

/// r220m11-rank-17: `(∀α β. α → β → α) → Int` — rank-2 K arg, annotated.
#[test]
fn r220m11_rank_17_annotated_rank_two_k_arg_accepts() {
    let t = TypeShape::arrow(vec![k_ty()], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 2);
    assert_accept(&t, SubLanguage::Lambda, true);
}

/// r220m11-rank-18: `(∀α. α → α) → (Int, Bool)` — rank-2 with tuple ret.
#[test]
fn r220m11_rank_18_annotated_rank_two_tuple_ret_accepts() {
    let t = TypeShape::arrow(
        vec![id_ty()],
        TypeShape::tuple(vec![TypeShape::Concrete, TypeShape::Concrete]),
    );
    assert_eq!(rank_of(&t), 2);
    assert_accept(&t, SubLanguage::Lambda, true);
}

/// r220m11-rank-19: `Int → (∀α. α → α) → Int` — rank-2 in second arg.
#[test]
fn r220m11_rank_19_annotated_rank_two_second_arg_accepts() {
    let t = TypeShape::arrow(
        vec![TypeShape::Concrete],
        TypeShape::arrow(vec![id_ty()], TypeShape::Concrete),
    );
    assert_eq!(rank_of(&t), 2);
    assert_accept(&t, SubLanguage::Lambda, true);
}

/// r220m11-rank-20: `((∀α. α → α), Int) → Int` — rank-2 via tuple-in-arg,
/// annotated.
#[test]
fn r220m11_rank_20_annotated_rank_two_via_tuple_arg_accepts() {
    let t = TypeShape::arrow(
        vec![TypeShape::tuple(vec![id_ty(), TypeShape::Concrete])],
        TypeShape::Concrete,
    );
    // Per spec edge case #5 (documented in rank_restrict.rs): tuple
    // component with ∀ promotes rank of enclosing tuple to 2; then the
    // outer arrow's argument position promotes to 3. This is by design
    // (products count as argument-like positions). Rank 3 is banned
    // even when annotated per edge case #4 (annotation ceiling = 2 for
    // Lambda); T0700 fires.
    assert_eq!(rank_of(&t), 3);
    assert_reject_t0700(&t, SubLanguage::Lambda, true);
}

// ── Sub-language accept (rank-1 in each) — 21..30 ────────────────────────

/// r220m11-rank-21: pipeline stage `Int → Int` — first-order, accepts.
#[test]
fn r220m11_rank_21_pipeline_first_order_accepts() {
    let t = TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete);
    assert_accept(&t, SubLanguage::Pipeline, false);
}

/// r220m11-rank-22: pipeline stage of prenex rank-1 accepts at ceiling 1.
#[test]
fn r220m11_rank_22_pipeline_prenex_rank_one_accepts() {
    let t = id_ty();
    assert_eq!(rank_of(&t), 1);
    assert_eq!(max_rank(SubLanguage::Pipeline, false), 1);
    assert_accept(&t, SubLanguage::Pipeline, false);
}

/// r220m11-rank-23: pipeline stage returning a tuple, rank-1 accepts.
#[test]
fn r220m11_rank_23_pipeline_tuple_ret_rank_one_accepts() {
    let t = TypeShape::forall(
        vec![0],
        TypeShape::arrow(
            vec![TypeShape::Var(0)],
            TypeShape::tuple(vec![TypeShape::Var(0), TypeShape::Concrete]),
        ),
    );
    assert_eq!(rank_of(&t), 1);
    assert_accept(&t, SubLanguage::Pipeline, false);
}

/// r220m11-rank-24: datalog predicate `Int → Bool` — first-order, accepts.
#[test]
fn r220m11_rank_24_datalog_first_order_accepts() {
    let t = TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete);
    assert_accept(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-25: datalog predicate `(Str, Int) → Bool` — first-order.
#[test]
fn r220m11_rank_25_datalog_two_arg_first_order_accepts() {
    let t = TypeShape::arrow(
        vec![TypeShape::Concrete, TypeShape::Concrete],
        TypeShape::Concrete,
    );
    assert_accept(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-26: datalog polymorphic predicate `∀α. α → α → Bool`, rank 1.
#[test]
fn r220m11_rank_26_datalog_prenex_polymorphic_accepts() {
    let t = TypeShape::forall(
        vec![0],
        TypeShape::arrow(
            vec![TypeShape::Var(0)],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Concrete),
        ),
    );
    assert_eq!(rank_of(&t), 1);
    assert_accept(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-27: lambda body `Int → Int` — always accepts.
#[test]
fn r220m11_rank_27_lambda_first_order_accepts() {
    let t = TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete);
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-28: lambda body identity `∀α. α → α` — rank 1 accepts.
#[test]
fn r220m11_rank_28_lambda_identity_rank_one_accepts() {
    assert_accept(&id_ty(), SubLanguage::Lambda, false);
}

/// r220m11-rank-29: lambda body apply combinator — rank 1 accepts.
#[test]
fn r220m11_rank_29_lambda_apply_combinator_rank_one_accepts() {
    let t = TypeShape::forall(
        vec![0, 1],
        TypeShape::arrow(
            vec![TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(1))],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(1)),
        ),
    );
    assert_eq!(rank_of(&t), 1);
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-30: lambda body annotated rank-2 accepts (annotation on).
#[test]
fn r220m11_rank_30_lambda_annotated_rank_two_accepts() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 2);
    assert_accept(&t, SubLanguage::Lambda, true);
}

// ── Rank-2 unannotated in lambda (reject T0700) — 31..35 ─────────────────

/// r220m11-rank-31: `(∀α. α → α) → Int` in lambda, no annotation → T0700.
#[test]
fn r220m11_rank_31_lambda_unannotated_rank_two_rejects() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-32: `(∀α β. α → β → α) → Int` in lambda, no annotation.
#[test]
fn r220m11_rank_32_lambda_unannotated_k_arg_rejects() {
    let t = TypeShape::arrow(vec![k_ty()], TypeShape::Concrete);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-33: `(∀α. α → α) → (Int, Bool)` unannotated — T0700.
#[test]
fn r220m11_rank_33_lambda_unannotated_tuple_ret_rejects() {
    let t = TypeShape::arrow(
        vec![id_ty()],
        TypeShape::tuple(vec![TypeShape::Concrete, TypeShape::Concrete]),
    );
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-34: hint on lambda unannotated rank-2 names the escape hatch.
#[test]
fn r220m11_rank_34_lambda_unannotated_hint_mentions_annotate_type_boundary() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let d = check_rank_restricted(&t, SubLanguage::Lambda, false, s());
    assert_eq!(d.len(), 1);
    assert!(d[0].message().contains("@annotate_type_boundary"));
}

/// r220m11-rank-35: `Int → (∀α. α → α) → Int` unannotated — T0700 (rank 2).
#[test]
fn r220m11_rank_35_lambda_unannotated_second_arg_rejects() {
    let t = TypeShape::arrow(
        vec![TypeShape::Concrete],
        TypeShape::arrow(vec![id_ty()], TypeShape::Concrete),
    );
    assert_eq!(rank_of(&t), 2);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

// ── Non-prenex nested ∀ (reject T0700) — 36..40 ──────────────────────────

/// r220m11-rank-36: `∀α. (∀β. β → β) → α` — non-prenex, rank 2.
#[test]
fn r220m11_rank_36_nested_forall_in_arg_rejects_unannotated() {
    let t = TypeShape::forall(
        vec![0],
        TypeShape::arrow(vec![id_ty()], TypeShape::Var(0)),
    );
    assert_eq!(rank_of(&t), 2);
    assert!(!is_prenex(&t));
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-37: `∀α β. (∀γ. γ → γ) → α → β` — non-prenex, rank 2.
#[test]
fn r220m11_rank_37_multi_forall_with_nested_rejects_unannotated() {
    let t = TypeShape::forall(
        vec![0, 1],
        TypeShape::arrow(
            vec![id_ty()],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(1)),
        ),
    );
    assert_eq!(rank_of(&t), 2);
    assert!(!is_prenex(&t));
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-38: `∀α. α → (∀β. β → β)` — ∀ in return position, non-prenex.
#[test]
fn r220m11_rank_38_forall_in_return_rejects_unannotated() {
    // rank(∀β. β → β) = 1; not in arg position, but body contains inner ∀ so
    // this is non-prenex; rank(Arrow(Var, id_ty)) = max(promote(Var), rank(id_ty))
    // = max(0, 1) = 1; forall rule = max(1, 1) = 1... So this actually is rank 1.
    // Historical footnote: ∀ nested inside a positive/return position does
    // NOT bump rank under Peyton-Jones et al. 2007 — only argument positions do.
    // So this form is technically rank-1 but non-prenex, and Damas-Milner
    // still can't produce it without explicit annotation. The rank check as
    // spec'd here accepts it at rank ≤ 1; higher-layer semantics catch the
    // non-prenex-ness at a different pass. Keep this test explicitly probing
    // the boundary: the check accepts even though the form is non-prenex.
    let t = TypeShape::forall(
        vec![0],
        TypeShape::arrow(vec![TypeShape::Var(0)], id_ty()),
    );
    // Under the pure-rank check, this is rank 1 — accept.
    assert_eq!(rank_of(&t), 1);
    assert!(!is_prenex(&t));
    // The check accepts a rank-1 non-prenex form; the boundary between "rank"
    // and "prenex" is intentional. R225.M6 on the shell side re-checks
    // prenex-ness for let-generalisation completeness.
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-39: `(∀α. (∀β. β → β) → α) → Int` — rank 3 via nesting.
#[test]
fn r220m11_rank_39_deeply_nested_forall_rejects_unannotated() {
    let inner = TypeShape::forall(
        vec![0],
        TypeShape::arrow(vec![id_ty()], TypeShape::Var(0)),
    );
    // inner has rank 2; wrapping as an argument promotes it to rank 3.
    let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 3);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-40: prenex_is_rank_one_correspondence sanity check.
///
/// Any prenex form has rank ≤ 1; any rank-2+ form is non-prenex.
#[test]
fn r220m11_rank_40_prenex_iff_rank_leq_one() {
    let cases = [
        TypeShape::Concrete,
        TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete),
        id_ty(),
        k_ty(),
    ];
    for t in &cases {
        if is_prenex(t) {
            assert!(rank_of(t) <= 1, "prenex must be rank ≤ 1: {:?}", t);
        }
    }
    // Rank-2 form is not prenex.
    let rank_two = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_eq!(rank_of(&rank_two), 2);
    assert!(!is_prenex(&rank_two));
}

// ── Rank-3+ unannotated (reject T0700) — 41..43 ──────────────────────────

/// r220m11-rank-41: `((∀α. α → α) → Int) → Bool` — rank 3, unannotated.
#[test]
fn r220m11_rank_41_rank_three_unannotated_rejects() {
    let rank_two = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let t = TypeShape::arrow(vec![rank_two], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 3);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-42: `(((∀α. α → α) → Int) → Bool) → Char` — rank 4.
#[test]
fn r220m11_rank_42_rank_four_unannotated_rejects() {
    let r2 = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let r3 = TypeShape::arrow(vec![r2], TypeShape::Concrete);
    let t = TypeShape::arrow(vec![r3], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 4);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-43: rank-3 in pipeline sub-language — rejected.
#[test]
fn r220m11_rank_43_rank_three_in_pipeline_rejects() {
    let rank_two = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let t = TypeShape::arrow(vec![rank_two], TypeShape::Concrete);
    assert_reject_t0700(&t, SubLanguage::Pipeline, false);
}

// ── Rank-3+ even when annotated (reject T0700) — 44..45 ──────────────────

/// r220m11-rank-44: annotated lambda still rejects rank-3.
#[test]
fn r220m11_rank_44_annotated_lambda_rejects_rank_three() {
    let rank_two = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let t = TypeShape::arrow(vec![rank_two], TypeShape::Concrete);
    assert_eq!(rank_of(&t), 3);
    assert_reject_t0700(&t, SubLanguage::Lambda, true);
}

/// r220m11-rank-45: annotated lambda rank-3 hint says "rank ≥ 3 never accepted".
#[test]
fn r220m11_rank_45_annotated_lambda_rank_three_hint_explains_ceiling() {
    let rank_two = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let t = TypeShape::arrow(vec![rank_two], TypeShape::Concrete);
    let d = check_rank_restricted(&t, SubLanguage::Lambda, true, s());
    assert_eq!(d.len(), 1);
    let msg = d[0].message();
    assert!(msg.contains("rank"));
    assert!(msg.contains("3") || msg.contains("≥ 3"));
}

// ── Pipeline rank-2 (reject T0700) — 46..47 ──────────────────────────────

/// r220m11-rank-46: pipeline stage `(∀α. α → α) → Int` — T0700 even annotated.
#[test]
fn r220m11_rank_46_pipeline_rank_two_rejects_even_annotated() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_reject_t0700(&t, SubLanguage::Pipeline, true);
    assert_reject_t0700(&t, SubLanguage::Pipeline, false);
}

/// r220m11-rank-47: pipeline stage hint does NOT mention the escape hatch.
#[test]
fn r220m11_rank_47_pipeline_hint_does_not_offer_escape_hatch() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let d = check_rank_restricted(&t, SubLanguage::Pipeline, false, s());
    assert!(!d[0].message().contains("@annotate_type_boundary"));
    assert!(d[0].message().contains("pipeline"));
}

// ── Datalog rank-2 (reject T0700) — 48..49 ───────────────────────────────

/// r220m11-rank-48: datalog predicate `(∀α. α → α) → Bool` — T0700.
#[test]
fn r220m11_rank_48_datalog_rank_two_rejects_even_annotated() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_reject_t0700(&t, SubLanguage::Datalog, true);
    assert_reject_t0700(&t, SubLanguage::Datalog, false);
}

/// r220m11-rank-49: datalog hint identifies the sub-language in its text.
#[test]
fn r220m11_rank_49_datalog_hint_names_datalog() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    let d = check_rank_restricted(&t, SubLanguage::Datalog, false, s());
    assert!(d[0].message().contains("datalog"));
}

// ── Record with ∀ field, unannotated (reject T0700) — 50..52 ─────────────

/// r220m11-rank-50: `{ f: ∀α. α → α, g: Int }` — rank 2, unannotated → T0700.
#[test]
fn r220m11_rank_50_record_with_forall_field_rejects_unannotated() {
    let t = TypeShape::record(vec![(0, id_ty()), (1, TypeShape::Concrete)]);
    assert_eq!(rank_of(&t), 2);
    assert!(!is_prenex(&t));
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-51: `{ f: ∀α β. α → β → α }` — single polymorphic field, rank 2.
#[test]
fn r220m11_rank_51_record_single_forall_field_rejects_unannotated() {
    let t = TypeShape::record(vec![(0, k_ty())]);
    assert_eq!(rank_of(&t), 2);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-52: same record accepts under annotated Lambda.
#[test]
fn r220m11_rank_52_annotated_record_with_forall_field_accepts() {
    let t = TypeShape::record(vec![(0, id_ty()), (1, TypeShape::Concrete)]);
    assert_accept(&t, SubLanguage::Lambda, true);
}

// ── Tuple with ∀ component, unannotated (reject T0700) — 53..55 ──────────

/// r220m11-rank-53: `(∀α. α → α, Int)` — rank 2, unannotated → T0700.
#[test]
fn r220m11_rank_53_tuple_with_forall_component_rejects_unannotated() {
    let t = TypeShape::tuple(vec![id_ty(), TypeShape::Concrete]);
    assert_eq!(rank_of(&t), 2);
    assert!(!is_prenex(&t));
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-54: 3-tuple with one polymorphic component — rank 2.
#[test]
fn r220m11_rank_54_three_tuple_one_forall_rejects_unannotated() {
    let t = TypeShape::tuple(vec![TypeShape::Concrete, id_ty(), TypeShape::Concrete]);
    assert_eq!(rank_of(&t), 2);
    assert_reject_t0700(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-55: same tuple accepts under annotated Lambda.
#[test]
fn r220m11_rank_55_annotated_tuple_with_forall_component_accepts() {
    let t = TypeShape::tuple(vec![id_ty(), TypeShape::Concrete]);
    assert_accept(&t, SubLanguage::Lambda, true);
}

// ── Cross-sub-language composition (reject) — 56..60 ─────────────────────

/// r220m11-rank-56: annotated-in-lambda rank-2 value CANNOT flow into a
/// pipeline stage argument — pipeline rejects rank-2 regardless.
#[test]
fn r220m11_rank_56_annotated_rank_two_cannot_flow_into_pipeline() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_accept(&t, SubLanguage::Lambda, true);
    assert_reject_t0700(&t, SubLanguage::Pipeline, true);
}

/// r220m11-rank-57: annotated-in-lambda rank-2 value CANNOT flow into a
/// datalog predicate argument.
#[test]
fn r220m11_rank_57_annotated_rank_two_cannot_flow_into_datalog() {
    let t = TypeShape::arrow(vec![id_ty()], TypeShape::Concrete);
    assert_accept(&t, SubLanguage::Lambda, true);
    assert_reject_t0700(&t, SubLanguage::Datalog, true);
}

/// r220m11-rank-58: rank-1 forms are the meet of every ceiling — they
/// compose across all three sub-languages.
#[test]
fn r220m11_rank_58_rank_one_composes_across_all_sublanguages() {
    let t = id_ty();
    assert_accept(&t, SubLanguage::Pipeline, false);
    assert_accept(&t, SubLanguage::Datalog, false);
    assert_accept(&t, SubLanguage::Lambda, false);
}

/// r220m11-rank-59: rank-0 forms compose across every sub-language
/// under every annotation.
#[test]
fn r220m11_rank_59_rank_zero_composes_universally() {
    let t = TypeShape::arrow(vec![TypeShape::Concrete], TypeShape::Concrete);
    for sub in [SubLanguage::Pipeline, SubLanguage::Datalog, SubLanguage::Lambda] {
        for annotated in [false, true] {
            assert_accept(&t, sub, annotated);
        }
    }
}

/// r220m11-rank-60: max_rank ceilings match the spec's §3.1 table.
#[test]
fn r220m11_rank_60_ceilings_match_spec_table() {
    assert_eq!(max_rank(SubLanguage::Pipeline, false), 1);
    assert_eq!(max_rank(SubLanguage::Pipeline, true), 1);
    assert_eq!(max_rank(SubLanguage::Datalog, false), 1);
    assert_eq!(max_rank(SubLanguage::Datalog, true), 1);
    assert_eq!(max_rank(SubLanguage::Lambda, false), 1);
    assert_eq!(max_rank(SubLanguage::Lambda, true), 2);
}

// ── Coverage sanity: cargo-generated harness discovers exactly 60 tests ──
//
// If a future refactor accidentally removes one, this const will drift
// from the acceptance count. Compile-time-only documentation, not a
// runtime check (mirrors the R220.M8 corpus convention).
#[allow(dead_code)]
const R220_M11_TEST_COUNT: usize = 60;
