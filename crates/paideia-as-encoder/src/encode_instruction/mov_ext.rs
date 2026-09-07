//! Sign/zero-extending MOV encoders: `movzx`, `movsx`, and the memory
//! helper `movzx_reg64_mem_base_disp`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;


/// Phase 6 m3-002: Encode MOVZX (move with zero-extend) instruction.
///
/// MOVZX r64, r/m8/r/m16/r/m32 — zero-extends smaller operand to 64-bit.
/// For now, we only support movzx rax, byte [rdi+offset] pattern used in field access.
///
/// Opcode: 0F B6 for r/m8 → r64, 0F B7 for r/m16 → r64, etc.
/// This is a placeholder implementation; full support deferred to future phase.
pub(super) fn encode_movzx(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 13 m6-001: MOVZX encoder supporting both register-to-register and memory-source.
    // For field access lowering: movzx rax, byte [rdi + offset] or movzx rax, word [rdi + offset]
    // Operands: [0] = dst (Reg), [1] = src (Reg or MemSib)
    //
    // Opcodes:
    // - 1 byte source: `REX.W 0F B6 /r` (movzx r64, r/m8)
    // - 2 byte source: `REX.W 0F B7 /r` (movzx r64, r/m16)

    if inst.operands.len() != 2 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Movzx,
            expected: 2,
            got: inst.operands.len(),
        });
    }

    // Extract destination register
    let dest_reg = match &inst.operands[0] {
        Operand::Reg(reg) => *reg,
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Movzx,
            });
        }
    };

    // Determine source width from encoding_hint
    let src_width = inst.encoding_hint.map(|h| h.operand_size).unwrap_or(1);

    match &inst.operands[1] {
        Operand::Reg(src_reg) => {
            // Register-to-register: already supported
            movzx_reg64(buf, reg64_from(dest_reg)?, reg64_from(*src_reg)?, src_width);
            Ok(EncodeOutput::new())
        }
        Operand::MemSib { base, index, disp, .. } => {
            // Memory source: movzx r64, [base + disp]
            if index.is_some() {
                return Err(EncodeError::Unsupported(
                    "MOVZX: indexed addressing not supported",
                ));
            }

            match src_width {
                1 | 2 => {
                    movzx_reg64_mem_base_disp(buf, reg64_from(dest_reg)?, reg64_from(*base)?, *disp, src_width);
                    Ok(EncodeOutput::new())
                }
                _ => {
                    Err(EncodeError::Unsupported(
                        "MOVZX: source width must be 1 or 2 bytes",
                    ))
                }
            }
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Movzx,
        }),
    }
}

/// Encode `movzx r64, [base + disp]` — zero-extend load from memory.
///
/// Instruction: REX.W opcode /r [disp]
/// Operand-size: determined by src_width parameter
/// - 1 byte (r/m8 → r64):  `REX.W 0F B6 /r` (movzx r64, byte [mem])
/// - 2 bytes (r/m16 → r64): `REX.W 0F B7 /r` (movzx r64, word [mem])
///
/// REX.W: always set (64-bit destination)
/// REX.R: set if dst in r8–r15
/// REX.B: set if base in r8–r15
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `movzx rax, byte [rdi]`: `48 0F B6 07`
/// - `movzx rax, word [rdi + 8]`: `48 0F B7 47 08`
pub(super) fn movzx_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32, src_width: u8) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    match src_width {
        1 => buf.bytes.push(0xB6), // movzx r64, r/m8
        2 => buf.bytes.push(0xB7), // movzx r64, r/m16
        _ => return, // Invalid width; caller should have checked
    }
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Phase 13 m6-001: Encode MOVSX (move with sign-extend), register-to-register or memory-source.
///
/// MOVSX r64, r/m8/r/m16/r/m32 — sign-extends a smaller source register or memory location into a
/// 64-bit destination. Used by the cast emit path for *widening signed* casts and field access.
///
/// Operands: `[Reg(dst), Reg(src)]` or `[Reg(dst), MemSib{...}]`. The source width (1, 2, or 4 bytes) is
/// taken from `encoding_hint.operand_size`; if no hint is present we default to
/// 4 bytes (the common `i32 as i64` widening).
///
/// Opcodes: width 1 → `0F BE`, width 2 → `0F BF`,
/// width 4 → `63` (MOVSXD), all with `REX.W`.
pub(super) fn encode_movsx(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if inst.operands.len() != 2 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Movsx,
            expected: 2,
            got: inst.operands.len(),
        });
    }

    let dest_reg = match &inst.operands[0] {
        Operand::Reg(reg) => *reg,
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Movsx,
            });
        }
    };

    let src_width = inst.encoding_hint.map(|h| h.operand_size).unwrap_or(4);

    match &inst.operands[1] {
        Operand::Reg(src_reg) => {
            // Register-to-register: movsx r64, reg
            if movsx_reg64(buf, reg64_from(dest_reg)?, reg64_from(*src_reg)?, src_width) {
                Ok(EncodeOutput::new())
            } else {
                Err(EncodeError::Unsupported(
                    "MOVSX: source width must be 1, 2, or 4 bytes",
                ))
            }
        }
        Operand::MemSib { base, index, disp, .. } => {
            // Memory source: movsx r64, [base + disp]
            if index.is_some() {
                return Err(EncodeError::Unsupported(
                    "MOVSX: indexed addressing not supported",
                ));
            }

            match src_width {
                1 | 2 | 4 => {
                    movsx_reg64_mem_base_disp(buf, reg64_from(dest_reg)?, reg64_from(*base)?, *disp, src_width);
                    Ok(EncodeOutput::new())
                }
                _ => {
                    Err(EncodeError::Unsupported(
                        "MOVSX: source width must be 1, 2, or 4 bytes",
                    ))
                }
            }
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Movsx,
        }),
    }
}
