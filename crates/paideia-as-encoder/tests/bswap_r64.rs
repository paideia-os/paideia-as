//! Tests for `bswap r64` encoding — PA-R13-014 (issue #943).

use paideia_as_encoder::encode::{bswap_reg64, CodeBuffer, Reg64};

/// Test `bswap rax` → `48 0F C8`
#[test]
fn bswap_rax_emits_48_0f_c8() {
    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::Rax);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xC8]);
}

/// Test `bswap rcx` → `48 0F C9`
#[test]
fn bswap_rcx_emits_48_0f_c9() {
    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xC9]);
}

/// Test `bswap rdi` → `48 0F CF`
#[test]
fn bswap_rdi_emits_48_0f_cf() {
    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::Rdi);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xCF]);
}

/// Test `bswap r8` → `49 0F C8`
#[test]
fn bswap_r8_emits_49_0f_c8() {
    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::R8);
    assert_eq!(buf.bytes, vec![0x49, 0x0F, 0xC8]);
}

/// Test `bswap r15` → `49 0F CF`
#[test]
fn bswap_r15_emits_49_0f_cf() {
    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::R15);
    assert_eq!(buf.bytes, vec![0x49, 0x0F, 0xCF]);
}

/// Exhaustive 16-register byte-length check: all emit exactly 3 bytes.
#[test]
fn bswap_all_16_registers_emit_3_bytes() {
    let registers = [
        Reg64::Rax,  // 0
        Reg64::Rcx,  // 1
        Reg64::Rdx,  // 2
        Reg64::Rbx,  // 3
        Reg64::Rsp,  // 4
        Reg64::Rbp,  // 5
        Reg64::Rsi,  // 6
        Reg64::Rdi,  // 7
        Reg64::R8,   // 8
        Reg64::R9,   // 9
        Reg64::R10,  // 10
        Reg64::R11,  // 11
        Reg64::R12,  // 12
        Reg64::R13,  // 13
        Reg64::R14,  // 14
        Reg64::R15,  // 15
    ];

    for reg in &registers {
        let mut buf = CodeBuffer::new();
        bswap_reg64(&mut buf, *reg);
        assert_eq!(
            buf.bytes.len(),
            3,
            "bswap {:?} should emit 3 bytes, got {}",
            reg,
            buf.bytes.len()
        );
    }
}

/// Test iced-x86 round-trip for `bswap rax`.
#[test]
fn bswap_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::Rax);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bswap);
    assert_eq!(instr.op_count(), 1);
}

/// Test iced-x86 round-trip for `bswap r8`.
#[test]
fn bswap_r8_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::R8);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bswap);
    assert_eq!(instr.op_count(), 1);
}

/// Test iced-x86 round-trip for `bswap r15`.
#[test]
fn bswap_r15_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bswap_reg64(&mut buf, Reg64::R15);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bswap);
    assert_eq!(instr.op_count(), 1);
}
