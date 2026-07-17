//! Per-IR-node instruction payload + side-table.
//!
//! This module re-exports the instruction types from paideia-as-runtime
//! and defines the IR-side side-table for instruction metadata.

// Re-export all instruction types from runtime crate.
pub use paideia_as_runtime::instruction::{
    Cond, CpuFeature, EncodingHint, InstrMode, Instruction, IntWidth, Mnemonic, Operand, RegId,
    Scale, SegPrefix, SegReg,
};

use crate::node::IrNodeId;

crate::impl_named_side_table!(
    /// Side-table mapping IrNodeId → Instruction payload.
    ///
    /// Keeps IrNodeData ≤ 48 bytes (const_assert pinned).
    pub struct InstructionSideTable, IrNodeId => Instruction
);
