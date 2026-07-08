//! Tests for `popcnt r32/r64, r/m32/r/m64` encoding
//! — PA-R15-006 (issue #961).

use paideia_as_encoder::encode::{popcnt_reg64_reg64, popcnt_reg64_mem_base_disp, popcnt_reg32_reg32, popcnt_reg32_mem_base_disp, CodeBuffer, Reg64};

// ============================================================================
// POPCNT Byte-Exact Tests (8 tests)
// ============================================================================

/// Test `popcnt rax, rcx` → `F3 48 0F B8 C1`
#[test]
fn popcnt_rax_rcx_emits_f3_48_0f_b8_c1() {
    let mut buf = CodeBuffer::new();
    popcnt_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xB8, 0xC1]);
}

/// Test `popcnt rcx, rax` → `F3 48 0F B8 C8`
#[test]
fn popcnt_rcx_rax_emits_f3_48_0f_b8_c8() {
    let mut buf = CodeBuffer::new();
    popcnt_reg64_reg64(&mut buf, Reg64::Rcx, Reg64::Rax);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xB8, 0xC8]);
}

/// Test `popcnt rax, [rcx]` → `F3 48 0F B8 01`
#[test]
fn popcnt_rax_mem_rcx_emits_f3_48_0f_b8_01() {
    let mut buf = CodeBuffer::new();
    popcnt_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xB8, 0x01]);
}

/// Test `popcnt r8, r15` → `F3 4D 0F B8 C7`
#[test]
fn popcnt_r8_r15_emits_f3_4d_0f_b8_c7() {
    let mut buf = CodeBuffer::new();
    popcnt_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
    assert_eq!(buf.bytes, vec![0xF3, 0x4D, 0x0F, 0xB8, 0xC7]);
}

/// Test `popcnt rax, [rsp]` → `F3 48 0F B8 04 24` (SIB escape)
#[test]
fn popcnt_rax_mem_rsp_emits_f3_48_0f_b8_04_24() {
    let mut buf = CodeBuffer::new();
    popcnt_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xB8, 0x04, 0x24]);
}

/// Test `popcnt rax, [rbp]` → `F3 48 0F B8 45 00` (forced disp8=0)
#[test]
fn popcnt_rax_mem_rbp_emits_f3_48_0f_b8_45_00() {
    let mut buf = CodeBuffer::new();
    popcnt_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xB8, 0x45, 0x00]);
}

/// Test `popcnt eax, ecx` → `F3 0F B8 C1` (W32, no REX)
#[test]
fn popcnt_eax_ecx_emits_f3_0f_b8_c1() {
    let mut buf = CodeBuffer::new();
    popcnt_reg32_reg32(&mut buf, 0, 1); // eax=0, ecx=1
    assert_eq!(buf.bytes, vec![0xF3, 0x0F, 0xB8, 0xC1]);
}

/// Test `popcnt r8d, ecx` → `F3 44 0F B8 C1` (W32, REX.R only)
#[test]
fn popcnt_r8d_ecx_emits_f3_44_0f_b8_c1() {
    let mut buf = CodeBuffer::new();
    popcnt_reg32_reg32(&mut buf, 8, 1); // r8d=8, ecx=1
    assert_eq!(buf.bytes, vec![0xF3, 0x44, 0x0F, 0xB8, 0xC1]);
}

// ============================================================================
// iced-x86 Round-Trip Tests (2 tests)
// ============================================================================

/// Test iced-x86 round-trip for `popcnt rax, rcx`.
#[test]
fn popcnt_rax_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    popcnt_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Popcnt);
}

/// Test iced-x86 round-trip for `popcnt eax, [rdi]`.
#[test]
fn popcnt_eax_mem_rdi_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    popcnt_reg32_mem_base_disp(&mut buf, 0, 7, 0); // eax=0, rdi=7

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Popcnt);
}
