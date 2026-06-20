//! Text section emitter that consumes InstructionSideTable.
//!
//! Integrates paideia-as-encoder to encode Instruction records from the
//! InstructionSideTable into .text section machine code. Phase-4-m2-001
//! minimum: handles the 10-mnemonic catalog; future work expands coverage.

use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};
use paideia_as_ir::InstructionSideTable;

/// Error type for text section emission.
#[derive(Debug, Clone)]
pub enum TextEmitterError {
    /// Instruction encoding failed.
    EncodeError(String),
}

impl std::fmt::Display for TextEmitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextEmitterError::EncodeError(msg) => write!(f, "encoding error: {}", msg),
        }
    }
}

impl std::error::Error for TextEmitterError {}

/// Emit .text section content from an InstructionSideTable.
///
/// Iterates over all instruction entries in the table, encodes each
/// using the shared encoder, and appends bytes to the output buffer.
/// Returns the encoding statistics and any errors encountered.
pub fn emit_text_from_instructions(
    table: &InstructionSideTable,
    output: &mut Vec<u8>,
) -> Result<EncodeStats, TextEmitterError> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();

    // Iterate over all instructions in the side-table.
    for instruction in table.entries().values() {
        encode_instruction(instruction, &mut buf, &mut stats)
            .map_err(|e| TextEmitterError::EncodeError(format!("{:?}", e)))?;
    }

    // Append the accumulated bytes to the output.
    output.extend_from_slice(&buf.bytes);
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{Instruction, IrNodeId, Mnemonic, Operand, RegId};
    use smallvec::SmallVec;

    #[test]
    fn emit_empty_instruction_table_produces_empty_text() {
        let table = InstructionSideTable::new();
        let mut output = Vec::new();
        let result = emit_text_from_instructions(&table, &mut output);
        assert!(result.is_ok());
        assert_eq!(output.len(), 0);
    }

    #[test]
    fn emit_single_mov_instruction() {
        let mut table = InstructionSideTable::new();

        // Create: mov rax, rbx
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))].into(),
            encoding_hint: None,
        };
        table.insert(IrNodeId::new(1).unwrap(), inst);

        let mut output = Vec::new();
        let result = emit_text_from_instructions(&table, &mut output);
        assert!(result.is_ok());
        // mov r64, r64 is 3 bytes: 48 89 <ModR/M>
        assert!(output.len() >= 3);
    }

    #[test]
    fn emit_multiple_instructions_concatenates() {
        let mut table = InstructionSideTable::new();

        // mov rax, rbx
        let inst1 = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))].into(),
            encoding_hint: None,
        };
        table.insert(IrNodeId::new(1).unwrap(), inst1);

        // add rax, rcx
        let inst2 = Instruction {
            mnemonic: Mnemonic::Add,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))].into(),
            encoding_hint: None,
        };
        table.insert(IrNodeId::new(2).unwrap(), inst2);

        let mut output = Vec::new();
        let result = emit_text_from_instructions(&table, &mut output);
        assert!(result.is_ok());
        // Both instructions should be encoded and concatenated
        assert!(output.len() > 3);
    }

    #[test]
    fn encode_stats_track_instruction_count() {
        let mut table = InstructionSideTable::new();

        let inst1 = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))].into(),
            encoding_hint: None,
        };
        table.insert(IrNodeId::new(1).unwrap(), inst1);

        let inst2 = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
        };
        table.insert(IrNodeId::new(2).unwrap(), inst2);

        let mut output = Vec::new();
        let stats =
            emit_text_from_instructions(&table, &mut output).expect("encoding should succeed");
        assert_eq!(stats.total, 2);
    }
}
