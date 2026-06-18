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
mod node;
pub mod pretty;

pub use anf::{AnfRewrite, is_atomic, normalise_operands};
pub use arena::IrArena;
pub use node::{EffectRowId, IrKind, IrNodeData, IrNodeId, LinClass};
