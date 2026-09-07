//! AVX2 VEX-prefixed encoders: `vpxor`, `vpcmpeqb`, `vpmovmskb`,
//! `vmovdqu`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Phase R18 PA-R18-011 (issue #1004): Encode Vpxor ymm dst, ymm src1, ymm src2.
/// Expects [Reg(dst), Reg(src1), Reg(src2)], all YMM registers (RegId 37-52).
pub(super) fn encode_vpxor(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src1), Operand::Reg(src2)] => {
            crate::encode_vex::encode_vpxor_reg_reg_reg(buf, dst.0, src1.0, src2.0);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Vpxor,
        }),
    }
}

/// Phase R18 PA-R18-011 (issue #1004): Encode Vpcmpeqb ymm dst, ymm src1, ymm src2.
pub(super) fn encode_vpcmpeqb(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src1), Operand::Reg(src2)] => {
            crate::encode_vex::encode_vpcmpeqb_reg_reg_reg(buf, dst.0, src1.0, src2.0);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Vpcmpeqb,
        }),
    }
}

/// Phase R18 PA-R18-011 (issue #1004): Encode Vpmovmskb r32 dst, ymm src.
pub(super) fn encode_vpmovmskb(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            crate::encode_vex::encode_vpmovmskb_reg32_ymm(buf, dst.0, src.0);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Vpmovmskb,
        }),
    }
}

/// Phase R18 PA-R18-011 (issue #1004): Encode Vmovdqu ymm/[mem] dst, ymm/[mem] src.
/// paideia-as#1295-b: added RIP-relative memory operand shapes to unblock
/// paideia-os R21.M2 #832 (YMM-preservation fixture reads/writes YMM state
/// through .rodata / .bss labels via `[rip + label]` memory refs).
pub(super) fn encode_vmovdqu(inst: &Instruction, buf: &mut CodeBuffer, is_store: bool) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        // vmovdqu ymm dst, ymm src (register-to-register)
        [Operand::Reg(dst), Operand::Reg(src)] => {
            crate::encode_vex::encode_vmovdqu_ymm_ymm(buf, dst.0, src.0);
            Ok(EncodeOutput::new())
        }
        // vmovdqu ymm dst, [mem] src (load form, is_store=false)
        [Operand::Reg(dst), Operand::MemSib { base, index: None, scale: Scale::X1, disp }] if !is_store => {
            crate::encode_vex::encode_vmovdqu_ymm_mem(buf, dst.0, base.0, *disp);
            Ok(EncodeOutput::new())
        }
        // vmovdqu [mem] dst, ymm src (store form, is_store=true)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] if is_store => {
            crate::encode_vex::encode_vmovdqu_mem_ymm(buf, base.0, *disp, src.0);
            Ok(EncodeOutput::new())
        }
        // paideia-as#1295-b: vmovdqu ymm dst, [rip + sym + addend] (load, RIP-rel with symbol)
        [Operand::Reg(dst), Operand::MemRipRelSym { name, addend }] if !is_store => {
            // #1143: capture instruction start so disp_offset stays instruction-local
            // (RelocSite.byte_offset contract per the mov/lea patterns above).
            let start = buf.bytes.len();
            let disp_abs = crate::encode_vex::encode_vmovdqu_ymm_riprel(buf, dst.0, 0);
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: (disp_abs - start) as u32,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // paideia-as#1295-b: vmovdqu ymm dst, [rip + disp] (load, RIP-rel plain)
        [Operand::Reg(dst), Operand::MemRipRel { disp }] if !is_store => {
            crate::encode_vex::encode_vmovdqu_ymm_riprel(buf, dst.0, *disp);
            Ok(EncodeOutput::new())
        }
        // paideia-as#1295-b: vmovdqu [rip + sym + addend], ymm src (store, RIP-rel with symbol)
        [Operand::MemRipRelSym { name, addend }, Operand::Reg(src)] if is_store => {
            let start = buf.bytes.len();
            let disp_abs = crate::encode_vex::encode_vmovdqu_riprel_ymm(buf, 0, src.0);
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: (disp_abs - start) as u32,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // paideia-as#1295-b: vmovdqu [rip + disp], ymm src (store, RIP-rel plain)
        [Operand::MemRipRel { disp }, Operand::Reg(src)] if is_store => {
            crate::encode_vex::encode_vmovdqu_riprel_ymm(buf, *disp, src.0);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Vmovdqu { is_store },
        }),
    }
}
