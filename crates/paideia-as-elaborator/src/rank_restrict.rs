//! Rank-restricted let-polymorphism check (R220.M11, closes `paideia-as#1425`).
//!
//! Formalises and enforces the sound HM subset paideia-as adopts across
//! the three sub-languages the semantic shell composes: pipeline, Datalog,
//! and lambda. See `design/toolchain/rank-restricted-hm.md` for the full
//! spec; this module is its executable half.
//!
//! ## Why a check at all
//!
//! Unrestricted rank-N HM inference is undecidable (Wells 1999).
//! Damas-Milner 1982 is decidable and complete at rank 1 (all `∀` at the
//! outermost prenex position). Peyton Jones et al. 2007 showed that
//! *predicative rank-N with explicit annotations* stays decidable — the
//! shape paideia-as adopts here.
//!
//! ## The rank formula (from `design/toolchain/rank-restricted-hm.md` §1)
//!
//! ```text
//! rank(τ)         = 0                                       -- monotype
//! rank(σ₁ → σ₂)   = max(promote(σ₁), rank(σ₂))
//!                   promote(σ) = rank(σ) + 1  if σ has any ∀
//!                                rank(σ)      otherwise
//! rank(∀α. σ)     = max(1, rank(σ))
//!
//! rank((σ₁,…,σₙ))          = max_i promote(σᵢ)   -- tuples like arguments
//! rank({f₁:σ₁,…,fₙ:σₙ})    = max_i promote(σᵢ)   -- records like arguments
//! ```
//!
//! Rank-1 forms are exactly the prenex forms. Rank ≥ 2 forms are
//! non-prenex by definition.
//!
//! ## Composition with R220.M8
//!
//! This module sits alongside `crate::effect_infer` (R220.M8, landed
//! v0.36.6). It does not touch [`paideia_as_effects::Substitution`] —
//! effect rows are orthogonal to type rank (a row-polymorphic function
//! `∀e. Unit →!{Mem | e} Unit` is rank 1 as long as `e` is a row
//! variable, not a polytype). The check treats effect rows as
//! monomorphic w.r.t. type rank; row-substitution machinery
//! (`Substitution::apply` / `Substitution::compose` from M8) continues
//! to run over row variables independently of the T0700 pass here.
//!
//! ## What this module does NOT do (deferred)
//!
//! - **Walker-side wiring.** M11 lands the pure function and the
//!   diagnostic; call sites in the pipeline / Datalog / lambda walkers
//!   land alongside their sub-language substrates (R221.M4, R226 group,
//!   R225.M6 on `paideia-os`).
//! - **`Scheme` wrapper in `paideia-as-types`.** Phase-1 `Type` is
//!   monomorphic; polymorphism is implicit through unification
//!   variables. When `Scheme` lands, [`TypeShape`] here becomes a thin
//!   adapter — the *semantics* below are stable across that refactor.
//! - **Impredicative rank-1** and **higher-rank row polymorphism** are
//!   not part of the R220 subset; see spec §7.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

/// Diagnostic code for a rank-restricted let-polymorphism violation.
///
/// **T0700** is in the Type-system range (Category::T, 500..=899).
/// Semantic-shell R225.M6 lifts this to shell-side `E0980` per plan §6 R2.
pub const T_RANK_VIOLATION: u16 = 700;

/// Which sub-language a checked position belongs to.
///
/// The ceiling differs by sub-language — see [`max_rank`]. Pipeline and
/// Datalog have no escape hatch; only lambda accepts rank-2 with an
/// explicit `@annotate_type_boundary` marker.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SubLanguage {
    /// A pipeline stage type (`pipeline { … }`).
    Pipeline,
    /// A Datalog predicate declaration (`datalog { … }`).
    Datalog,
    /// A lambda body / let-generalisation type (general functional code).
    Lambda,
}

impl SubLanguage {
    /// Rendered name for diagnostic messages.
    fn label(self) -> &'static str {
        match self {
            SubLanguage::Pipeline => "pipeline",
            SubLanguage::Datalog => "datalog",
            SubLanguage::Lambda => "lambda",
        }
    }
}

