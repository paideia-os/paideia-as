//! Text section emitter that consumes InstructionSideTable.
//!
//! Integrates paideia-as-encoder to encode Instruction records from the
//! InstructionSideTable into .text section machine code. Phase-4-m2-001
//! minimum: handles the 10-mnemonic catalog; future work expands coverage.
//!
//! Phase-4-m2-002: Threads InstructionSideTable through emit-stage to
//! track instruction offsets for DWARF .debug_line section reconstruction.

use paideia_as_encoder::{CodeBuffer, EncodeStats, LabelFixup, RelocSite, encode_instruction};
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

/// Result of emitting text section: encoding stats, offset map, relocations, label fixups, and bytes.
///
/// The offset map tracks the byte offset at which each instruction
/// was emitted, enabling DWARF .debug_line reconstruction (Phase-4-m2-002).
/// Relocations are collected from all encoded instructions (Phase-5-m4-004).
/// Label fixups are collected from Jcc and Jmp instructions with label references (Phase-6-m4-004).
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
    /// Label fixup sites collected from Jcc/Jmp instructions with label references.
    /// Phase-6-m4-004: These are applied by cmd_build::patch_label_fixups() after
    /// all instructions have been encoded and label offsets are known.
    pub label_fixups: Vec<LabelFixup>,
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
/// Phase-7-m1-003: Records byte_offset_in_text for each instruction before encoding,
/// enabling precise relocation offset computation.
pub fn emit_text_from_instructions(
    table: &mut InstructionSideTable,
    output: &mut Vec<u8>,
) -> Result<EmitResult, TextEmitterError> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let mut offset_map = HashMap::new();
    let mut reloc_sites = Vec::new();
    let mut label_fixups = Vec::new();

    // Iterate over all instructions in the side-table, tracking byte offsets.
    // We collect and sort by (emission_order, node_id) to ensure deterministic order
    // across invocations (HashMap iteration is not ordered) and to break virtual-IrNodeId
    // collisions (#1140: sibling functions with identical synthetic IDs).
    let mut entries: Vec<(u32, IrNodeId)> =
        table.entries().iter().map(|(k, v)| (v.emission_order, *k)).collect();
    entries.sort();

    for (_, node_id) in entries {
        let offset_before = buf.bytes.len() as u32;

        // Phase-7-m1-003: Record byte_offset_in_text before encoding.
        // This is the ground truth for relocation offset computation.
        if let Some(inst_mut) = table.get_mut(node_id) {
            inst_mut.byte_offset_in_text = Some(offset_before);
        }

        // Get the immutable instruction for encoding
        let instruction = table.get(node_id).unwrap();
        let encode_output = encode_instruction(instruction, &mut buf, &mut stats)
            .map_err(|e| TextEmitterError::EncodeError(format!("{:?}", e)))?;

        // Phase-5-m4-004: Collect relocation sites from this instruction.
        // Each RelocSite has a byte_offset relative to the instruction's start.
        // We adjust it to be relative to the start of .text section.
        for mut site in encode_output.reloc_sites {
            site.byte_offset = offset_before + site.byte_offset;
            reloc_sites.push(site);
        }

        // Phase-6-m4-004: Collect label fixup sites from this instruction.
        // Each LabelFixup has a byte_offset relative to the instruction's start.
        // We adjust it to be relative to the start of .text section.
        for mut fixup in encode_output.label_fixups {
            fixup.byte_offset = offset_before + fixup.byte_offset;
            label_fixups.push(fixup);
        }

        offset_map.insert(node_id, offset_before as u64);
    }

    // Append the accumulated bytes to the output.
    output.extend_from_slice(&buf.bytes);
    Ok(EmitResult {
        encode_stats: stats,
        offset_map,
        reloc_sites,
        label_fixups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{InstrMode, Instruction, IrNodeId, Mnemonic, Operand, RegId};
    use smallvec::SmallVec;

    #[test]
    fn emit_empty_instruction_table_produces_empty_text() {
        let mut table = InstructionSideTable::new();
        let mut output = Vec::new();
        let result = emit_text_from_instructions(&mut table, &mut output);
        assert!(result.is_ok());
        let emit_result = result.unwrap();
        assert_eq!(output.len(), 0);
        assert!(emit_result.offset_map.is_empty());
        assert!(emit_result.reloc_sites.is_empty());
        assert!(emit_result.label_fixups.is_empty());
    }

    #[test]
    fn emit_single_mov_instruction() {
        let mut table = InstructionSideTable::new();

        // Create: mov rax, rbx
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))].into(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        let node_id = IrNodeId::new(1).unwrap();
        table.insert(node_id, inst);

        let mut output = Vec::new();
        let result = emit_text_from_instructions(&mut table, &mut output);
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
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        let node_id_1 = IrNodeId::new(1).unwrap();
        table.insert(node_id_1, inst1);

        // add rax, rcx
        let inst2 = Instruction {
            mnemonic: Mnemonic::Add,
            operands: vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))].into(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        let node_id_2 = IrNodeId::new(2).unwrap();
        table.insert(node_id_2, inst2);

        let mut output = Vec::new();
        let result = emit_text_from_instructions(&mut table, &mut output);
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
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        table.insert(IrNodeId::new(1).unwrap(), inst1);

        let inst2 = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        table.insert(IrNodeId::new(2).unwrap(), inst2);

        let mut output = Vec::new();
        let emit_result =
            emit_text_from_instructions(&mut table, &mut output).expect("encoding should succeed");
        assert_eq!(emit_result.encode_stats.total, 2);
        // Verify offset map contains both instructions
        assert_eq!(emit_result.offset_map.len(), 2);
    }

    #[test]
    fn sysv_call_with_args_emits_movs_before_call_and_ret_last() {
        // Issue #1099: Verify that arg MOVs sort before CALL and RET is the last byte.
        // This test simulates a 2-arg SysV call: mov rdi, imm; mov rsi, imm; call target; ret
        let mut table = InstructionSideTable::new();

        // First arg MOV: rdi = 0x1234 (uses ID 1_000_000 + 100*1 + 5 where reg 5 is RSI)
        // Actually, RDI is reg 5, RSI is reg 4 in the abi module
        // Let's use the correct reg IDs: we want mov rdi, mov rsi, call, ret
        let l = 1u32; // Lambda node ID

        // MOV rdi, 0x1234 at ID 1_000_000 + 100*1 + 5 (RDI reg)
        let mov_rdi_id = IrNodeId::new(1_000_000 + l * 100 + 5).unwrap(); // RDI = reg 5
        let mov_rdi = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: vec![Operand::Reg(RegId(5)), Operand::Imm64(0x1234)].into(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        table.insert(mov_rdi_id, mov_rdi);

        // MOV rsi, 0x5678 at ID 1_000_000 + 100*1 + 4 (RSI reg)
        let mov_rsi_id = IrNodeId::new(1_000_000 + l * 100 + 4).unwrap(); // RSI = reg 4
        let mov_rsi = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: vec![Operand::Reg(RegId(4)), Operand::Imm64(0x5678)].into(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        table.insert(mov_rsi_id, mov_rsi);

        // CALL at ID 1_050_000 + 100*1
        let call_id = IrNodeId::new(1_050_000 + l * 100).unwrap();
        let call_inst = Instruction {
            mnemonic: Mnemonic::Call,
            operands: vec![Operand::SymbolRef {
                name: "target".to_string(),
                addend: 0,
            }]
            .into(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        table.insert(call_id, call_inst);

        // RET at ID 1_150_000 + 100*1
        let ret_id = IrNodeId::new(1_150_000 + l * 100).unwrap();
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
                    emission_order: 0,
        };
        table.insert(ret_id, ret_inst);

        // Emit and verify byte order
        let mut output = Vec::new();
        let result = emit_text_from_instructions(&mut table, &mut output);
        assert!(result.is_ok(), "emission should succeed");

        let _emit_result = result.unwrap();
        assert!(output.len() > 0, "output should not be empty");

        // RET is 0xC3 — should be the last byte
        assert_eq!(output[output.len() - 1], 0xC3, "last byte should be RET (0xC3)");

        // CALL is 0xE8 (relative call) — should appear once and after MOVs
        let call_byte_count = output.iter().filter(|&&b| b == 0xE8).count();
        assert_eq!(call_byte_count, 1, "should have exactly one CALL (0xE8)");

        // Find the offset of CALL relative to the start
        let call_offset = output.iter().position(|&b| b == 0xE8).unwrap();

        // MOVs should appear before CALL
        // movabs reg64, imm64 is 0x48 0xB8+reg (10 bytes)
        // mov reg64, imm32 is 0x48 0xC7 0xC0+reg imm32 (7 bytes)
        // Check for either encoding
        let has_mov_before_call = output[..call_offset]
            .windows(2)
            .any(|w| {
                // B8 form: 0x48 0xB8-0xBF
                (w[0] == 0x48 && w[1] >= 0xB8 && w[1] <= 0xBF) ||
                // C7 form: 0x48 0xC7 (followed by ModR/M byte with reg bits in 0xC0-0xC7)
                (w[0] == 0x48 && w[1] == 0xC7)
            });
        assert!(
            has_mov_before_call,
            "should have at least one MOV before CALL (either 0x48 0xB8-0xBF or 0x48 0xC7 forms)"
        );

        // No bytes should exist after RET
        assert_eq!(
            output[output.len() - 1], 0xC3,
            "RET must be the final byte"
        );
    }
}
