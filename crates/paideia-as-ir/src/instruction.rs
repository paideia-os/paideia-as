//! Per-IR-node instruction payload + side-table.
//!
//! Types re-exported from paideia-as-runtime; InstructionSideTable stays
//! here because impl_named_side_table! is an ir-crate macro.

pub use paideia_as_runtime::instruction::{
    Cond, CpuFeature, EncodingHint, InstrMode, Instruction, IntWidth,
    Mnemonic, Operand, RegId, Scale, SegPrefix, SegReg,
};

use crate::node::IrNodeId;

crate::impl_named_side_table!(
    /// Side-table mapping IrNodeId → Instruction payload.
    pub struct InstructionSideTable, IrNodeId => Instruction
);