/// Rank-analysis surface type.
///
/// A view over the elaborator's lowered type built on demand for the
/// rank check. Phase-1 `paideia-as-types::Type` is monomorphic and has
/// no explicit `Scheme` / `ForAll` variant; this enum is the shim that
/// lets the check operate without waiting on that refactor.
///
/// Every variant models exactly the constructs the rank formula
/// distinguishes: monotypes (`Concrete`, `Var`), arrows, universal
/// quantifiers, and products (tuples and records) — the last two are
/// treated as "curried argument positions" for rank promotion, per
/// spec §1.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeShape {
    /// A concrete monotype leaf: primitive (`Int`, `Bool`, `Unit`, …),
    /// nominal (`Named T args` when all args are `Concrete`), or a
    /// reference / pointer whose pointee is concrete. Rank 0.
    Concrete,
    /// A type variable bound by an enclosing `Forall`. Rank 0 —
    /// substitution never raises rank on its own.
    Var(u32),
    /// Function arrow. Rank is `max(promote(param_i), rank(ret))`.
    Arrow {
        /// Parameter types.
        params: Vec<TypeShape>,
        /// Return type.
        ret: Box<TypeShape>,
    },
    /// Universal quantification over the listed type variables. Rank
    /// `max(1, rank(body))`.
    Forall {
        /// Bound type-variable ids (opaque; the check does not perform
        /// scope tracking — that belongs upstream).
        vars: Vec<u32>,
        /// The body under the quantifier.
        body: Box<TypeShape>,
    },
    /// Product tuple `(σ₁, …, σₙ)`. Rank is `max_i promote(σᵢ)`.
    Tuple(Vec<TypeShape>),
    /// Record `{f₁: σ₁, …, fₙ: σₙ}`. Rank is `max_i promote(σᵢ)`.
    ///
    /// The field-name key is interned (`u32`); the check only reads the
    /// value type, so the key discipline (uniqueness, ordering) is left
    /// to the caller — this module treats a record purely as a bag of
    /// component types.
    Record(Vec<(u32, TypeShape)>),
}

impl TypeShape {
    /// Build a concrete leaf.
    #[must_use]
    pub fn concrete() -> Self {
        TypeShape::Concrete
    }

    /// Build an arrow.
    #[must_use]
    pub fn arrow(params: Vec<TypeShape>, ret: TypeShape) -> Self {
        TypeShape::Arrow {
            params,
            ret: Box::new(ret),
        }
    }

    /// Build a universal quantifier.
    #[must_use]
    pub fn forall(vars: Vec<u32>, body: TypeShape) -> Self {
        TypeShape::Forall {
            vars,
            body: Box::new(body),
        }
    }

    /// Build a tuple.
    #[must_use]
    pub fn tuple(elems: Vec<TypeShape>) -> Self {
        TypeShape::Tuple(elems)
    }

    /// Build a record.
    #[must_use]
    pub fn record(fields: Vec<(u32, TypeShape)>) -> Self {
        TypeShape::Record(fields)
    }
}

/// The maximum rank accepted at a position under the given sub-language
/// and annotation state.
///
/// | Sub-language | annotated | maxRank |
/// |--------------|-----------|---------|
/// | Pipeline     | any       | 1       |
/// | Datalog      | any       | 1       |
/// | Lambda       | false     | 1       |
/// | Lambda       | true      | 2       |
///
/// Rank ≥ 3 is rejected in every sub-language and every annotation mode
/// — the annotation lifts to rank 2, not rank ∞. See spec §3.1.
#[must_use]
pub fn max_rank(sublang: SubLanguage, annotated: bool) -> u32 {
    match (sublang, annotated) {
        (SubLanguage::Pipeline, _) => 1,
        (SubLanguage::Datalog, _) => 1,
        (SubLanguage::Lambda, false) => 1,
        (SubLanguage::Lambda, true) => 2,
    }
}

/// Does the type mention a `∀` anywhere in its subtree?
///
/// Used to compute the argument-position "promote" bump in
/// [`rank_of`] — a `∀`-bearing argument adds 1 to the rank of the
/// enclosing arrow / tuple / record component.
#[must_use]
pub fn contains_forall(ty: &TypeShape) -> bool {
    match ty {
        TypeShape::Concrete | TypeShape::Var(_) => false,
        TypeShape::Forall { .. } => true,
        TypeShape::Arrow { params, ret } => {
            params.iter().any(contains_forall) || contains_forall(ret)
        }
        TypeShape::Tuple(elems) => elems.iter().any(contains_forall),
        TypeShape::Record(fields) => fields.iter().any(|(_, t)| contains_forall(t)),
    }
}

