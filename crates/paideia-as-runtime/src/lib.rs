//! paideia-as-runtime
//!
//! Linkable library exposing the paideia-as instruction IR and x86_64
//! encoder. Consumed by the paideia-as CLI and by paideia-os host processes
//! that emit code dynamically (WASM jail, JIT paths).
//!
//! `no_std + alloc` at the crate root; std is available only when a
//! downstream (e.g. the CLI) links us in a std context.

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod abi;
pub mod instruction;
pub mod node_id;

pub use instruction::{
    Cond, CpuFeature, EncodingHint, InstrMode, Instruction, IntWidth,
    Mnemonic, Operand, RegId, Scale, SegPrefix, SegReg,
};
pub use node_id::IrNodeId;
