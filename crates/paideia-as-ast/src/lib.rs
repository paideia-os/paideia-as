//! paideia-as-ast
//!
//! Arena-backed surface AST for paideia-as source files. Every node is
//! interned in an [`AstArena`] and referred to by [`NodeId`]. See
//! `design/toolchain/syntax-reference.md` and the parser crate for the
//! consumer side.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod arena;
mod exprs;
mod item_atomic;
mod items;
mod macros;
mod modules;
mod node_id;
mod patterns;
pub mod pattern_type_hints;
pub mod pretty;
pub mod reflect;
mod stmts;
mod types;
mod visit;

pub use arena::{AstArena, NodeData, NodeKind};
pub use item_atomic::ItemAtomicTable;
pub use exprs::{
    ExprData, GenericParam, HandlerArm, LoopKind, MatchArm, MatchAttrs, PrefixOp, SegPrefix,
    SharingConstraint,
};
pub use items::{
    AtomicOrdering, AttrValue, CallingConvention, ImplDecl, InterruptAttr, ItemAttribute,
    ItemData, TraitMethod,
};
pub use macros::{MacroDeclData, MacroFragment, MacroFragmentKind, MacroRule};
pub use modules::{
    Def, Functor, IncludeDecl, ModuleDecl, SigDecl, Signature, Structure, TypeAbstraction,
    TypeDecl, ValDecl,
};
pub use node_id::NodeId;
pub use pattern_type_hints::PatternTypeHints;
pub use patterns::{PatField, PatternData};
pub use reflect::{SerializedSpan, SerializedTerm, Term, TermHead};
pub use stmts::StmtData;
pub use types::{EnumVariant, LinClass, TypeData};
pub use visit::{
    ExprVisitor, ItemVisitor, PatternVisitor, StmtVisitor, TypeVisitor, walk_expr, walk_item,
    walk_pattern, walk_stmt, walk_type,
};