/// Compute the syntactic rank of a type.
///
/// Follows the recursive definition from spec §1. Termination is by
/// structural recursion on `TypeShape`; there is no unification and no
/// substitution — the rank is a purely syntactic property.
#[must_use]
pub fn rank_of(ty: &TypeShape) -> u32 {
    /// The "promote" helper: if `σ` contains a `∀`, its rank in an
    /// argument-like position is `rank(σ) + 1`; otherwise `rank(σ)`.
    fn promote(sigma: &TypeShape) -> u32 {
        let r = rank_of(sigma);
        if contains_forall(sigma) {
            r.saturating_add(1).max(1)
        } else {
            r
        }
    }

    match ty {
        TypeShape::Concrete | TypeShape::Var(_) => 0,
        TypeShape::Arrow { params, ret } => {
            let arg = params.iter().map(promote).max().unwrap_or(0);
            let r = rank_of(ret);
            arg.max(r)
        }
        TypeShape::Forall { body, .. } => rank_of(body).max(1),
        TypeShape::Tuple(elems) => elems.iter().map(promote).max().unwrap_or(0),
        TypeShape::Record(fields) => fields.iter().map(|(_, t)| promote(t)).max().unwrap_or(0),
    }
}

/// Is the type in prenex form?
///
/// Equivalently: `rank_of(ty) ≤ 1`. A prenex type has all `∀` at the
/// outermost prefix; anything below an arrow, tuple, or record component
/// is `∀`-free.
///
/// A monotype (no `∀` anywhere) is trivially prenex. A form
/// `∀α₁ … ∀αₙ. ρ` with ρ containing no `∀` is prenex. Any nested `∀`
/// breaks prenex-ness and lifts the rank to 2 or higher.
#[must_use]
pub fn is_prenex(ty: &TypeShape) -> bool {
    match ty {
        TypeShape::Forall { body, .. } => is_prenex(body),
        other => !contains_forall(other),
    }
}

/// Check a type at a term position under a given sub-language.
///
/// Emits **at most one** [`T0700`](T_RANK_VIOLATION) diagnostic at
/// `span` when the type's rank exceeds the sub-language's ceiling.
///
/// The diagnostic message identifies the sub-language, the observed
/// rank, the accepted maximum, and — when a workaround exists — the
/// `@annotate_type_boundary` escape hatch. See spec §5.4 for the exact
/// text shape.
///
/// # Contract
/// - `ty` is inspected structurally; no unification or substitution
///   runs here. Row-substitution machinery
///   ([`paideia_as_effects::Substitution::apply`],
///   [`paideia_as_effects::Substitution::compose`] from R220.M8) is
///   orthogonal to this check.
/// - The check returns `Vec<Diagnostic>` (empty on accept) so callers
///   can compose it with other passes without special-casing the
///   success shape — matches the [`crate::effect_infer::RowOutcome`]
///   discipline from R220.M8.
///
/// # Diagnostics
/// - **T0700** — rank exceeds ceiling for the (sub-language, annotated)
///   pair. The message names the observed rank, the ceiling, and (for
///   `Lambda` with `rank ≤ 2`) suggests `@annotate_type_boundary`.
#[must_use]
pub fn check_rank_restricted(
    ty: &TypeShape,
    sublang: SubLanguage,
    annotated: bool,
    span: Span,
) -> Vec<Diagnostic> {
    let observed = rank_of(ty);
    let ceiling = max_rank(sublang, annotated);
    if observed <= ceiling {
        return Vec::new();
    }
    vec![rank_violation_diag(observed, ceiling, sublang, annotated, span)]
}

/// Construct the T0700 diagnostic for a rank violation.
fn rank_violation_diag(
    observed: u32,
    ceiling: u32,
    sublang: SubLanguage,
    annotated: bool,
    span: Span,
) -> Diagnostic {
    let hint = rank_violation_hint(observed, sublang, annotated);
    let msg = format!(
        "type-rank violation: this {} position accepts up to rank {}, got rank {}\n{}",
        sublang.label(),
        ceiling,
        observed,
        hint,
    );
    Diagnostic::error(t_code(T_RANK_VIOLATION))
        .message(msg)
        .with_span(span)
        .finish()
}

/// Choose the hint text based on sub-language and annotation state.
///
/// - Pipeline / Datalog: no escape hatch — the fix is to lower the type
///   to rank ≤ 1.
/// - Lambda unannotated, `observed ≤ 2`: propose
///   `@annotate_type_boundary`.
/// - Lambda annotated or `observed ≥ 3`: explain that rank ≥ 3 is not
///   accepted; decompose.
fn rank_violation_hint(observed: u32, sublang: SubLanguage, annotated: bool) -> &'static str {
    match sublang {
        SubLanguage::Pipeline => {
            "hint: pipeline stages must be rank ≤ 1; decompose the type or move the polymorphism inside a lambda"
        }
        SubLanguage::Datalog => {
            "hint: datalog predicates must be rank ≤ 1; decompose the type or move the polymorphism inside a lambda"
        }
        SubLanguage::Lambda => {
            if annotated {
                "hint: rank ≥ 3 is never accepted (the annotation lifts to rank 2); decompose the type"
            } else if observed <= 2 {
                "hint: add @annotate_type_boundary to lift the ceiling to rank 2"
            } else {
                "hint: rank ≥ 3 is never accepted; decompose the type"
            }
        }
    }
}

