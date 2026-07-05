//! Tests for `adc r32/r64, r/m32/r/m64` and `sbb r32/r64, r/m32/r/m64` encoding
//! — PA-R15-005 (issue #960).

use paideia_as_encoder::encode::{adc_reg64_reg64, adc_reg64_mem_base_disp, sbb_reg64_reg64, sbb_reg64_mem_base_disp, adc_reg32_reg32, adc_reg32_mem_base_disp, sbb_reg32_reg32, CodeBuffer, Reg64};

// ============================================================================
// ADC and SBB Byte-Exact Tests (12 tests)
// ============================================================================

/// Test `adc rax, rcx` → `48 13 C1`
#[test]
fn adc_rax_rcx_emits_48_13_c1() {
    let mut buf = CodeBuffer::new();
    adc_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0x48, 0x13, 0xC1]);
}

/// Test `adc rcx, rax` → `48 13 C8`
#[test]
fn adc_rcx_rax_emits_48_13_c8() {
    let mut buf = CodeBuffer::new();
    adc_reg64_reg64(&mut buf, Reg64::Rcx, Reg64::Rax);
    assert_eq!(buf.bytes, vec![0x48, 0x13, 0xC8]);
}

/// Test `adc rax, [rcx]` → `48 13 01`
#[test]
fn adc_rax_mem_rcx_emits_48_13_01() {
    let mut buf = CodeBuffer::new();
    adc_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x13, 0x01]);
}

/// Test `adc r8, r15` → `4D 13 C7`
#[test]
fn adc_r8_r15_emits_4d_13_c7() {
    let mut buf = CodeBuffer::new();
    adc_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
    assert_eq!(buf.bytes, vec![0x4D, 0x13, 0xC7]);
}

/// Test `adc rax, [r13+8]` → `49 13 45 08` (r13 forces mod=01)
#[test]
fn adc_rax_mem_r13_plus8_emits_49_13_45_08() {
    let mut buf = CodeBuffer::new();
    adc_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::R13, 8);
    assert_eq!(buf.bytes, vec![0x49, 0x13, 0x45, 0x08]);
}

/// Test `adc rax, [rsp]` → `48 13 04 24` (SIB escape)
#[test]
fn adc_rax_mem_rsp_emits_48_13_04_24() {
    let mut buf = CodeBuffer::new();
    adc_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x13, 0x04, 0x24]);
}

/// Test `sbb rax, rcx` → `48 1B C1`
#[test]
fn sbb_rax_rcx_emits_48_1b_c1() {
    let mut buf = CodeBuffer::new();
    sbb_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0x48, 0x1B, 0xC1]);
}

/// Test `sbb rax, [rcx]` → `48 1B 01`
#[test]
fn sbb_rax_mem_rcx_emits_48_1b_01() {
    let mut buf = CodeBuffer::new();
    sbb_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x1B, 0x01]);
}

/// Test `sbb r15, [rax]` → `4C 1B 38`
#[test]
fn sbb_r15_mem_rax_emits_4c_1b_38() {
    let mut buf = CodeBuffer::new();
    sbb_reg64_mem_base_disp(&mut buf, Reg64::R15, Reg64::Rax, 0);
    assert_eq!(buf.bytes, vec![0x4C, 0x1B, 0x38]);
}

/// Test `adc eax, ecx` → `13 C1` (W32, no REX)
#[test]
fn adc_eax_ecx_emits_13_c1() {
    let mut buf = CodeBuffer::new();
    adc_reg32_reg32(&mut buf, 0, 1); // eax=0, ecx=1
    assert_eq!(buf.bytes, vec![0x13, 0xC1]);
}

/// Test `sbb r8d, ecx` → `44 1B C1` (W32, REX.R only)
#[test]
fn sbb_r8d_ecx_emits_44_1b_c1() {
    let mut buf = CodeBuffer::new();
    sbb_reg32_reg32(&mut buf, 8, 1); // r8d=8, ecx=1
    assert_eq!(buf.bytes, vec![0x44, 0x1B, 0xC1]);
}

/// Test `adc eax, [rax]` → `13 00` (W32, no REX)
#[test]
fn adc_eax_mem_rax_emits_13_00() {
    let mut buf = CodeBuffer::new();
    adc_reg32_mem_base_disp(&mut buf, 0, 0, 0); // eax=0, base=rax
    assert_eq!(buf.bytes, vec![0x13, 0x00]);
}

// ============================================================================
// iced-x86 Round-Trip Tests (3 tests)
// ============================================================================

/// Test iced-x86 round-trip for `adc rax, rcx`.
#[test]
fn adc_rax_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    adc_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Adc);
}

/// Test iced-x86 round-trip for `sbb rcx, [rdi]`.
#[test]
fn sbb_rcx_mem_rdi_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    sbb_reg64_mem_base_disp(&mut buf, Reg64::Rcx, Reg64::Rdi, 0);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Sbb);
}

/// Test iced-x86 round-trip for `adc r15, r14`.
#[test]
fn adc_r15_r14_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    adc_reg64_reg64(&mut buf, Reg64::R15, Reg64::R14);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Adc);
}
