//! Load-effective-address encoder: the address-form-directed `lea` that
//! covers register/memory/RIP-relative/symbol-reference forms.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_lea(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // lea r64, [base + disp]
            // LEA uses MOV encoding but with different semantics
            // lea r64, [rbp + disp] → 48 8D /r [ModR/M] [disp]
            //
            // Issue #994: this used to hand-roll the ModR/M byte directly
            // (`0x40 | (dest<<3) | base`), which is wrong whenever base's
            // low 3 bits are 100 (RSP or R12) — that r/m encoding is the
            // architectural *SIB-byte-follows* escape, not "register
            // indirect with r/m=100", so the CPU (and any correct
            // disassembler) consumes the next byte — meant to be the
            // displacement — as the SIB byte instead, corrupting this
            // instruction and desyncing every later instruction's byte
            // alignment. `lea r11, [rsp + env_off]` (closure env-pointer
            // materialisation) hit exactly this case. `emit_mem_base_disp`
            // is the shared, correct helper other opcodes already use for
            // the identical [base + disp] addressing form (it emits the
            // required 0x24-style SIB escape for RSP/R12); route through it
            // instead of duplicating (and mis-duplicating) the encoding here.
            let dest_id = reg64_from(*dest)? as u8;
            let base_id = reg64_from(*base)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, (base_id >> 3) != 0);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode
            emit_mem_base_disp(buf, dest_id & 7, base_id, *disp);
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp,
            },
        ] => {
            // lea r64, [base + index * scale + disp]
            // Uses SIB (Scale-Index-Base) byte format: SIB = scale (2 bits) | index (3 bits) | base (3 bits)
            let dest_id = reg64_from(*dest)? as u8;
            let base_id = reg64_from(*base)? as u8;
            let index_id = reg64_from(*index)? as u8;

            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };

            let rex_byte = rex(
                true,
                (dest_id >> 3) != 0,
                (index_id >> 3) != 0,
                (base_id >> 3) != 0,
            );

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            // Use emit_mem_sib_disp to handle ModR/M, SIB, and displacement encoding,
            // including R13/RBP escape (disp8=0 when base is RBP/R13 and disp=0).
            emit_mem_sib_disp(buf, dest_id & 7, base_id, index_id, scale_bits, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::SymbolRef { name, addend }] => {
            // Mode32: lea r32, [symbol] → 8D /r [absolute ModR/M] [disp32_placeholder]
            if inst.mode == InstrMode::Mode32 {
                let dest_reg = reg64_from(*dest)?;
                let byte_offset = lea_reg32_mem_abs32(buf, dest_reg);

                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }

            // Mode64: lea r64, [symbol] → 48 8D /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            // RIP-relative addressing: mod=00, r/m=5
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            // PA-R17-014 / #992: Defense-in-depth check: addend must fit in i32 for rel32 relocations
            if i32::try_from(*addend as i64 + PC32_FIELD_BIAS as i64).is_err() {
                return Err(EncodeError::Unsupported("lea rel32 addend overflows i32"));
            }

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the lea instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for lea r64, [rip + sym + addend]
        [Operand::Reg(dest), Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 48 8D /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            // PA-R17-014 / #992: Defense-in-depth check: addend must fit in i32 for rel32 relocations
            if i32::try_from(*addend as i64 + PC32_FIELD_BIAS as i64).is_err() {
                return Err(EncodeError::Unsupported("lea rel32 addend overflows i32"));
            }

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "lea operand shape not supported by this encoder",
        )),
    }
}