/// Helper: build a Category::T error code.
fn t_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn s() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// Trivial: a monotype is rank 0 and prenex.
    #[test]
    fn monotype_is_rank_zero_prenex() {
        let t = TypeShape::Concrete;
        assert_eq!(rank_of(&t), 0);
        assert!(is_prenex(&t));
    }

    /// A first-order arrow chain (no ∀) stays at rank 0.
    #[test]
    fn first_order_arrow_is_rank_zero() {
        // (Int, Int) -> Int
        let t = TypeShape::arrow(vec![TypeShape::Concrete, TypeShape::Concrete], TypeShape::Concrete);
        assert_eq!(rank_of(&t), 0);
        assert!(is_prenex(&t));
    }

    /// A prenex ∀α. α → α is rank 1.
    #[test]
    fn prenex_forall_is_rank_one() {
        // ∀α. α → α
        let t = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        assert_eq!(rank_of(&t), 1);
        assert!(is_prenex(&t));
    }

    /// (∀α. α→α) → Int is rank 2 — ∀ inside argument.
    #[test]
    fn forall_in_argument_is_rank_two() {
        let inner = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        // (∀α. α→α) → Int
        let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
        assert_eq!(rank_of(&t), 2);
        assert!(!is_prenex(&t));
    }

    /// A rank-2 form is rejected under Lambda without annotation.
    #[test]
    fn lambda_unannotated_rejects_rank_two() {
        let inner = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
        let d = check_rank_restricted(&t, SubLanguage::Lambda, false, s());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code().number(), T_RANK_VIOLATION);
    }

    /// The same rank-2 form is accepted under Lambda WITH annotation.
    #[test]
    fn lambda_annotated_accepts_rank_two() {
        let inner = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
        let d = check_rank_restricted(&t, SubLanguage::Lambda, true, s());
        assert!(d.is_empty());
    }

    /// Rank 3 is rejected even with annotation.
    #[test]
    fn lambda_annotated_still_rejects_rank_three() {
        // ((∀α. α → α) → Int) → Bool has rank 3.
        let id = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let rank_two = TypeShape::arrow(vec![id], TypeShape::Concrete);
        let t = TypeShape::arrow(vec![rank_two], TypeShape::Concrete);
        assert_eq!(rank_of(&t), 3);
        let d = check_rank_restricted(&t, SubLanguage::Lambda, true, s());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code().number(), T_RANK_VIOLATION);
    }

    /// Pipeline never accepts rank ≥ 2 even with annotation.
    #[test]
    fn pipeline_rejects_rank_two_even_annotated() {
        let inner = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
        let d = check_rank_restricted(&t, SubLanguage::Pipeline, true, s());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code().number(), T_RANK_VIOLATION);
    }

    /// A tuple with a ∀ component promotes to rank 2.
    #[test]
    fn tuple_with_forall_component_is_rank_two() {
        let id = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let t = TypeShape::tuple(vec![id, TypeShape::Concrete]);
        assert_eq!(rank_of(&t), 2);
        assert!(!is_prenex(&t));
    }

    /// Diagnostic hint mentions the annotation for lambda unannotated rank-2.
    #[test]
    fn lambda_unannotated_hint_mentions_annotate_type_boundary() {
        let inner = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
        let d = check_rank_restricted(&t, SubLanguage::Lambda, false, s());
        assert!(
            d[0].message().contains("@annotate_type_boundary"),
            "hint missing escape-hatch reference"
        );
    }

    /// Diagnostic for pipeline does NOT propose the escape hatch.
    #[test]
    fn pipeline_hint_does_not_mention_escape_hatch() {
        let inner = TypeShape::forall(
            vec![0],
            TypeShape::arrow(vec![TypeShape::Var(0)], TypeShape::Var(0)),
        );
        let t = TypeShape::arrow(vec![inner], TypeShape::Concrete);
        let d = check_rank_restricted(&t, SubLanguage::Pipeline, false, s());
        assert!(
            !d[0].message().contains("@annotate_type_boundary"),
            "pipeline hint must not propose the lambda-only escape hatch"
        );
    }
}
