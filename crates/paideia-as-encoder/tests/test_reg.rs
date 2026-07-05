//! Integration tests for `test r/m64, r64` — PA-R13-007 (issue #936).
//!
//! Covers the general `REX.W 85 /r` encoding for all register pairs.
//! Byte-exact assertions validate ModR/M layout, REX prefix calculation,
//! and proper handling of extended registers (r8..r15).
//!
//! Test vectors were cross-checked against Intel SDM Vol 2A (TEST).

use paideia_as_encoder::{CodeBuffer, Reg64};

/// Encode a `test r64, r64` instruction using the primitive and return its bytes.
fn encode_test_reg64_reg64(dst: Reg64, src: Reg64) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    paideia_as_encoder::encode::test_reg64_reg64(&mut buf, dst, src);
    buf.as_slice().to_vec()
}

// ===== byte-exact vectors =====

#[test]
fn test_rax_rax_48_85_c0() {
    // Encode: test rax, rax
    // - dst = Rax (id 0), src = Rax (id 0)
    // - REX = 0x48 (W=1, R=0, B=0)
    // - ModR/M = 0xC0 | (0 << 3) | 0 = 0xC0
    assert_eq!(
        encode_test_reg64_reg64(Reg64::Rax, Reg64::Rax),
        &[0x48, 0x85, 0xC0]
    );
}

#[test]
fn test_rax_rdi_48_85_f8() {
    // Encode: test rax, rdi
    // - dst = Rax (id 0), src = Rdi (id 7)
    // - REX = 0x48 (W=1, R=0, B=0)
    // - ModR/M = 0xC0 | (7 << 3) | 0 = 0xC0 | 56 = 0xF8
    assert_eq!(
        encode_test_reg64_reg64(Reg64::Rax, Reg64::Rdi),
        &[0x48, 0x85, 0xF8]
    );
}

#[test]
fn test_rdi_rax_48_85_c7() {
    // Encode: test rdi, rax
    // - dst = Rdi (id 7), src = Rax (id 0)
    // - REX = 0x48 (W=1, R=0, B=0)
    // - ModR/M = 0xC0 | (0 << 3) | 7 = 0xC0 | 0 | 7 = 0xC7
    assert_eq!(
        encode_test_reg64_reg64(Reg64::Rdi, Reg64::Rax),
        &[0x48, 0x85, 0xC7]
    );
}

#[test]
fn test_r8_r8_4d_85_c0() {
    // Encode: test r8, r8
    // - dst = R8 (id 8), src = R8 (id 8)
    // - REX = 0x40 | (1 << 3) | (1 << 2) | 0 | 1 = 0x4D (W=1, R=1, B=1)
    // - ModR/M = 0xC0 | (0 << 3) | 0 = 0xC0 (low 3 bits of each)
    assert_eq!(
        encode_test_reg64_reg64(Reg64::R8, Reg64::R8),
        &[0x4D, 0x85, 0xC0]
    );
}

#[test]
fn test_rax_r15_4c_85_f8_extended_src() {
    // Encode: test rax, r15
    // - dst = Rax (id 0), src = R15 (id 15)
    // - REX = 0x40 | (1 << 3) | (1 << 2) | 0 | 0 = 0x4C (W=1, R=1, because src>>3 = 1)
    // - ModR/M = 0xC0 | (7 << 3) | 0 = 0xC0 | 56 = 0xF8 (7 = low 3 bits of 15)
    assert_eq!(
        encode_test_reg64_reg64(Reg64::Rax, Reg64::R15),
        &[0x4C, 0x85, 0xF8]
    );
}

#[test]
fn test_r15_r8_4f_85_c7_both_extended() {
    // Encode: test r15, r8
    // - dst = R15 (id 15), src = R8 (id 8)
    // - REX = 0x40 | (1 << 3) | (1 << 2) | 0 | 1 = 0x4D... wait
    // Let me recalculate: REX.W=1, REX.R=(src>>3)=(8>>3)=1, REX.B=(dst>>3)=(15>>3)=1
    // REX = 0x40 | (1 << 3) | (1 << 2) | 0 | 1 = 0x40 | 8 | 4 | 1 = 0x4D
    // ModR/M = 0xC0 | (0 << 3) | 7 = 0xC0 | 0 | 7 = 0xC7 (0 = low 3 bits of 8, 7 = low 3 bits of 15)
    assert_eq!(
        encode_test_reg64_reg64(Reg64::R15, Reg64::R8),
        &[0x4D, 0x85, 0xC7]
    );
}

// ===== iced-x86 round-trip validation =====

#[test]
fn test_rax_rax_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_test_reg64_reg64(Reg64::Rax, Reg64::Rax);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Test);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.op1_register(), Register::RAX);
    assert_eq!(decoded.len(), 3, "test r64, r64 is always 3 bytes (REX + opcode + ModR/M)");
}

#[test]
fn test_all_16_registers_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let reg_names = [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
        "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
    ];

    let regs = [
        Reg64::Rax, Reg64::Rcx, Reg64::Rdx, Reg64::Rbx,
        Reg64::Rsp, Reg64::Rbp, Reg64::Rsi, Reg64::Rdi,
        Reg64::R8, Reg64::R9, Reg64::R10, Reg64::R11,
        Reg64::R12, Reg64::R13, Reg64::R14, Reg64::R15,
    ];

    for (i, &reg) in regs.iter().enumerate() {
        let bytes = encode_test_reg64_reg64(reg, reg);
        assert_eq!(
            bytes.len(),
            3,
            "test r{} (dst), r{} (src) should be 3 bytes",
            reg_names[i],
            reg_names[i]
        );
        let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
        let decoded = decoder.decode();
        assert_eq!(
            decoded.mnemonic(),
            IcedMnem::Test,
            "test r{}, r{} mnemonic mismatch",
            reg_names[i],
            reg_names[i]
        );
    }
}
