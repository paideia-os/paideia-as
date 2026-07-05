//! Tests for `bswap r32` encoding — PA-R15-001 (issue #956).

use paideia_as_encoder::encode::{bswap_reg32, CodeBuffer, Reg32};

/// Test `bswap eax` → `0F C8`
#[test]
fn bswap_eax_emits_0f_c8() {
    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::Eax);
    assert_eq!(buf.bytes, vec![0x0F, 0xC8]);
}

/// Test `bswap ecx` → `0F C9`
#[test]
fn bswap_ecx_emits_0f_c9() {
    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::Ecx);
    assert_eq!(buf.bytes, vec![0x0F, 0xC9]);
}

/// Test `bswap edi` → `0F CF`
#[test]
fn bswap_edi_emits_0f_cf() {
    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::Edi);
    assert_eq!(buf.bytes, vec![0x0F, 0xCF]);
}

/// Test `bswap r8d` → `41 0F C8`
#[test]
fn bswap_r8d_emits_41_0f_c8() {
    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::R8d);
    assert_eq!(buf.bytes, vec![0x41, 0x0F, 0xC8]);
}

/// Test `bswap r15d` → `41 0F CF`
#[test]
fn bswap_r15d_emits_41_0f_cf() {
    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::R15d);
    assert_eq!(buf.bytes, vec![0x41, 0x0F, 0xCF]);
}

/// Exhaustive 16-register byte-length check: low 8 emit 2 bytes, high 8 emit 3 bytes.
#[test]
fn bswap_all_16_registers_emit_correct_byte_count() {
    let registers_and_counts = [
        (Reg32::Eax, 2),
        (Reg32::Ecx, 2),
        (Reg32::Edx, 2),
        (Reg32::Ebx, 2),
        (Reg32::Esp, 2),
        (Reg32::Ebp, 2),
        (Reg32::Esi, 2),
        (Reg32::Edi, 2),
        (Reg32::R8d, 3),
        (Reg32::R9d, 3),
        (Reg32::R10d, 3),
        (Reg32::R11d, 3),
        (Reg32::R12d, 3),
        (Reg32::R13d, 3),
        (Reg32::R14d, 3),
        (Reg32::R15d, 3),
    ];

    for (reg, expected_len) in &registers_and_counts {
        let mut buf = CodeBuffer::new();
        bswap_reg32(&mut buf, *reg);
        assert_eq!(
            buf.bytes.len(),
            *expected_len,
            "bswap {:?} should emit {} bytes, got {}",
            reg,
            expected_len,
            buf.bytes.len()
        );
    }
}

/// Test iced-x86 round-trip for `bswap eax`.
#[test]
fn bswap_eax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::Eax);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bswap);
    assert_eq!(instr.op_count(), 1);
}

/// Test iced-x86 round-trip for `bswap r8d`.
#[test]
fn bswap_r8d_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::R8d);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bswap);
    assert_eq!(instr.op_count(), 1);
}

/// Test iced-x86 round-trip for `bswap r15d`.
#[test]
fn bswap_r15d_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bswap_reg32(&mut buf, Reg32::R15d);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bswap);
    assert_eq!(instr.op_count(), 1);
}

/// Exhaustive iced-x86 round-trip over all 16 GPRs — confirm decoder
/// sees `bswap <r32>` (never `bswap <r64>`) and that no REX.W is emitted.
#[test]
fn bswap_all_16_registers_round_trip_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register as IcedReg};

    let registers = [
        (Reg32::Eax,  IcedReg::EAX),
        (Reg32::Ecx,  IcedReg::ECX),
        (Reg32::Edx,  IcedReg::EDX),
        (Reg32::Ebx,  IcedReg::EBX),
        (Reg32::Esp,  IcedReg::ESP),
        (Reg32::Ebp,  IcedReg::EBP),
        (Reg32::Esi,  IcedReg::ESI),
        (Reg32::Edi,  IcedReg::EDI),
        (Reg32::R8d,  IcedReg::R8D),
        (Reg32::R9d,  IcedReg::R9D),
        (Reg32::R10d, IcedReg::R10D),
        (Reg32::R11d, IcedReg::R11D),
        (Reg32::R12d, IcedReg::R12D),
        (Reg32::R13d, IcedReg::R13D),
        (Reg32::R14d, IcedReg::R14D),
        (Reg32::R15d, IcedReg::R15D),
    ];

    for (reg, expected_iced) in &registers {
        let mut buf = CodeBuffer::new();
        bswap_reg32(&mut buf, *reg);

        // REX.W (0x48 bit) must NEVER be set — else this decodes as bswap r64.
        if !buf.bytes.is_empty() && (buf.bytes[0] & 0xF0) == 0x40 {
            assert_eq!(buf.bytes[0] & 0x08, 0, "REX.W set for {:?}", reg);
        }

        let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Bswap, "wrong mnemonic for {:?}", reg);
        assert_eq!(instr.op_count(), 1);
        assert_eq!(instr.op_kind(0), OpKind::Register);
        assert_eq!(instr.op_register(0), *expected_iced, "wrong register for {:?}", reg);
    }
}
