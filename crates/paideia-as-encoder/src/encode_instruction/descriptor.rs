//! Descriptor-table loader encoders: `lgdt`, `lidt`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_lgdt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // lgdt [base + disp] - load GDT descriptor
    // Mode32 short-circuit: lgdt [symbol] with absolute 32-bit addressing
    if inst.mode == InstrMode::Mode32 {
        if let [Operand::SymbolRef { name, addend }] = inst.operands.as_slice() {
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0x15);
            buf.bytes.extend([0, 0, 0, 0]);
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::Abs32,
                addend: *addend,
            });
            return Ok(output);
        }
    }

    match inst.operands.as_slice() {
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // Valid form: [base] with optional displacement, no index
            let base_reg = reg64_from(*base)?;
            encode_descriptor_table_load(buf, base_reg, *disp, 2); // 2 = /2 for lgdt
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // lgdt [symbol] in Mode64 → 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            // RIP-relative addressing: mod=00, /2 for lgdt
            buf.bytes.push(0x15); // 0x05 | (2 << 3) = rip-relative with /2

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the lgdt instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for lgdt [rip + sym + addend]
        [Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0x15); // 0x05 | (2 << 3) = rip-relative with /2
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [
            Operand::MemSib {
                base: _,
                index: Some(_),
                scale: _,
                disp: _,
            },
        ] => {
            // Indexed form not supported
            Err(EncodeError::Unsupported("lgdt/lidt indexed form"))
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Lgdt,
        }),
    }
}

pub(super) fn encode_lidt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // lidt [base + disp] - load IDT descriptor
    match inst.operands.as_slice() {
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // Valid form: [base] with optional displacement, no index
            let base_reg = reg64_from(*base)?;
            encode_descriptor_table_load(buf, base_reg, *disp, 3); // 3 = /3 for lidt
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // lidt [symbol] → 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            // RIP-relative addressing: mod=00, /3 for lidt
            buf.bytes.push(0x1D); // 0x05 | (3 << 3) = rip-relative with /3

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the lidt instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for lidt [rip + sym + addend]
        [Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0x1D); // 0x05 | (3 << 3) = rip-relative with /3
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [
            Operand::MemSib {
                base: _,
                index: Some(_),
                scale: _,
                disp: _,
            },
        ] => {
            // Indexed form not supported
            Err(EncodeError::Unsupported("lgdt/lidt indexed form"))
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Lidt,
        }),
    }
}
