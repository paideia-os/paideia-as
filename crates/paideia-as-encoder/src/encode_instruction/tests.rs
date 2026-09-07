//! In-file dispatcher/round-trip tests.
//!
//! Extracted verbatim (paideia-as#1400) from the former single-file
//! `encode_instruction.rs` as the body of its `#[cfg(test)] mod tests`
//! block; every assertion is unchanged.

    use super::*;
    use paideia_as_ir::{InstrMode, Instruction, Mnemonic, Operand, RegId, Scale, SegReg};

    #[test]
    fn encode_mov_rax_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
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
    fn encode_not_rax_emits_48_f7_d0() {
        // Mnemonic::Not with [Reg(rax)] dispatches to not_reg64 → 48 F7 D0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Not,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xD0]);
    }

    #[test]
    fn encode_not_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Not,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Not);
    }

    #[test]
    fn encode_mov_sized_w32_dispatches_to_b8_imm32() {
        // Mnemonic::MovSized { W32 } with [Reg(rax), Imm64(42)] → B8 2A 00 00 00.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W32,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB8, 0x2A, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn encode_mov_sized_w16_dispatches_to_66_b8_imm16() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W16,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x66, 0xB8, 0x2A, 0x00]);
    }

    #[test]
    fn encode_mov_sized_w8_dispatches_to_b0_imm8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W8,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB0, 0x2A]);
    }

    #[test]
    fn encode_mov_sized_w64_delegates_to_generic_mov() {
        // W64 must reproduce the established 64-bit immediate move (48 B8 ...).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W64,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(
            buf.as_slice(),
            &[0x48, 0xB8, 0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn encode_cmpsized_al_imm8_short_form() {
        // Mnemonic::CmpSized { W8 } with [Reg(RAX), Imm64(0)] → 3C 00 (AL-implicit short form).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::CmpSized {
                width: IntWidth::W8,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x3C, 0x00]);
    }

    #[test]
    fn encode_cmpsized_bl_imm8_modrm_form() {
        // Mnemonic::CmpSized { W8 } with [Reg(RBX), Imm64(0)] → 80 FB 00 (ModR/M form for BL).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::CmpSized {
                width: IntWidth::W8,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(3)), Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x80, 0xFB, 0x00]);
    }

    #[test]
    fn encode_cmpsized_r10b_imm8_with_rex() {
        // Mnemonic::CmpSized { W8 } with [Reg(R10), Imm64(5)] → 41 80 FA 05 (REX.B + ModR/M form for R10B).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::CmpSized {
                width: IntWidth::W8,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(10)), Operand::Imm64(5)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x80, 0xFA, 0x05]);
    }

    #[test]
    fn encode_cmp_rax_zero_still_emits_64bit() {
        // Regression test: existing Mnemonic::Cmp with [Reg(RAX), Imm64(0)] must still produce
        // the 64-bit form: 48 83 F8 00 (not the 8-bit 3C 00).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cmp,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xF8, 0x00]);
    }

    #[test]
    fn encode_movsx_default_width_emits_48_63() {
        // Mnemonic::Movsx with [Reg(rax), Reg(rcx)] and no hint defaults to a
        // 4-byte source → MOVSXD → 48 63 C1.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Movsx,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x63, 0xC1]);
    }

    #[test]
    fn encode_movsx_width1_via_hint_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
        use paideia_as_ir::EncodingHint;

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Movsx,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: Some(EncodingHint {
                opcode: 0x0FBE,
                operand_size: 1,
            }),
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xBE, 0xC1]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Movsx);
    }

    #[test]
    fn encode_mov_rax_imm64_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)),
                Operand::Imm64(0x1234567890ABCDEF)
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

    // Phase 15 m3-001: Mode32 dispatch tests
    #[test]
    fn mov_eax_imm32_mode32_emits_b8_no_rex() {
        // mov eax, 0x83 in 32-bit mode → B8 83 00 00 00 (no REX)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x83)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB8, 0x83, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mov_ecx_zero_mode32_emits_b9_zero() {
        // mov ecx, 0x00 in 32-bit mode → B9 00 00 00 00 (no REX)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Imm64(0x00)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB9, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mov_r8d_imm32_mode32_emits_rex_b_b8() {
        // mov r8d, 0x40000083 in 32-bit mode → 41 B8 83 00 00 40 (REX.B for r8)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Imm64(0x40000083)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0xB8, 0x83, 0x00, 0x00, 0x40]);
    }

    #[test]
    fn mov_eax_ecx_mode32_emits_89_c8() {
        // mov eax, ecx in 32-bit mode → 89 C8 (store form per AC specs)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x89, 0xC8]);
    }

    #[test]
    fn mov_eax_imm64_overflow_mode32_yields_e0501() {
        // mov eax, 0x1_0000_0000 in 32-bit mode → E0019 error (64-bit imm)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1_0000_0000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err());
        match result {
            Err(EncodeError::Unsupported(msg)) => {
                assert!(msg.contains("E0019"));
            }
            _ => panic!("Expected E0019 error for 64-bit immediate in 32-bit mode"),
        }
    }

    #[test]
    fn mov_rax_imm_mode32_yields_e0501() {
        // mov rax, 0x83 with Mode32 → E0019 error (64-bit destination)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W64,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x83)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err());
        match result {
            Err(EncodeError::Unsupported(msg)) => {
                assert!(msg.contains("E0019"));
            }
            _ => panic!("Expected E0019 error for 64-bit destination in 32-bit mode"),
        }
    }

    #[test]
    fn mov_eax_ecx_mode32_round_trips_through_iced_x86_32bit_decoder() {
        // Verify 32-bit mov eax, ecx round-trips with iced-x86 32-bit decoder
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Decode in 32-bit mode
        let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_add_rax_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_sub_rax_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_ret_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ret,
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
        assert_eq!(instr.mnemonic(), IcedMnem::Ret);
    }

    #[test]
    fn encode_rep_movsb_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsb,
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
        assert_eq!(instr.mnemonic(), IcedMnem::Movsb);
    }

    #[test]
    fn encode_rep_movsb_rejects_operand() {
        // PA-R13-011 (#940): rep movsb must not have any operands.
        // This test verifies that rep_movsb rax; correctly fails with OperandCount error.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsb,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::RepMovsb, expected: 0, .. }));
    }

    #[test]
    fn encode_rep_stosq_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify byte sequence: F3 48 AB
        assert_eq!(buf.as_slice(), &[0xF3, 0x48, 0xAB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Stosq);
    }

    #[test]
    fn encode_rep_stosq_rejects_operand() {
        // PA-R13-012 (#941): rep stosq must not have any operands.
        // This test verifies that rep_stosq rax; correctly fails with OperandCount error.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosq,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::RepStosq, expected: 0, .. }));
    }

    #[test]
    fn encode_rep_stosb_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosb,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify byte sequence: F3 AA
        assert_eq!(buf.as_slice(), &[0xF3, 0xAA]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Stosb);
    }

    #[test]
    fn encode_rep_stosb_rejects_operand() {
        // #1228: rep stosb must not have any operands (AL/RCX/RDI implicit).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosb,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::RepStosb, expected: 0, .. }));
    }

    #[test]
    fn encode_rep_movsq_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify byte sequence: F3 48 A5
        assert_eq!(buf.as_slice(), &[0xF3, 0x48, 0xA5]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Movsq);
    }

    #[test]
    fn encode_rep_movsq_rejects_operand() {
        // #1228: rep movsq must not have any operands (RSI/RDI/RCX implicit).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsq,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::RepMovsq, expected: 0, .. }));
    }

    #[test]
    fn encode_indexed_load_via_mov_dispatches_correctly() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(6),        // rsi
                    index: Some(RegId(7)), // rdi
                    scale: Scale::X8,
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

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_reg_mem_disp_plain_mov_defaults_to_w64() {
        // PA-R16-007: plain Mov with MemDisp now supported, defaults to W64
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemDisp { disp: 0x1000 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_ok(), "mov rax, [0x1000] should now be supported");
        // Verify it encodes as W64 (with REX.W)
        assert_eq!(
            buf.as_slice(),
            &[0x48, 0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
            "plain Mov with MemDisp defaults to W64"
        );
    }

    // ── Tightened instruction encoding tests ────────────────────

    #[test]
    fn encode_add_with_small_imm_uses_8bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 8-bit immediate form (4 bytes: REX.W 83 /0 imm8)
        assert_eq!(buf.len(), 4);
        assert_eq!(stats.tightened, 1, "Expected one tightening for small imm8");

        // Verify with iced
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_add_with_imm_fitting_in_i32_uses_32bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 32-bit immediate form (7 bytes: REX.W 81 /0 imm32)
        assert_eq!(buf.len(), 7);
        assert_eq!(stats.tightened, 1, "Expected one tightening for i32 imm");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_sub_with_small_imm_uses_8bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 8-bit immediate form (4 bytes: REX.W 83 /5 imm8)
        assert_eq!(buf.len(), 4);
        assert_eq!(stats.tightened, 1, "Expected one tightening for small imm8");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_sub_with_imm_fitting_in_i32_uses_32bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 32-bit immediate form (7 bytes: REX.W 81 /5 imm32)
        assert_eq!(buf.len(), 7);
        assert_eq!(stats.tightened, 1, "Expected one tightening for i32 imm");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_sub_rax_5_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(5)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
        assert_eq!(instr.immediate32(), 5);
    }

    #[test]
    fn encode_jcc_with_rel8_disp_uses_rel8_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(paideia_as_ir::Cond::Eq),
            operands: smallvec::smallvec![Operand::Imm64(50)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use rel8 form (2 bytes: 0x74 disp8)
        assert_eq!(buf.len(), 2);
        assert_eq!(stats.tightened, 1, "Expected one tightening for rel8 Jcc");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    #[test]
    fn encode_jcc_with_large_disp_uses_rel32_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(paideia_as_ir::Cond::Ne),
            operands: smallvec::smallvec![Operand::Imm64(0x1000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use rel32 form (6 bytes: 0x0F 0x85 disp32)
        assert_eq!(buf.len(), 6);
        assert_eq!(stats.tightened, 0, "Expected no tightening for large disp");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    #[test]
    fn encode_stats_counts_tightening() {
        let mut stats = EncodeStats::new();
        assert_eq!(stats.tightened, 0);
        assert_eq!(stats.total, 0);

        stats.record_instruction();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.tightened, 0);

        stats.record_tightening();
        assert_eq!(stats.tightened, 1);
        assert_eq!(stats.total, 1);

        stats.record_instruction();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.tightened, 1);
    }

    // ── Phase-5 m2-002: zero-operand control + sync instructions ────────

    #[test]
    fn encode_nop_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Nop,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x90]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Nop);
    }

    #[test]
    fn encode_hlt_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Hlt,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xF4]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Hlt);
    }

    #[test]
    fn encode_cli_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cli,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xFA]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cli);
    }

    #[test]
    fn encode_sti_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sti,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xFB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sti);
    }

    #[test]
    fn encode_swapgs_round_trips_through_iced_x86() {
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

        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0xF8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
    }

    #[test]
    fn encode_cpuid_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cpuid,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0xA2]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cpuid);
    }

    // ── I/O port instruction tests (phase-5 m2-003) ──────────────

    #[test]
    fn encode_in_al_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 1 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // al
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEC]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_ax_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 2 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // ax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x66, 0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_eax_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 4 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // eax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_out_dx_al_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 1 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // al
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEE]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_ax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 2 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // ax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x66, 0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_eax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 4 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // eax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    // ── Phase-5 m2-004: MSR and interrupt instructions ────────────

    #[test]
    fn encode_wrmsr_round_trips_through_iced_x86() {
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

        assert_eq!(buf.as_slice(), &[0x0F, 0x30]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Wrmsr);
    }

    #[test]
    fn encode_rdmsr_round_trips_through_iced_x86() {
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

        assert_eq!(buf.as_slice(), &[0x0F, 0x32]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdmsr);
    }

    #[test]
    fn encode_int_0x20_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int,
            operands: smallvec::smallvec![Operand::Imm64(0x20)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xCD, 0x20]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int);
    }

    // ── Phase-5 m2-005: control register MOV instruction encoding ────────

    // Write (mov cr_idx, rax) tests via encode_instruction dispatcher
    #[test]
    fn encode_instruction_mov_cr0_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // mov cr0, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr3_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(3)), Operand::Reg(RegId(0))], // mov cr3, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr4_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(4)), Operand::Reg(RegId(0))], // mov cr4, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr8_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(0))], // mov cr8, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr2_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(2)), Operand::Reg(RegId(0))], // mov cr2, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // Read (mov rax, cr_idx) tests via encode_instruction dispatcher
    #[test]
    fn encode_instruction_mov_rax_cr0_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // mov rax, cr0
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr3_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))], // mov rax, cr3
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr4_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(4))], // mov rax, cr4
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr8_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(8))], // mov rax, cr8
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr2_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(2))], // mov rax, cr2
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),

        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // ── Phase-5 m2-007: descriptor-table load (lgdt/lidt) ────────

    #[test]
    fn encode_lgdt_rdi_disp0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
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

        // Expect: 0F 01 17 (3 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x17]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lgdt_rdi_disp8_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
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

        // Expect: 0F 01 57 08 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x57, 0x08]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lgdt_rdi_disp_neg128_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: -128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 57 80 (4 bytes, -128 as u8 = 0x80)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x57, 0x80]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lidt_rdi_disp0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
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

        // Expect: 0F 01 1F (3 bytes, encoding: 0F 01 /3)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x1F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_lidt_rdi_disp16_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 16,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 5F 10 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x5F, 0x10]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_lidt_rdi_disp_neg128_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: -128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 5F 80 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x5F, 0x80]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    // ── Phase-5 m2-008: interrupt-return + system-return instructions ────────

    #[test]
    fn encode_iret_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: CF (1 byte)
        assert_eq!(buf.as_slice(), &[0xCF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Note: in 64-bit decoder, bare CF is decoded as Iretd (32-bit form)
        assert_eq!(instr.mnemonic(), IcedMnem::Iretd);
    }

    #[test]
    fn encode_iretq_round_trips_through_iced_x86() {
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

        // Expect: 48 CF (2 bytes, REX.W prefix)
        assert_eq!(buf.as_slice(), &[0x48, 0xCF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Iretq);
    }

    #[test]
    fn encode_sysret_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sysret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 0F 07 (3 bytes, REX.W prefix + two-byte opcode)
        assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0x07]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Note: in 64-bit decoder, 48 0F 07 is decoded as Sysretq (64-bit form)
        assert_eq!(instr.mnemonic(), IcedMnem::Sysretq);
    }

    #[test]
    fn encode_far_jmp_mem_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
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

        // Expect: 48 FF 2F (3 bytes)
        // 48 = REX.W
        // FF = opcode
        // 2F = ModR/M with mod=00, reg=5, rm=7 (rdi)
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x2F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_mem_rdi_plus_8_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
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

        // Expect: 48 FF 6F 08 (4 bytes)
        // 48 = REX.W
        // FF = opcode
        // 6F = ModR/M with mod=01, reg=5, rm=7 (rdi + disp8)
        // 08 = disp8
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x6F, 0x08]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_mem_rip_relative_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemRipRel { disp: 0x1000 }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 2D 00 10 00 00 (7 bytes)
        // 48 = REX.W
        // FF = opcode
        // 2D = ModR/M with mod=00, reg=5, rm=5 (rip-relative marker)
        // 00 10 00 00 = 0x1000 in little-endian
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x2D, 0x00, 0x10, 0x00, 0x00]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_imm_sym_produces_abs32_reloc_mode64() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x18), // selector
                Operand::SymbolRef {
                    name: "long_mode_entry".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode64,
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: EA 00 00 00 00 18 00 (7 bytes)
        // EA = opcode
        // 00 00 00 00 = placeholder for imm32 offset (relocation target)
        // 18 00 = imm16 selector (0x18 in little-endian)
        assert_eq!(buf.as_slice(), &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "long_mode_entry");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Abs32);
        assert_eq!(output.reloc_sites[0].addend, 0);
    }

    #[test]
    fn encode_far_jmp_imm_sym_with_addend_produces_abs32_reloc() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x20), // selector
                Operand::SymbolRef {
                    name: "kernel_entry".to_string(),
                    addend: 16,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: EA 00 00 00 00 20 00 (7 bytes) — same byte form in Mode32 and Mode64
        assert_eq!(buf.as_slice(), &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "kernel_entry");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Abs32);
        assert_eq!(output.reloc_sites[0].addend, 16); // addend passed through unchanged
    }

    #[test]
    fn encode_far_jmp_imm_sym_reloc_agnostic_to_mode() {
        // Mode32 and Mode64 must emit identical bytes for ljmp selector:symbol
        let inst_mode32 = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x18),
                Operand::SymbolRef {
                    name: "entry".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        
        emission_order: 0,
        };

        let inst_mode64 = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x18),
                Operand::SymbolRef {
                    name: "entry".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode64,
        
        emission_order: 0,
        };

        let mut buf_mode32 = CodeBuffer::new();
        let mut buf_mode64 = CodeBuffer::new();
        let mut stats = EncodeStats::new();

        encode_instruction(&inst_mode32, &mut buf_mode32, &mut stats)
            .expect("Mode32 encoding failed");
        encode_instruction(&inst_mode64, &mut buf_mode64, &mut stats)
            .expect("Mode64 encoding failed");

        // Both must produce the same bytes (EA selector:offset form is mode-agnostic)
        assert_eq!(buf_mode32.as_slice(), buf_mode64.as_slice());
        assert_eq!(
            buf_mode32.as_slice(),
            &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00]
        );
    }

    // ── Phase-5 m5-002: SymbolRef tests ───────────────────────────────

    #[test]
    fn encode_lea_rax_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::SymbolRef {
                    name: "gdt_descriptor".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8D 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 8D = LEA opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "gdt_descriptor");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    #[test]
    fn encode_lgdt_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::SymbolRef {
                name: "gdt_descriptor".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 15 00 00 00 00 (7 bytes)
        // 0F 01 = two-byte opcode
        // 15 = ModR/M with mod=00, reg=2 (/2 for lgdt), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x15, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "gdt_descriptor");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    #[test]
    fn encode_mov_rax_symbol_ref_with_addend_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 8B = mov r64, r/m64 opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site with addend
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 4); // PC32_FIELD_BIAS: IR addend 8 → reloc addend 8 + (-4) = 4
    }

    // ── PA10-006w: mov [rip+sym], r64 store form ─────

    #[test]
    fn encode_mov_mem_sym_rax_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 0,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 89 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 89 = mov r/m64, r64 opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    #[test]
    fn encode_mov_mem_sym_addend_rdi_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 8,
                },
                Operand::Reg(RegId(7)), // rdi
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 89 3D 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 89 = mov r/m64, r64 opcode
        // 3D = ModR/M with mod=00, reg=7 (rdi), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x3D, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site with addend
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 4); // PC32_FIELD_BIAS: IR addend 8 → reloc addend 8 + (-4) = 4
    }

    #[test]
    fn encode_mov_mem_sym_r8_sets_rex_r() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "buf".to_string(),
                    addend: 0,
                },
                Operand::Reg(RegId(8)), // r8
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 4C 89 05 00 00 00 00 (7 bytes)
        // 4C = REX.W | REX.R (for r8 as source register)
        // 89 = mov r/m64, r64 opcode
        // 05 = ModR/M with mod=00, reg=0 (r8 & 7), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "buf");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4);
    }

    #[test]
    fn encode_mov_mem_sym_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "data".to_string(),
                    addend: 0,
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
        assert_eq!(instr.op_kind(0), OpKind::Memory); // destination is memory (rip-relative)
        assert_eq!(instr.op_kind(1), OpKind::Register); // source is register (rax)
    }

    #[test]
    fn encode_call_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Call,
            operands: smallvec::smallvec![Operand::SymbolRef {
                name: "kernel_main_64".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: Some(0),
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: E8 00 00 00 00 (5 bytes)
        // E8 = call rel32 opcode
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0xE8, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site (Phase 7 m1-001: uses Plt32)
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "kernel_main_64");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Plt32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    // Phase 6 m1-002: CR move dispatch tests
    // These tests verify that MOV instructions with CR operands are correctly
    // classified and routed through encode_mov_cr_dispatcher, emitting the correct bytes.

    /// Test: mov cr3, rdi → 0F 22 DF
    /// CR3 = 16 + 3 = 19, RDI = 7
    #[test]
    fn encode_mov_cr3_rdi_emits_0f22df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(19)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xDF]);
    }

    /// Test: mov cr4, rcx → 0F 22 E1
    /// CR4 = 16 + 4 = 20, RCX = 1
    #[test]
    fn encode_mov_cr4_rcx_emits_0f22e1() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(20)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE1]);
    }

    /// Test: mov cr0, rax → 0F 22 C0
    /// CR0 = 16 + 0 = 16, RAX = 0
    #[test]
    fn encode_mov_cr0_rax_via_dispatch_emits_0f22c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(16)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);
    }

    /// Test: mov rdi, cr3 → 0F 20 DF (read from CR3)
    /// RDI = 7, CR3 = 16 + 3 = 19
    #[test]
    fn encode_mov_rdi_cr3_emits_0f20df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(19))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xDF]);
    }

    /// Test: mov rcx, cr4 → 0F 20 E1 (read from CR4)
    /// RCX = 1, CR4 = 16 + 4 = 20
    #[test]
    fn encode_mov_rcx_cr4_emits_0f20e1() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(20))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE1]);
    }

    /// Test: mov cr8, rax → 44 0F 22 C0 (CR8 requires REX.R)
    /// CR8 = 16 + 8 = 24, RAX = 0
    #[test]
    fn encode_mov_cr8_rax_via_dispatch_emits_440f22c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(24)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);
    }

    // Phase 6 m1-003: DR move dispatch tests
    // These tests verify that MOV instructions with DR operands are correctly
    // classified and routed through encode_mov_dr_dispatcher, emitting the correct bytes.
    // DR encoding: dr_idx = RegId - 25 (compact encoding), opcodes 0F 23 (write), 0F 21 (read).

    /// Test: mov dr0, rax → 0F 23 C0
    /// DR0 = 25 + 0 = 25, RAX = 0
    #[test]
    fn encode_mov_dr0_rax_emits_0f23c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(25)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);
    }

    /// Test: mov dr1, rdi → 0F 23 CF
    /// DR1 = 25 + 1 = 26, RDI = 7
    #[test]
    fn encode_mov_dr1_rdi_emits_0f23cf() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(26)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xCF]);
    }

    /// Test: mov dr7, rcx → 0F 23 F9
    /// DR7 = 25 + 7 = 32, RCX = 1
    #[test]
    fn encode_mov_dr7_rcx_emits_0f23f9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(32)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF9]);
    }

    /// Test: mov rax, dr0 → 0F 21 C0 (read from DR0)
    /// RAX = 0, DR0 = 25 + 0 = 25
    #[test]
    fn encode_mov_rax_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(25))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }

    /// Test: mov rdi, dr1 → 0F 21 CF (read from DR1)
    /// RDI = 7, DR1 = 25 + 1 = 26
    #[test]
    fn encode_mov_rdi_dr1_emits_0f21cf() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(26))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xCF]);
    }

    /// Test: mov rcx, dr7 → 0F 21 F9 (read from DR7)
    /// RCX = 1, DR7 = 25 + 7 = 32
    #[test]
    fn encode_mov_rcx_dr7_emits_0f21f9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(32))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF9]);
    }

    /// Test: mov r8, dr0 → 0F 21 C0 (read from DR0 into r8, GPR 8)
    /// R8 = 8, DR0 = 25 + 0 = 25
    #[test]
    fn encode_mov_r8_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(25))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }

    // Phase 15 m5-002: Segment register MOV instructions (opcode 8E /r).
    // Pattern: mov sreg, r16 → 8E /r

    #[test]
    fn encode_mov_ds_ax_emits_8e_d8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Ds),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E D8 (Ds=3, so ModR/M = 0xC0 | (3 << 3) | 0 = 0xD8)
        assert_eq!(buf.as_slice(), &[0x8E, 0xD8]);
    }

    #[test]
    fn encode_mov_es_ax_emits_8e_c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Es),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E C0 (Es=0, so ModR/M = 0xC0 | (0 << 3) | 0 = 0xC0)
        assert_eq!(buf.as_slice(), &[0x8E, 0xC0]);
    }

    #[test]
    fn encode_mov_ss_ax_emits_8e_d0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Ss),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E D0 (Ss=2, so ModR/M = 0xC0 | (2 << 3) | 0 = 0xD0)
        assert_eq!(buf.as_slice(), &[0x8E, 0xD0]);
    }

    #[test]
    fn encode_mov_fs_ax_emits_8e_e0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Fs),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E E0 (Fs=4, so ModR/M = 0xC0 | (4 << 3) | 0 = 0xE0)
        assert_eq!(buf.as_slice(), &[0x8E, 0xE0]);
    }

    #[test]
    fn encode_mov_gs_ax_emits_8e_e8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Gs),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        
        emission_order: 0,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E E8 (Gs=5, so ModR/M = 0xC0 | (5 << 3) | 0 = 0xE8)
        assert_eq!(buf.as_slice(), &[0x8E, 0xE8]);
    }

    #[test]
    fn encode_mov_ds_ax_mode_agnostic() {
        // Verify Mode32 and Mode64 produce identical bytecode.
        let test_mode = |mode: InstrMode| {
            let mut buf = CodeBuffer::new();
            let inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: smallvec::smallvec![
                    Operand::SegReg(SegReg::Ds),
                    Operand::Reg(RegId(0)), // rax
                ],
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
            
            emission_order: 0,
            };

            let mut stats = EncodeStats::new();
            encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
            buf.as_slice().to_vec()
        };

        let bytes_mode32 = test_mode(InstrMode::Mode32);
        let bytes_mode64 = test_mode(InstrMode::Mode64);

        // Both should emit identical bytecode (no mode-dependent encoding for sreg MOV).
        assert_eq!(bytes_mode32, bytes_mode64);
        assert_eq!(bytes_mode32, vec![0x8E, 0xD8]);
    }
