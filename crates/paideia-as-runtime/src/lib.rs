//! paideia-as-runtime: runtime-compatible core types for instruction encoding.
//!
//! This crate provides no_std-compatible types that are foundational to
//! runtime environments: Instruction, Mnemonic, IrNodeId. Types in this
//! crate have no dependence on elaboration, parsing, or type-checking — only
//! on x86_64 encoding concerns.

#![no_std]
extern crate alloc;

pub mod instruction;
pub mod node_id;

pub use instruction::{
    Cond, CpuFeature, EncodingHint, InstrMode, Instruction, IntWidth, Mnemonic,
    Operand, RegId, Scale, SegPrefix, SegReg,
};
pub use node_id::IrNodeId;
