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
mod types;

pub use cap_set::{CapId, CapSet, CapSetInterner};
pub use intern::TypeInterner;
pub use kinds::{Kind, type_kind};
pub use types::{CapSetId, SIZE_WIDTH_SENTINEL, Type, TypeId};
