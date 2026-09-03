//! paideia-as-types
//!
//! Monomorphic type interner + lattice-class kinds. The elaborator
//! (PR-30+) uses this to assign types to IR nodes. See
//! `design/toolchain/custom-assembler.md` §5.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cap_set;
mod fixed_point;
mod handler_compose;
mod intern;
mod kind_inference;
mod kinds;
mod layout;
mod regions;
mod row_poly;
mod row_subtype;
mod session;
mod subst;
mod types;
mod unify;
mod vec_typaram;

pub use cap_set::{CapId, CapSet, CapSetInterner};
pub use fixed_point::{FixedPoint, fp_add, fp_mul};
pub use handler_compose::{ComposeErr, Handler, compose_handlers};
pub use intern::TypeInterner;
pub use kind_inference::{infer_kind_for_generic_param, kind_of_type_constructor};
pub use kinds::{
    HrKind, Kind, ModuleKind, SigDeclKind, SignatureKind, kind_functor, kind_signature, type_kind,
};
pub use layout::{Layout, bit_width, layout_of};
pub use regions::{RegionGraph, RegionId, RegionInterner};
pub use row_poly::{EffectRow, RowVar, unify_rows};
pub use row_subtype::{RowRecord, sub_record};
pub use session::{SessionTy, SessionWfError, dual, wf_session, wf_session_with_env};
pub use subst::Subst;
pub use types::{CapSetId, EnumPayload, SIZE_WIDTH_SENTINEL, TyVar, Type, TypeId};
pub use unify::{UnifyError, unify};
pub use vec_typaram::{VecTy, kind_of_vec, vec_layout};
