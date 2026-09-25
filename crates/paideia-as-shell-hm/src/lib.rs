//! paideia-as-shell-hm — R225.M1 (Hindley-Milner Algorithm W core) +
//! R225.M2 (Rémy-style row types for records) + R225.M3 (Rémy-style
//! row polymorphism for effect rows, disjoint from record rows).
//!
//! A textbook Damas-Milner type inference engine over a pure lambda
//! subset extended with row-polymorphic records. This is the
//! algorithmic substrate the later R225 milestones extend (effects,
//! rank-restricted let-polymorphism, pipeline and Datalog surface).
//!
//! # Lineage
//!
//! * Milner 1978, *A Theory of Type Polymorphism in Programming*
//!   (Algorithm W, the two-substitution presentation used here).
//! * Damas & Milner 1982, *Principal Type-Schemes for Functional
//!   Programs* (soundness / completeness / principality proofs).
//! * Robinson 1965, *A Machine-Oriented Logic Based on the Resolution
//!   Principle* (the unification algorithm at the heart of W).
//!
//! # Scope at M2
//!
//! The expression language is the pure lambda calculus plus `let`,
//! record literals, and field access:
//!
//! ```text
//!     e ::= x                       -- variable
//!         | c                       -- literal (Int | Str)
//!         | \x. e                   -- abstraction
//!         | e1 e2                   -- application
//!         | let x = e1 in e2        -- let-generalisation
//!         | { l1 = e1, ..., ln = en }  -- record literal
//!         | e.l                     -- field access
//! ```
//!
//! The type language is monotypes and rank-1 schemes, with rows for
//! records:
//!
//! ```text
//!     τ ::= a                       -- type variable
//!         | C                       -- type constant (Int | Str)
//!         | τ1 -> τ2                -- function type
//!         | { ρ }                   -- record over row ρ
//!     ρ ::= ∅                       -- empty (closed) row
//!         | r                       -- row variable (tail)
//!         | l: τ ; ρ                -- row extension
//!     σ ::= ∀α. τ                   -- (rank-1) type scheme
//! ```
//!
//! # Restrictions
//!
//! * No recursion (no `letrec` / no fixpoint combinator).
//! * No sum types.
//! * No effects.
//! * No rank-2+ polymorphism — this is Damas-Milner, not System F.
//! * The rank-restriction spec at `design/toolchain/rank-restricted-hm.md`
//!   is a downstream check (R225.M6); no policy is enforced here beyond
//!   the intrinsic rank-1 shape of `TypeScheme`.
//!
//! # Pipeline position
//!
//! ```text
//!     Expr ── infer ─▶  (Substitution, MonoType)
//!                            │
//!                            ▼
//!                   generalize (at Let)
//!                            │
//!                            ▼
//!                       TypeScheme
//! ```
//!
//! # Non-goals
//!
//! This crate never mutates the existing `paideia-as-elaborator` type
//! system. It stands alone so R225.M2+ can iterate on the shell's HM
//! surface without perturbing the compiler frontend that ships the
//! phase-1 self-host.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod diagnostic;
pub mod effect_row;
pub mod expr;
pub mod infer;
pub mod subst;
pub mod ty;
pub mod typed_value;
pub mod unify;

pub use diagnostic::{
    infer_with_diagnostic, render_diagnostic, TypeDiagnostic, TypeSpan,
};
pub use effect_row::{unify_effect_rows, EffectRow};
pub use expr::{app, field, i, lam, let_, record, s, v, Expr, Lit};
pub use infer::{
    generalize, infer, instantiate, FreshVarGen, InferError, TypeEnv,
};
pub use subst::Substitution;
pub use ty::{MonoType, RowType, TypeScheme, TypeVar};
pub use typed_value::{
    compose_pipeline_effects, typed_value_pipe, unify_typed_values,
    unify_typed_values_with_fresh, union_effect_rows, TypedValue,
};
pub use unify::{unify, unify_with_fresh, UnifyError};
