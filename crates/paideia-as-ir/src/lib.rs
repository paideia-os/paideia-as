//! paideia-as-ir
//!
//! Typed-core IR for paideia-as. Parallels the AST arena pattern with a
//! single `IrArena` slab over `IrNodeData`. Every node carries a
//! substructural lattice class and an interned effect-row reference per
//! `design/toolchain/custom-assembler.md` §6.1.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod anf;
mod arena;
pub mod effect_rewrite;
pub mod handler_value;
pub mod modules;
mod node;
pub mod pretty;
pub mod walker;
pub mod walker_ctx;

pub use anf::{AnfRewrite, is_atomic, normalise_operands};
pub use arena::IrArena;
pub use effect_rewrite::{
    HandlerTable, PerformRewrite, WithRewrite, rewrite_perform, rewrite_unsafe_passthrough,
    rewrite_with_save_restore,
};
pub use handler_value::{HandlerInfo, HandlerSideTable, pretty_handler};
pub use modules::{
    FieldKind, FunctorInfo, ModuleField, ModuleInfo, ModuleSideTable, pretty_module,
};
pub use node::{EffectRowId, IrKind, IrNodeData, IrNodeId, LinClass};
pub use walker::{IrWalker, walk};
pub use walker_ctx::WalkerCtx;

// Re-export smallvec for clients building child lists.
pub use smallvec::SmallVec;
