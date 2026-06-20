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
mod items;
mod macros;
mod modules;
mod node_id;
mod patterns;
pub mod pretty;
pub mod reflect;
mod stmts;
mod types;
mod visit;

pub use arena::{AstArena, NodeData, NodeKind};
pub use exprs::{ExprData, HandlerArm, LoopKind, MatchArm, SharingConstraint};
pub use items::ItemData;
pub use macros::{MacroDeclData, MacroFragment, MacroFragmentKind, MacroRule};
pub use modules::{
    Def, Functor, IncludeDecl, ModuleDecl, SigDecl, Signature, Structure, TypeAbstraction,
    TypeDecl, ValDecl,
};
pub use node_id::NodeId;
pub use patterns::{PatField, PatternData};
pub use reflect::{SerializedSpan, SerializedTerm, Term, TermHead};
pub use stmts::StmtData;
pub use types::{EnumVariant, LinClass, TypeData};
pub use visit::{
    ExprVisitor, ItemVisitor, PatternVisitor, StmtVisitor, TypeVisitor, walk_expr, walk_item,
    walk_pattern, walk_stmt, walk_type,
};
