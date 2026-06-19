//! paideia-as-effects
//!
//! Effect-row representation and row-polymorphism unifier per
//! `design/toolchain/custom-assembler.md` §4.2. Uses
//! `paideia_as_ir::EffectRowId` as the interner's id type so types,
//! effects, and IR all share the same row identifier space.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod intern;
mod registry;
mod row;
mod unify;

pub use intern::EffectInterner;
pub use registry::{EffectRegistry, F_REDECL_MISMATCH, Operation, SignatureId};
pub use row::{EffectId, EffectRow, RowVarId};
pub use unify::{RowDiff, Substitution, UnifyError, unify};
