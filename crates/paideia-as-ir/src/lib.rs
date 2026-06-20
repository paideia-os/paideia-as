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
pub mod call_meta;
pub mod effect_rewrite;
pub mod enum_layout;
pub mod handler_value;
pub mod instruction;
pub mod load_store;
pub mod loop_meta;
pub mod modules;
pub mod monomorphisation;
mod node;
pub mod opt;
pub mod pretty;
pub mod record_layout;
pub mod string_literal;
pub mod walker;
pub mod walker_ctx;

pub use anf::{AnfRewrite, is_atomic, normalise_operands};
pub use arena::IrArena;
pub use call_meta::{CallMeta, CallSideTable};
pub use effect_rewrite::{
    HandlerTable, PerformRewrite, WithRewrite, rewrite_perform, rewrite_unsafe_passthrough,
    rewrite_with_save_restore,
};
pub use enum_layout::{EnumConsInfo, EnumConsSideTable, EnumDiscriminantSideTable, EnumTypeId};
pub use handler_value::{HandlerInfo, HandlerSideTable, pretty_handler};
pub use instruction::{
    Cond, EncodingHint, Instruction, InstructionSideTable, Mnemonic, Operand, RegId, Scale,
};
pub use load_store::{
    LoadStoreInfo, LoadStoreSideTable, Signedness, Width, alloc_load, alloc_store,
};
pub use loop_meta::{LoopMeta, LoopMetaTable};
pub use modules::{
    FieldKind, FunctorInfo, ModuleField, ModuleInfo, ModuleSideTable, pretty_module,
};
pub use monomorphisation::{MonoKey, MonomorphisationTable};
pub use node::{EffectRowId, IrKind, IrNodeData, IrNodeId, LinClass};
pub use record_layout::{FieldAccessInfo, FieldAccessSideTable, RecordLayoutTable, RecordTypeId};
pub use string_literal::{StringLiteralInfo, StringLiteralTable};
pub use walker::{IrWalker, walk};
pub use walker_ctx::WalkerCtx;

// Re-export smallvec for clients building child lists.
pub use smallvec::SmallVec;
