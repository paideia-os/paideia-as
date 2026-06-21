//! Text section emitter that consumes InstructionSideTable.
//!
//! Integrates paideia-as-encoder to encode Instruction records from the
//! InstructionSideTable into .text section machine code. Phase-4-m2-001
//! minimum: handles the 10-mnemonic catalog; future work expands coverage.
//!
//! Phase-4-m2-002: Threads InstructionSideTable through emit-stage to
//! track instruction offsets for DWARF .debug_line section reconstruction.

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocSite, encode_instruction};
use paideia_as_ir::{InstructionSideTable, IrNodeId};
use std::collections::HashMap;

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

/// Result of emitting text section: encoding stats, offset map, relocations, and bytes.
///
/// The offset map tracks the byte offset at which each instruction
/// was emitted, enabling DWARF .debug_line reconstruction (Phase-4-m2-002).
/// Relocations are collected from all encoded instructions (Phase-5-m4-004).
#[derive(Debug, Clone)]
pub struct EmitResult {
    /// Instruction count, instruction bytes, etc.
    pub encode_stats: EncodeStats,
    /// Map from IrNodeId to emitted byte offset within .text.
    /// Used by DWARF emit-stage to build .debug_line with post-rewrite offsets.
    pub offset_map: HashMap<IrNodeId, u64>,
    /// Relocation sites collected from instruction encoding.
    /// Each relocation specifies a symbol reference to be resolved at link time.
    /// Phase-5-m4-004: Used to populate .rela.text section in ELF emission.
    pub reloc_sites: Vec<RelocSite>,
}

/// Emit .text section content from an InstructionSideTable.
///
/// Iterates over all instruction entries in the table, encodes each
/// using the shared encoder, and appends bytes to the output buffer.
/// Returns encoding statistics, an offset map (IrNodeId → byte offset),
/// collected relocation sites, and any errors encountered.
///
/// Phase-4-m2-002: The offset map enables DWARF emit-stage to reconstruct
/// .debug_line with post-rewrite instruction offsets.
/// Phase-5-m4-004: Relocation sites are collected for linking .text references
/// to .rodata / .data symbols.
pub fn emit_text_from_instructions(
    table: &InstructionSideTable,
    output: &mut Vec<u8>,
) -> Result<EmitResult, TextEmitterError> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let mut offset_map = HashMap::new();
    let mut reloc_sites = Vec::new();

    // Iterate over all instructions in the side-table, tracking byte offsets.
    // We collect and sort entries by node_id to ensure deterministic order
    // across invocations (HashMap iteration is not ordered).
    let mut entries: Vec<_> = table.entries().iter().collect();
    entries.sort_by_key(|&(&node_id, _)| node_id);

    for (&node_id, instruction) in entries {
        let offset_before = buf.bytes.len() as u64;
        let encode_output = encode_instruction(instruction, &mut buf, &mut stats)
            .map_err(|e| TextEmitterError::EncodeError(format!("{:?}", e)))?;

        // Phase-5-m4-004: Collect relocation sites from this instruction.
        // Each RelocSite has a byte_offset relative to the instruction's start.
        // We adjust it to be relative to the start of .text section.
        for mut site in encode_output.reloc_sites {
            site.byte_offset = (offset_before as u32) + site.byte_offset;
            reloc_sites.push(site);
        }

        offset_map.insert(node_id, offset_before);
    }

    // Append the accumulated bytes to the output.
    output.extend_from_slice(&buf.bytes);
    Ok(EmitResult {
        encode_stats: stats,
        offset_map,
        reloc_sites,
    })
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
        let emit_result = result.unwrap();
        assert_eq!(output.len(), 0);
        assert!(emit_result.offset_map.is_empty());
        assert!(emit_result.reloc_sites.is_empty());
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
        let node_id = IrNodeId::new(1).unwrap();
        table.insert(node_id, inst);

        let mut output = Vec::new();
        let result = emit_text_from_instructions(&table, &mut output);
        assert!(result.is_ok());
        let emit_result = result.unwrap();
        // mov r64, r64 is 3 bytes: 48 89 <ModR/M>
        assert!(output.len() >= 3);
        // Offset map should track the instruction at offset 0
        assert!(emit_result.offset_map.contains_key(&node_id));
        assert_eq!(emit_result.offset_map[&node_id], 0);
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
        let node_id_1 = IrNodeId::new(1).unwrap();
        table.insert(node_id_1, inst1);

        // add rax, rcx
        let inst2 = Instruction {
            mnemonic: Mnemonic::Add,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))].into(),
            encoding_hint: None,
        };
        let node_id_2 = IrNodeId::new(2).unwrap();
        table.insert(node_id_2, inst2);

        let mut output = Vec::new();
        let result = emit_text_from_instructions(&table, &mut output);
        assert!(result.is_ok());
        let emit_result = result.unwrap();
        // Both instructions should be encoded and concatenated
        assert!(output.len() > 3);
        // Both nodes should have offsets
        assert!(emit_result.offset_map.contains_key(&node_id_1));
        assert!(emit_result.offset_map.contains_key(&node_id_2));
        // Second instruction should have offset > first
        let offset_1 = emit_result.offset_map[&node_id_1];
        let offset_2 = emit_result.offset_map[&node_id_2];
        assert!(offset_2 > offset_1);
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
        let emit_result =
            emit_text_from_instructions(&table, &mut output).expect("encoding should succeed");
        assert_eq!(emit_result.encode_stats.total, 2);
        // Verify offset map contains both instructions
        assert_eq!(emit_result.offset_map.len(), 2);
    }
}
