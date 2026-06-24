//! paideia-as-ir
//!
//! Typed-core IR for paideia-as. Parallels the AST arena pattern with a
//! single `IrArena` slab over `IrNodeData`. Every node carries a
//! substructural lattice class and an interned effect-row reference per
//! `design/toolchain/custom-assembler.md` §6.1.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod addr_of;
pub mod anf;
mod arena;
pub mod binding_name;
pub mod borrow;
pub mod call_meta;
pub mod cast_meta;
pub mod constant_pool;
pub mod data;
pub mod effect_rewrite;
pub mod enum_layout;
pub mod handler_value;
pub mod instruction;
pub mod lambda_param;
pub mod let_meta;
pub mod literal_bytes;
pub mod literal_value;
pub mod load_store;
pub mod loop_meta;
pub mod modules;
pub mod monomorphisation;
mod node;
pub mod opt;
pub mod pretty;
pub mod record_layout;
pub mod section_attr;
pub mod string_literal;
pub mod symbol;
pub mod walker;
pub mod walker_ctx;

pub use addr_of::{AddrOfMeta, AddrOfSideTable};
pub use anf::{AnfRewrite, is_atomic, normalise_operands};
pub use arena::IrArena;
pub use binding_name::BindingNameTable;
pub use borrow::{BorrowMeta, BorrowSideTable};
pub use call_meta::{CallMeta, CallSideTable};
pub use cast_meta::CastSideTable;
pub use constant_pool::ConstantPoolTable;
pub use data::{DataEntry, DataSideTable, RelocSpec, RelocWidth, SectionKind};
pub use effect_rewrite::{
    HandlerTable, PerformRewrite, WithRewrite, rewrite_perform, rewrite_unsafe_passthrough,
    rewrite_with_save_restore,
};
pub use enum_layout::{EnumConsInfo, EnumConsSideTable, EnumDiscriminantSideTable, EnumTypeId};
pub use handler_value::{HandlerInfo, HandlerSideTable, pretty_handler};
pub use instruction::{
    Cond, EncodingHint, InstrMode, Instruction, InstructionSideTable, IntWidth, Mnemonic, Operand,
    RegId, Scale, SegReg,
};
pub use lambda_param::LambdaParamTable;
pub use let_meta::{LetInfo, LetMetaTable};
pub use literal_bytes::LiteralBytesTable;
pub use literal_value::LiteralValueTable;
pub use load_store::{
    LoadStoreInfo, LoadStoreSideTable, Signedness, Width, alloc_load, alloc_store,
};
pub use loop_meta::{LoopMeta, LoopMetaTable};
pub use modules::{
    FieldKind, FunctorInfo, ModuleField, ModuleInfo, ModuleSideTable, pretty_module,
};
pub use monomorphisation::{MonoKey, MonomorphisationTable, TypeId};
pub use node::{EffectRowId, IrKind, IrNodeData, IrNodeId, LinClass};
pub use record_layout::{FieldAccessInfo, FieldAccessSideTable, RecordLayoutTable, RecordTypeId};
pub use section_attr::{SectionAttr, SectionAttrTable};
pub use string_literal::{StringLiteralInfo, StringLiteralTable};
pub use symbol::{Symbol, SymbolKind, SymbolTable, Visibility};
pub use walker::{IrWalker, walk};
pub use walker_ctx::WalkerCtx;

// Re-export smallvec for clients building child lists.
pub use smallvec::SmallVec;
