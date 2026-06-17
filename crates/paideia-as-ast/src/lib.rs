//! paideia-as-ast
//!
//! Arena-backed surface AST for paideia-as source files. Every node is
//! interned in an [`AstArena`] and referred to by [`NodeId`]. See
//! `design/toolchain/syntax-reference.md` and the parser crate for the
//! consumer side.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod arena;
mod items;
mod node_id;
pub mod pretty;
mod visit;

pub use arena::{AstArena, NodeData, NodeKind};
pub use items::ItemData;
pub use node_id::NodeId;
pub use visit::{ItemVisitor, walk_item};
