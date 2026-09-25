//! paideia-as-shell-hm — R225.M1 (Hindley-Milner Algorithm W core).
//!
//! A textbook Damas-Milner type inference engine over a pure lambda
//! subset. This is the algorithmic substrate the later R225 milestones
//! extend (rows, effects, rank-restricted let-polymorphism, pipeline
//! and Datalog surface).
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
//! # Scope at M1
//!
//! The expression language is the pure lambda calculus plus `let`:
//!
//! ```text
//!     e ::= x                       -- variable
//!         | c                       -- literal (Int | Str)
//!         | \x. e                   -- abstraction
//!         | e1 e2                   -- application
//!         | let x = e1 in e2        -- let-generalisation
//! ```
//!
//! The type language is monotypes and rank-1 schemes:
//!
//! ```text
//!     τ ::= a                       -- type variable
//!         | C                       -- type constant (Int | Str)
//!         | τ1 -> τ2                -- function type
//!     σ ::= ∀α. τ                   -- (rank-1) type scheme
//! ```
//!
//! # Restrictions (M1)
//!
//! * No recursion (no `letrec` / no fixpoint combinator).
//! * No product / sum / record / row types.
//! * No effects.
//! * No rank-2+ polymorphism — this is Damas-Milner, not System F.
//! * The rank-restriction spec at `design/toolchain/rank-restricted-hm.md`
//!   is a downstream check (R225.M6); at M1 no policy is enforced beyond
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

pub mod expr;
pub mod infer;
pub mod subst;
pub mod ty;
pub mod unify;

pub use expr::{app, i, lam, let_, s, v, Expr, Lit};
pub use infer::{
    generalize, infer, instantiate, FreshVarGen, InferError, TypeEnv,
};
pub use subst::Substitution;
pub use ty::{MonoType, TypeScheme, TypeVar};
pub use unify::{unify, UnifyError};
