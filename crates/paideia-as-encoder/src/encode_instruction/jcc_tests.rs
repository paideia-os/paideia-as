//! Jcc-family and control-transfer round-trip tests.
//!
//! Extracted verbatim (paideia-as#1400) from the former single-file
//! `encode_instruction.rs` as the body of its `#[cfg(test)] mod jcc_tests`
//! block; every assertion is unchanged.

    use super::*;
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    use paideia_as_ir::{Cond as IrCond, InstrMode, Instruction, Mnemonic, Operand};

    // Test 1: Je with immediate (rel32) round-trips through iced-x86
    #[test]
    fn jcc_je_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Eq),
            operands: smallvec::smallvec![Operand::Imm64(0x100)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should be 6 bytes: 0F 84 <rel32_le>
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x84);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    // Test 2: Jne with immediate (rel32) round-trips
    #[test]
    fn jcc_jne_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Ne),
            operands: smallvec::smallvec![Operand::Imm64(0x1000)], // Large displacement
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x85);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    // Test 3: Jl (signed less than) round-trips
    #[test]
    fn jcc_jl_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Lt),
            operands: smallvec::smallvec![Operand::Imm64(0x200)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8C);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jl);
    }

    // Test 4: Jg (signed greater than) round-trips
    #[test]
    fn jcc_jg_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Gt),
            operands: smallvec::smallvec![Operand::Imm64(0x300)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8F);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jg);
    }

    // Test 5: Jle round-trips
    #[test]
    fn jcc_jle_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Le),
            operands: smallvec::smallvec![Operand::Imm64(0x400)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8E);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jle);
    }

    // Test 6: Jge round-trips
    #[test]
    fn jcc_jge_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Ge),
            operands: smallvec::smallvec![Operand::Imm64(-5000i64)], // Large negative displacement
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8D);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jge);
    }

    // Test 7: Jb (below, unsigned) round-trips
    #[test]
    fn jcc_jb_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Below),
            operands: smallvec::smallvec![Operand::Imm64(0x500)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x82);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jb);
    }

    // Test 8: Jbe round-trips
    #[test]
    fn jcc_jbe_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::BelowOrEqual),
            operands: smallvec::smallvec![Operand::Imm64(0x600)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x86);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jbe);
    }

    // Test 9: Ja round-trips
    #[test]
    fn jcc_ja_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Above),
            operands: smallvec::smallvec![Operand::Imm64(0x700)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x87);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ja);
    }

    // Test 10: Jae round-trips
    #[test]
    fn jcc_jae_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::AboveOrEqual),
            operands: smallvec::smallvec![Operand::Imm64(0x800)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x83);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jae);
    }

    // Test 11: Jz (alias for Je) round-trips
    #[test]
    fn jcc_jz_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Zero),
            operands: smallvec::smallvec![Operand::Imm64(0x100)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x84); // Same opcode as Je

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Decoder will recognize as Je (iced-x86 canonicalizes)
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    // Test 12: Jnz (alias for Jne) round-trips
    #[test]
    fn jcc_jnz_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NonZero),
            operands: smallvec::smallvec![Operand::Imm64(0x200)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x85); // Same opcode as Jne

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Decoder will recognize as Jne (iced-x86 canonicalizes)
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    // Test 13: Js round-trips
    #[test]
    fn jcc_js_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Sign),
            operands: smallvec::smallvec![Operand::Imm64(0x300)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x88);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Js);
    }

    // Test 14: Jns round-trips
    #[test]
    fn jcc_jns_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NotSign),
            operands: smallvec::smallvec![Operand::Imm64(0x400)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x89);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jns);
    }

    // Test 15: Jo round-trips
    #[test]
    fn jcc_jo_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Overflow),
            operands: smallvec::smallvec![Operand::Imm64(0x500)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x80);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jo);
    }

    // Test 16: Jno round-trips
    #[test]
    fn jcc_jno_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NotOverflow),
            operands: smallvec::smallvec![Operand::Imm64(0x600)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x81);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jno);
    }

    // Test 17: Je with label reference records fixup correctly
    #[test]
    fn jcc_je_label_records_fixup() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Eq),
            operands: smallvec::smallvec![Operand::LabelRef {
                name: "fail".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should emit 6 bytes with zero placeholder
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x84);
        assert_eq!(&buf.as_slice()[2..6], &[0, 0, 0, 0]);

        // Should record fixup
        assert_eq!(output.label_fixups.len(), 1);
        let fixup = &output.label_fixups[0];
        assert_eq!(fixup.label_name, "fail");
        assert_eq!(fixup.byte_offset, 2);
        assert_eq!(fixup.addend, 0);
        assert_eq!(fixup.instruction_size, 6);
    }

    // Test 18: Jmp with label reference records fixup correctly
    #[test]
    fn jmp_label_records_fixup() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::LabelRef {
                name: "end".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should emit 5 bytes: E9 + zero rel32
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.as_slice()[0], 0xE9);
        assert_eq!(&buf.as_slice()[1..5], &[0, 0, 0, 0]);

        // Should record fixup
        assert_eq!(output.label_fixups.len(), 1);
        let fixup = &output.label_fixups[0];
        assert_eq!(fixup.label_name, "end");
        assert_eq!(fixup.byte_offset, 1);
        assert_eq!(fixup.addend, 0);
        assert_eq!(fixup.instruction_size, 5);
    }

    // ── PA8 m3-003 (#827): width-aware mov reg, imm via MovSized ─────────
    //
    // The elaborator retargets `mov al, imm` → MovSized{W8} and
    // `mov eax, imm` → MovSized{W32} (see unsafe_walker::register_name_width).
    // These two tests pin the encoded bytes the retarget produces, so the
    // narrow r8/r32 immediate forms can never silently regress to the generic
    // 10-byte 64-bit move.

    // mov al/cl, imm8 → B0+rb imm8 (2 bytes, no REX.W).
    #[test]
    fn movsized_w8_reg_imm_emits_b0_plus_rb_imm8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W8,
            },
            // RegId(1) = rcx, so the 8-bit low byte is `cl`.
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Imm64(0x2a)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // `mov cl, 0x2a` → B0+1 2a = B1 2A.
        assert_eq!(buf.as_slice(), &[0xB1, 0x2A]);
    }

    // mov eax/ecx, imm32 → B8+rd imm32 (5 bytes, no REX.W, implicit zero-extend).
    #[test]
    fn movsized_w32_reg_imm_emits_b8_plus_rd_imm32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W32,
            },
            // RegId(1) = rcx, so the 32-bit sub-register is `ecx`.
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Imm64(0x2a)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // `mov ecx, 0x2a` → B8+1 2a 00 00 00 = B9 2A 00 00 00.
        assert_eq!(buf.as_slice(), &[0xB9, 0x2A, 0x00, 0x00, 0x00]);
    }

    // ── Phase 8 m5-001: supervisor TLB and timing mnemonics ─────────────────

    #[test]
    fn encode_rdtsc_zero_operands_emits_0f31() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdtsc,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 31 (2 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x31]);
    }

    #[test]
    fn encode_rdtsc_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdtsc,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x31]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdtsc);
    }

    #[test]
    fn encode_invlpg_mem_rdi_emits_0f017f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 /7 = 0F 01 3F (3 bytes)
        // ModR/M: mod=00, reg=7, rm=7 (rdi) = 00_111_111 = 0x3F
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x3F]);
    }

    #[test]
    fn encode_invlpg_mem_rdi_plus_8_emits_0f01777f08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 /7 + disp8 = 0F 01 7F 08 (4 bytes)
        // ModR/M: mod=01, reg=7, rm=7 (rdi) = 01_111_111 = 0x7F, disp=0x08
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x7F, 0x08]);
    }

    #[test]
    fn encode_invlpg_mem_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x3F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Invlpg);
    }

    // ── Phase 8 m5-002: general memory operand mov [base + disp] ──────────

    #[test]
    fn encode_mov_reg64_mem_base_disp0_emits_48_8b_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 07 (3 bytes, REX.W + mov r64, r/m64 + ModR/M)
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x07]);
    }

    #[test]
    fn encode_mov_reg64_mem_base_disp8_emits_48_8b_47_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 47 08 (4 bytes, disp8 form)
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x47, 0x08]);
    }

    #[test]
    fn encode_mov_reg64_mem_base_disp32_emits_48_8b_87_xxxx() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 256,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 87 00 01 00 00 (7 bytes, disp32 form for 256)
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x87, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_mov_mem_base_disp_reg64_emits_48_89_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 89 07 (3 bytes, REX.W + mov r/m64, r64 + ModR/M)
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x07]);
    }

    #[test]
    fn encode_mov_mem_base_disp_reg64_round_trips_load() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_mem_base_disp_reg64_round_trips_store() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 8,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // ── Phase 8 m5-004: comprehensive iced-x86 round-trip fixtures ──────────────
    // ≥12 fixtures covering m5-001 supervisor mnemonics + m5-002 memory operands

    #[test]
    fn encode_lgdt_rax_disp0_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(0), // rax
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lidt_rbx_disp32_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(3), // rbx
                index: None,
                scale: Scale::X1,
                disp: 256,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_wrmsr_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Wrmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Wrmsr);
    }

    #[test]
    fn encode_rdmsr_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdmsr);
    }

    #[test]
    fn encode_iretq_round_trips_m5_004() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iretq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Iretq);
    }

    #[test]
    fn encode_swapgs_round_trips_m5_004() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Swapgs,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
    }

    #[test]
    fn encode_int_0x21_round_trips_m5_004() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int,
            operands: smallvec::smallvec![Operand::Imm64(0x21)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int);
    }

    #[test]
    fn encode_mov_rax_mem_rsi_disp512_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(6), // rsi
                    index: None,
                    scale: Scale::X1,
                    disp: 512,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_mem_r13_disp8_r14_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::MemSib {
                    base: RegId(13), // r13
                    index: None,
                    scale: Scale::X1,
                    disp: 16,
                },
                Operand::Reg(RegId(14)), // r14
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_invlpg_rcx_disp128_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(1), // rcx
                index: None,
                scale: Scale::X1,
                disp: 128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Invlpg);
    }

    #[test]
    fn encode_rdtsc_m5_004_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdtsc,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdtsc);
    }

    // PA10-006a: ljmp immediate form tests
    #[test]
    fn encode_ljmp_imm_selector_offset_emits_ea_form() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x0008),     // selector
                Operand::Imm64(0x12345678), // offset
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: EA 78 56 34 12 08 00
        // EA = opcode, 78 56 34 12 = offset in LE, 08 00 = selector in LE
        assert_eq!(buf.as_slice(), &[0xEA, 0x78, 0x56, 0x34, 0x12, 0x08, 0x00]);
    }

    #[test]
    fn encode_ljmp_imm_produces_correct_length() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x0008),     // selector
                Operand::Imm64(0xdeadbeef), // offset
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify length: EA + imm32 + imm16 = 7 bytes
        assert_eq!(buf.len(), 7);

        // Verify first byte is EA opcode
        assert_eq!(buf.as_slice()[0], 0xEA);
    }

    #[test]
    fn encode_ljmp_imm_with_symbol_ref_emits_reloc() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x0008), // selector
                Operand::SymbolRef {
                    name: "kernel_entry".to_string(),
                    addend: 0,
                },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify bytecode: EA + 4 zero placeholder + selector
        assert_eq!(buf.as_slice(), &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00]);

        // Verify relocation site at byte offset 1 (after EA opcode)
        assert_eq!(output.reloc_sites.len(), 1);
        let reloc = &output.reloc_sites[0];
        assert_eq!(reloc.byte_offset, 1);
        assert_eq!(reloc.symbol, "kernel_entry");
        assert_eq!(reloc.kind, RelocKind::Abs32);
        assert_eq!(reloc.addend, 0);
    }

    // Phase R9 m2-001 (PA-R9-001): Push r64 tests
    #[test]
    fn encode_push_rax_emits_50() {
        // push rax → 50 (no REX needed for rax)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x50]);
    }

    #[test]
    fn encode_push_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Push);
        assert_eq!(instr.op0_register(), Register::RAX);
    }

    #[test]
    fn encode_push_rbx_emits_53() {
        // push rbx → 53 (no REX needed for rbx)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(3))], // rbx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x53]);
    }

    #[test]
    fn encode_push_r9_emits_41_51() {
        // push r9 → 41 51 (REX.B 51)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(9))], // r9
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x51]);
    }

    #[test]
    fn encode_push_r15_emits_41_57() {
        // push r15 → 41 57 (REX.B 57)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))], // r15
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x57]);
    }

    // Phase R9 m2-001 (PA-R9-001): Pop r64 tests
    #[test]
    fn encode_pop_rcx_emits_59() {
        // pop rcx → 59 (no REX needed for rcx)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))], // rcx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x59]);
    }

    #[test]
    fn encode_pop_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))], // rcx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Pop);
        assert_eq!(instr.op0_register(), Register::RCX);
    }

    #[test]
    fn encode_pop_rdx_emits_5a() {
        // pop rdx → 5a (no REX needed for rdx)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(2))], // rdx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x5a]);
    }

    #[test]
    fn encode_pop_r8_emits_41_58() {
        // pop r8 → 41 58 (REX.B 58)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))], // r8
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x58]);
    }

    #[test]
    fn encode_pop_r14_emits_41_5e() {
        // pop r14 → 41 5e (REX.B 5e)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(14))], // r14
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x5e]);
    }

    // Phase R9 m2-002 (PA-R9-002): Pushfq and Popfq tests
    #[test]
    fn encode_pushfq_emits_9c() {
        // pushfq → 9C
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pushfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x9C]);
    }

    #[test]
    fn encode_pushfq_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pushfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Pushfq);
    }

    #[test]
    fn encode_popfq_emits_9d() {
        // popfq → 9D
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Popfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x9D]);
    }

    #[test]
    fn encode_popfq_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Popfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Popfq);
    }

    // Phase R9 m2-003 (PA-R9-003): Int3 tests
    #[test]
    fn encode_int3_emits_cc() {
        // int3 → CC
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int3,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xCC]);
    }

    #[test]
    fn encode_int3_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int3,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int3);
    }

    // Phase R11 PA-R11-006 (issue #909): Div/Idiv r64 instruction tests
    #[test]
    fn encode_div_rax_emits_48_f7_f0() {
        // Mnemonic::Div with [Reg(rax)] → 48 F7 F0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xF0]);
    }

    #[test]
    fn encode_div_rcx_emits_48_f7_f1() {
        // Mnemonic::Div with [Reg(rcx)] → 48 F7 F1
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xF1]);
    }

    #[test]
    fn encode_div_r8_emits_49_f7_f0() {
        // Mnemonic::Div with [Reg(r8)] → 49 F7 F0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xF0]);
    }

    #[test]
    fn encode_div_r15_emits_49_f7_f7() {
        // Mnemonic::Div with [Reg(r15)] → 49 F7 F7
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xF7]);
    }

    #[test]
    fn encode_idiv_rax_emits_48_f7_f8() {
        // Mnemonic::Idiv with [Reg(rax)] → 48 F7 F8
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Idiv,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xF8]);
    }

    #[test]
    fn encode_idiv_r8_emits_49_f7_f8() {
        // Mnemonic::Idiv with [Reg(r8)] → 49 F7 F8
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Idiv,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xF8]);
    }

    #[test]
    fn encode_idiv_r15_emits_49_f7_ff() {
        // Mnemonic::Idiv with [Reg(r15)] → 49 F7 FF
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Idiv,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xFF]);
    }

    #[test]
    fn encode_div_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Div);
        assert_eq!(instr.op0_register(), Register::RCX);
    }

    // paideia-as#1398: mul r64 (REX.W F7 /4) — unsigned wide multiply.
    // rdx:rax = rax * r/m. Complements imul (signed low-64) and div (128÷64)
    // for wide-integer software emulation (postui#43 Fixed64 32×32-split).
    #[test]
    fn encode_mul_rax_emits_48_f7_e0() {
        // Mnemonic::Mul with [Reg(rax)] → 48 F7 E0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xE0]);
    }

    #[test]
    fn encode_mul_rcx_emits_48_f7_e1() {
        // Mnemonic::Mul with [Reg(rcx)] → 48 F7 E1
        // ModR/M breakdown: mod=11 (reg-direct), reg=/4 (100b opcode ext), rm=001b (rcx)
        // = 11 100 001 = 0xE1
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xE1]);
    }

    #[test]
    fn encode_mul_r8_emits_49_f7_e0() {
        // Mnemonic::Mul with [Reg(r8)] → 49 F7 E0
        // REX.WB (0x49) since r8 requires the B extension for the rm field.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xE0]);
    }

    #[test]
    fn encode_mul_r15_emits_49_f7_e7() {
        // Mnemonic::Mul with [Reg(r15)] → 49 F7 E7
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xE7]);
    }

    #[test]
    fn encode_mul_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mul);
        assert_eq!(instr.op0_register(), Register::RCX);
    }

    #[test]
    fn encode_mul_r8_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mul);
        assert_eq!(instr.op0_register(), Register::R8);
    }

    #[test]
    fn encode_mul_rejects_wrong_operand_shape() {
        // paideia-as#1398: mul r64 accepts exactly one register operand.
        // An immediate operand must produce EncodeError::OperandShape.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mul,
            operands: smallvec::smallvec![Operand::Imm64(0x1234)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Mul }));
    }

    #[test]
    fn encode_ltr_ax_emits_0f_00_d8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x00, 0xD8]);
    }

    #[test]
    fn encode_ltr_cx_emits_0f_00_d9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x00, 0xD9]);
    }

    #[test]
    fn encode_ltr_r8_emits_41_0f_00_d8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x00, 0xD8]);
    }

    #[test]
    fn encode_ltr_r10_emits_41_0f_00_da() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(10))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x00, 0xDA]);
    }

    #[test]
    fn encode_ltr_r15_emits_41_0f_00_df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x00, 0xDF]);
    }

    #[test]
    fn encode_ltr_r10_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(10))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ltr);
        // Note: iced_x86 decodes the r/m16 operand as R10D when using REX.B in 64-bit mode.
        // The bytes are correct (41 0F 00 DA); iced_x86's width interpretation varies.
        // Verify the register index is 10 (either R10D, R10W, or R10 depending on decoder version).
        let reg = instr.op0_register();
        assert!(reg == Register::R10W || reg == Register::R10D,
                "Expected R10W or R10D, got {:?}", reg);
    }

    #[test]
    fn encode_ltr_rejects_imm_operand() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Ltr }));
    }

    // Phase R13 PA-R13-003 (issue #916): XCHG tests
    #[test]
    fn encode_xchg_rdi_rax_emits_48_87_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x87, 0x07]);
    }

    #[test]
    fn encode_xchg_rdi_disp8_r10_emits_4c_87_57_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
                Operand::Reg(RegId(10)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x4C, 0x87, 0x57, 0x08]);
    }

    #[test]
    fn encode_xchg_r8_rax_emits_49_87_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0x87, 0x00]);
    }

    #[test]
    fn encode_xchg_rdi_r15_emits_4c_87_3f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(15)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x4C, 0x87, 0x3F]);
    }

    #[test]
    fn encode_xchg_rsp_rax_emits_48_87_04_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x87, 0x04, 0x24]);
    }

    #[test]
    fn encode_xchg_rbp_rax_emits_48_87_45_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x87, 0x45, 0x00]);
    }

    #[test]
    fn encode_xchg_rdi_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Xchg);
    }

    // Phase R13 PA-R13-004 (issue #917): LOCK CMPXCHG tests
    #[test]
    fn encode_lock_cmpxchg_rdi_rcx_emits_f0_48_0f_b1_0f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xB1, 0x0F]);
    }

    #[test]
    fn encode_lock_cmpxchg_rdi_disp8_r10_emits_f0_4c_0f_b1_57_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
                Operand::Reg(RegId(10)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x4C, 0x0F, 0xB1, 0x57, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg_r8_rcx_emits_f0_49_0f_b1_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x49, 0x0F, 0xB1, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg_rsp_rax_emits_f0_48_0f_b1_04_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xB1, 0x04, 0x24]);
    }

    #[test]
    fn encode_lock_cmpxchg_rdi_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmpxchg);
        assert!(instr.has_lock_prefix());
    }

    // Phase R13 PA-R13-005 (issue #918): MFENCE tests
    #[test]
    fn encode_mfence_emits_0f_ae_f0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0xF0]);
    }

    #[test]
    fn encode_mfence_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mfence);
    }

    #[test]
    fn encode_mfence_rejects_operand() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::Mfence, expected: 0, .. }));
    }

    // Phase R13 PA-R13-007: fxsave/fxrstor byte-exact tests
    #[test]
    fn encode_fxsave_rdi_emits_0f_ae_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x07]);
    }

    #[test]
    fn encode_fxsave_rdi_disp8_emits_0f_ae_47_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x47, 0x08]);
    }

    #[test]
    fn encode_fxsave_r8_emits_41_0f_ae_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x00]);
    }

    #[test]
    fn encode_fxsave_rsp_emits_0f_ae_04_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x04, 0x24]);
    }

    #[test]
    fn encode_fxsave_rbp_emits_0f_ae_45_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x45, 0x00]);
    }

    #[test]
    fn encode_fxsave_r15_disp32_emits_41_0f_ae_87_disp32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x87, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_fxrstor_rdi_emits_0f_ae_0f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x0F]);
    }

    #[test]
    fn encode_fxrstor_rdi_disp8_emits_0f_ae_4f_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x4F, 0x08]);
    }

    #[test]
    fn encode_fxrstor_r8_emits_41_0f_ae_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x08]);
    }

    #[test]
    fn encode_fxrstor_rsp_emits_0f_ae_0c_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x0C, 0x24]);
    }

    #[test]
    fn encode_fxrstor_rbp_emits_0f_ae_4d_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x4D, 0x00]);
    }

    #[test]
    fn encode_fxrstor_r15_disp32_emits_41_0f_ae_8f_disp32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x8F, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_fxsave_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Fxsave);
    }

    #[test]
    fn encode_fxrstor_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Fxrstor);
    }

    #[test]
    fn encode_fxsave_reg_operand_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Fxsave }));
    }

    // Phase v0.21-006 (#1282): xsaveopt / xrstor byte-exact tests.
    //
    // Encoders landed under PA-R15-m4-005 (#1022) and are empirically exercised
    // by paideia-os R21.M1 XSAVE (src/kernel/core/cpu/xsave.pdx — xsave_save_for
    // / xsave_restore_for). Adds the SDM Vol 2 opcode-map witnesses that the
    // fxsave/fxrstor tests above already provide for /0 and /1: xsaveopt is /6
    // and xrstor is /5, sharing the 0F AE prefix. Coverage matches fxsave: rdi
    // (mod=00), rdi+disp8 (mod=01), r8 (REX.B, mod=00), rsp (SIB=24), rbp
    // (mod=01 disp8=00), r15+disp32 (REX.B + mod=10).
    #[test]
    fn encode_xsaveopt_rdi_emits_0f_ae_37() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x37]);
    }

    #[test]
    fn encode_xsaveopt_rdi_disp8_emits_0f_ae_77_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x77, 0x08]);
    }

    #[test]
    fn encode_xsaveopt_r8_emits_41_0f_ae_30() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x30]);
    }

    #[test]
    fn encode_xsaveopt_rsp_emits_0f_ae_34_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x34, 0x24]);
    }

    #[test]
    fn encode_xsaveopt_rbp_emits_0f_ae_75_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x75, 0x00]);
    }

    #[test]
    fn encode_xsaveopt_r15_disp32_emits_41_0f_ae_b7_disp32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0xB7, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_xrstor_rdi_emits_0f_ae_2f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x2F]);
    }

    #[test]
    fn encode_xrstor_rdi_disp8_emits_0f_ae_6f_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x6F, 0x08]);
    }

    #[test]
    fn encode_xrstor_r8_emits_41_0f_ae_28() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x28]);
    }

    #[test]
    fn encode_xrstor_rsp_emits_0f_ae_2c_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x2C, 0x24]);
    }

    #[test]
    fn encode_xrstor_rbp_emits_0f_ae_6d_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x6D, 0x00]);
    }

    #[test]
    fn encode_xrstor_r15_disp32_emits_41_0f_ae_af_disp32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0xAF, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_xsaveopt_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Xsaveopt);
    }

    #[test]
    fn encode_xrstor_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Xrstor);
    }

    #[test]
    fn encode_xsaveopt_reg_operand_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xsaveopt,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Xsaveopt }));
    }

    #[test]
    fn encode_xrstor_reg_operand_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xrstor,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Xrstor }));
    }

    // Error-shape tests for new instructions
    #[test]
    fn encode_xchg_reg_reg_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Xchg }));
    }

    #[test]
    fn encode_lock_cmpxchg_reg_reg_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg }));
    }

    // Phase R16 PA-R16-003 (issue #969): LOCK CMPXCHG32 tests
    #[test]
    fn encode_lock_cmpxchg32_rax_ecx_emits_f0_0f_b1_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x0F, 0xB1, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg32_r8_ecx_emits_rex_b() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x41, 0x0F, 0xB1, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg32_rax_r15d_emits_rex_r() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(15)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x44, 0x0F, 0xB1, 0x38]);
    }

    #[test]
    fn encode_lock_cmpxchg32_r15_disp8_r10d_emits_rex_rb() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
                Operand::Reg(RegId(10)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x45, 0x0F, 0xB1, 0x57, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg32_rsp_eax_emits_sib() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x0F, 0xB1, 0x04, 0x24]);
    }

    #[test]
    fn encode_lock_cmpxchg32_r13_disp0_eax_forces_disp8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x41, 0x0F, 0xB1, 0x45, 0x00]);
    }

    #[test]
    fn encode_lock_cmpxchg32_rax_ecx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmpxchg);
        assert!(instr.has_lock_prefix());
    }

    #[test]
    fn encode_lock_cmpxchg32_reg_reg_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg32 }));
    }

    // Phase R16 PA-R16-004 (issue #970): lock cmpxchg16b tests

    #[test]
    fn encode_lock_cmpxchg16b_rdi_emits_f0_48_0f_c7_0f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC7, 0x0F]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_r15_disp8_emits_f0_49_0f_c7_4f_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x49, 0x0F, 0xC7, 0x4F, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_rsp_emits_f0_48_0f_c7_0c_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC7, 0x0C, 0x24]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_r13_disp0_emits_f0_49_0f_c7_4d_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x49, 0x0F, 0xC7, 0x4D, 0x00]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmpxchg16b);
        assert!(instr.has_lock_prefix());
    }

    #[test]
    fn encode_lock_cmpxchg16b_reg_operand_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg16b }));
    }

    #[test]
    fn encode_lock_cmpxchg16b_sib_with_index_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg16b }));
    }

    #[test]
    fn encode_xchg_imm_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Xchg }));
    }

    // PA-R15-009a: jmp_mem_sib_no_base_indexed test suite (10 tests)

    #[test]
    fn encode_jmp_mem_sym_indexed_rax_8x_emits_correct_bytes() {
        // Test 1: [sym + rax*8] → FF 24 C5 00 00 00 00 (no REX.X)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "jump_table".to_string(),
                addend: 0,
                index: RegId(0), // rax
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xFF, 0x24, 0xC5, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "jump_table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Abs32);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rcx_1x_emits_correct_scale() {
        // Test 2: [sym + rcx*1] → FF 24 0D ... (scale=0 in SIB)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "vtable".to_string(),
                addend: 0,
                index: RegId(1), // rcx
                scale: Scale::X1,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0xFF);
        assert_eq!(buf.as_slice()[1], 0x24);
        // SIB: scale=0 (00), index=rcx(1) (<<3), base=101
        // (0 << 6) | (1 << 3) | 0b101 = 0x0D
        assert_eq!(buf.as_slice()[2], 0x0D);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rbx_4x_emits_correct_scale() {
        // Test 3: [sym + rbx*4] → FF 24 9D ... (scale=2 in SIB)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "dispatch".to_string(),
                addend: 0,
                index: RegId(3), // rbx
                scale: Scale::X4,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0xFF);
        assert_eq!(buf.as_slice()[1], 0x24);
        // SIB: scale=2 (10), index=rbx(3) (<<3), base=101
        // (2 << 6) | (3 << 3) | 0b101 = 0x9D
        assert_eq!(buf.as_slice()[2], 0x9D);
        assert_eq!(output.reloc_sites.len(), 1);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rdi_2x_emits_correct_scale() {
        // Test 4: [sym + rdi*2] → FF 24 7D ... (scale=1 in SIB)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "handlers".to_string(),
                addend: 0,
                index: RegId(7), // rdi
                scale: Scale::X2,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0xFF);
        assert_eq!(buf.as_slice()[1], 0x24);
        // SIB: scale=1 (01), index=rdi(7) (<<3), base=101
        // (1 << 6) | (7 << 3) | 0b101 = 0x7D
        assert_eq!(buf.as_slice()[2], 0x7D);
        assert_eq!(output.reloc_sites.len(), 1);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_r8_8x_emits_rex_x() {
        // Test 5: [sym + r8*8] → 42 FF 24 C5 ... (REX.X prefix)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "table".to_string(),
                addend: 0,
                index: RegId(8), // r8
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0x42); // REX.X
        assert_eq!(buf.as_slice()[1], 0xFF);
        assert_eq!(buf.as_slice()[2], 0x24);
        // SIB with r8 (index_id=8, low 3 bits=0): (3<<6)|(0<<3)|0b101 = 0xC5
        assert_eq!(buf.as_slice()[3], 0xC5);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 4); // disp32 at byte 4 with REX
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_r15_4x_emits_rex_x() {
        // Test 6: [sym + r15*4] → 42 FF 24 BD ... (REX.X prefix)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "jumptbl".to_string(),
                addend: 0,
                index: RegId(15), // r15
                scale: Scale::X4,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0x42); // REX.X
        assert_eq!(buf.as_slice()[1], 0xFF);
        assert_eq!(buf.as_slice()[2], 0x24);
        // SIB with r15 (index_id=15, low 3 bits=7): (2<<6)|(7<<3)|0b101 = 0xBD
        assert_eq!(buf.as_slice()[3], 0xBD);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 4);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_non_zero_addend_flows_into_reloc() {
        // Test 7: Non-zero addend flows verbatim into RelocSite
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "handler_table".to_string(),
                addend: 8,
                index: RegId(2), // rdx
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].addend, 8);
        assert_eq!(output.reloc_sites[0].symbol, "handler_table");
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rsp_as_index_rejects() {
        // Test 8: [sym + rsp*8] → EncodeError::InvalidOperand (RSP cannot be index)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "forbidden".to_string(),
                addend: 0,
                index: RegId(4), // rsp (id 4)
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::InvalidOperand(_)));
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rax_8x_round_trips_iced() {
        // Test 9: Iced roundtrip: rax*8 form disassembles correctly
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "table".to_string(),
                addend: 0,
                index: RegId(0), // rax
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_r15_4x_round_trips_iced() {
        // Test 10: Iced roundtrip: r15*4 form preserves REX.X
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "handlers".to_string(),
                addend: 0,
                index: RegId(15), // r15
                scale: Scale::X4,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }
