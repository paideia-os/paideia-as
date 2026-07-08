//! Prefetch instruction family tests (PA-R14-006, issue #949).
//!
//! Byte-exact verification via iced-x86 round-trip:
//! - prefetchnta, prefetcht0, prefetcht1, prefetcht2
//! - All use `0F 18 /N [base + disp]` pattern with reg_field N ∈ {0,1,2,3}
//! - REX.B for extended base registers (r8..r15)
//! - No REX.W (no operand-size prefix)

use paideia_as_encoder::{
    CodeBuffer,
    encode_instruction, EncodeStats,
};
use paideia_as_ir::{
    Instruction, Mnemonic, Operand, RegId, Scale,
};

#[test]
fn prefetchnta_base_only() {
    // prefetchnta [rdi] → 0F 18 07 (ModR/M: mod=00 reg=000 rm=111)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetchnta,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x0F, 0x18, 0x07]);
}

#[test]
fn prefetcht0_base_only() {
    // prefetcht0 [rdi] → 0F 18 0F (ModR/M: mod=00 reg=001 rm=111)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht0,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x0F, 0x18, 0x0F]);
}

#[test]
fn prefetcht1_base_only() {
    // prefetcht1 [rdi] → 0F 18 17 (ModR/M: mod=00 reg=010 rm=111)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht1,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x0F, 0x18, 0x17]);
}

#[test]
fn prefetcht2_base_only() {
    // prefetcht2 [rdi] → 0F 18 1F (ModR/M: mod=00 reg=011 rm=111)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht2,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x0F, 0x18, 0x1F]);
}

#[test]
fn prefetchnta_base_plus_disp8() {
    // prefetchnta [rdi + 8] → 0F 18 47 08 (ModR/M: mod=01 reg=000 rm=111, disp8)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetchnta,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x0F, 0x18, 0x47, 0x08]);
}

#[test]
fn prefetcht0_base_plus_disp32() {
    // prefetcht0 [rsi + 256] → 0F 18 8E 00 01 00 00 (ModR/M: mod=10 reg=001 rm=110, disp32)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht0,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(6),  // rsi
                index: None,
                scale: Scale::X1,
                disp: 256,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    let expected: Vec<u8> = vec![0x0F, 0x18, 0x8E, 0x00, 0x01, 0x00, 0x00];
    assert_eq!(buf.bytes, expected);
}

#[test]
fn prefetchnta_rex_b_extended_base() {
    // prefetchnta [r12] → 41 0F 18 04 24 (REX.B + two-byte opcode + ModR/M + SIB)
    // r12 = reg_id 12 (> 7), so REX.B set (0x41 = 0x40 | 0x01)
    // ModR/M: mod=00 reg=000 rm=100 (SIB escape)
    // SIB: scale=00 index=100 base=100 (r12 in low 3 bits, represented as 4)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetchnta,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(12),  // r12
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x41, 0x0F, 0x18, 0x04, 0x24]);
}

#[test]
fn prefetcht1_rex_b_extended_base_with_disp() {
    // prefetcht1 [r12 + 8] → 41 0F 18 54 24 08
    // REX.B (0x41) + two-byte opcode + ModR/M (mod=01 reg=010 rm=100) + SIB + disp8
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht1,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(12),  // r12
                index: None,
                scale: Scale::X1,
                disp: 8,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x41, 0x0F, 0x18, 0x54, 0x24, 0x08]);
}

#[test]
fn prefetcht2_rex_b_extended_base_r15() {
    // prefetcht2 [r15] → 41 0F 18 07 (REX.B + opcode + ModR/M)
    // r15 = reg_id 15 (> 7), so REX.B set
    // ModR/M: mod=00 reg=011 rm=111 (r15 in low 3 bits = 7)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht2,
        operands: {
            let mut ops = smallvec::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(15),  // r15
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.bytes, vec![0x41, 0x0F, 0x18, 0x1F]);
}

#[test]
fn all_four_prefetch_levels_encode_different() {
    // Verify that all four levels encode to distinct reg fields
    let test_cases = vec![
        (Mnemonic::Prefetchnta, 0x07),
        (Mnemonic::Prefetcht0, 0x0F),
        (Mnemonic::Prefetcht1, 0x17),
        (Mnemonic::Prefetcht2, 0x1F),
    ];

    for (mnem, expected_modrm) in test_cases {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: mnem,
            operands: {
                let mut ops = smallvec::SmallVec::new();
                ops.push(Operand::MemSib {
                    base: RegId(7),  // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                });
                ops
            },
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: paideia_as_ir::InstrMode::Mode64,
        };

        let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
        assert_eq!(buf.bytes.len(), 3, "Expected 3 bytes for [rdi]");
        assert_eq!(buf.bytes[2], expected_modrm, "ModR/M mismatch for {:?}", mnem);
    }
}

// ===== iced-x86 round-trip verification =====

#[test]
fn prefetchnta_base_rdi_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetchnta,
        operands: {
            let mut ops = paideia_as_ir::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Prefetchnta);
    assert_eq!(decoded.len(), 3);
}

#[test]
fn prefetcht0_base_plus_disp_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Prefetcht0,
        operands: {
            let mut ops = paideia_as_ir::SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),  // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_ir::InstrMode::Mode64,
    };

    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Prefetcht0);
    assert_eq!(decoded.len(), 4);
}
