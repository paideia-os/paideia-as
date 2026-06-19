//! paideia-as-types
//!
//! Monomorphic type interner + lattice-class kinds. The elaborator
//! (PR-30+) uses this to assign types to IR nodes. See
//! `design/toolchain/custom-assembler.md` §5.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cap_set;
mod intern;
mod kinds;
mod subst;
mod types;
mod unify;

pub use cap_set::{CapId, CapSet, CapSetInterner};
pub use intern::TypeInterner;
pub use kinds::{
    Kind, ModuleKind, SigDeclKind, SignatureKind, kind_functor, kind_signature, type_kind,
};
pub use subst::Subst;
pub use types::{CapSetId, SIZE_WIDTH_SENTINEL, TyVar, Type, TypeId};
pub use unify::{UnifyError, unify};
