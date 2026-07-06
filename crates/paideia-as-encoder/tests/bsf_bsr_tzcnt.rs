//! Tests for `bsf`, `bsr`, `tzcnt` r64, r/m64 encoding
//! — PA-R16-008 (issue #974).

use paideia_as_encoder::encode::{bsf_reg64_reg64, bsf_reg64_mem_base_disp, bsr_reg64_reg64, bsr_reg64_mem_base_disp, tzcnt_reg64_reg64, tzcnt_reg64_mem_base_disp, CodeBuffer, Reg64};

// ============================================================================
// BSF Byte-Exact Tests (4 tests)
// ============================================================================

/// Test `bsf rax, rcx` → `48 0F BC C1`
#[test]
fn bsf_rax_rcx_emits_48_0f_bc_c1() {
    let mut buf = CodeBuffer::new();
    bsf_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xBC, 0xC1]);
}

/// Test `bsf r8, r15` → `4D 0F BC C7`
#[test]
fn bsf_r8_r15_emits_4d_0f_bc_c7() {
    let mut buf = CodeBuffer::new();
    bsf_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
    assert_eq!(buf.bytes, vec![0x4D, 0x0F, 0xBC, 0xC7]);
}

/// Test `bsf rax, [rcx]` → `48 0F BC 01`
#[test]
fn bsf_rax_mem_rcx_emits_48_0f_bc_01() {
    let mut buf = CodeBuffer::new();
    bsf_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xBC, 0x01]);
}

/// Test `bsf rax, [rsp]` → `48 0F BC 04 24` (SIB escape)
#[test]
fn bsf_rax_mem_rsp_emits_48_0f_bc_04_24() {
    let mut buf = CodeBuffer::new();
    bsf_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xBC, 0x04, 0x24]);
}

// ============================================================================
// BSR Byte-Exact Tests (4 tests)
// ============================================================================

/// Test `bsr rax, rcx` → `48 0F BD C1`
#[test]
fn bsr_rax_rcx_emits_48_0f_bd_c1() {
    let mut buf = CodeBuffer::new();
    bsr_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xBD, 0xC1]);
}

/// Test `bsr r8, r15` → `4D 0F BD C7`
#[test]
fn bsr_r8_r15_emits_4d_0f_bd_c7() {
    let mut buf = CodeBuffer::new();
    bsr_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
    assert_eq!(buf.bytes, vec![0x4D, 0x0F, 0xBD, 0xC7]);
}

/// Test `bsr rax, [rcx]` → `48 0F BD 01`
#[test]
fn bsr_rax_mem_rcx_emits_48_0f_bd_01() {
    let mut buf = CodeBuffer::new();
    bsr_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xBD, 0x01]);
}

/// Test `bsr rax, [rbp]` → `48 0F BD 45 00` (forced disp8=0)
#[test]
fn bsr_rax_mem_rbp_emits_48_0f_bd_45_00() {
    let mut buf = CodeBuffer::new();
    bsr_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x0F, 0xBD, 0x45, 0x00]);
}

// ============================================================================
// TZCNT Byte-Exact Tests (4 tests)
// ============================================================================

/// Test `tzcnt rax, rcx` → `F3 48 0F BC C1`
#[test]
fn tzcnt_rax_rcx_emits_f3_48_0f_bc_c1() {
    let mut buf = CodeBuffer::new();
    tzcnt_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xBC, 0xC1]);
}

/// Test `tzcnt r8, r15` → `F3 4D 0F BC C7`
#[test]
fn tzcnt_r8_r15_emits_f3_4d_0f_bc_c7() {
    let mut buf = CodeBuffer::new();
    tzcnt_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
    assert_eq!(buf.bytes, vec![0xF3, 0x4D, 0x0F, 0xBC, 0xC7]);
}

/// Test `tzcnt rax, [rcx]` → `F3 48 0F BC 01`
#[test]
fn tzcnt_rax_mem_rcx_emits_f3_48_0f_bc_01() {
    let mut buf = CodeBuffer::new();
    tzcnt_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xBC, 0x01]);
}

/// Test `tzcnt rax, [rsp]` → `F3 48 0F BC 04 24` (SIB escape)
#[test]
fn tzcnt_rax_mem_rsp_emits_f3_48_0f_bc_04_24() {
    let mut buf = CodeBuffer::new();
    tzcnt_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.bytes, vec![0xF3, 0x48, 0x0F, 0xBC, 0x04, 0x24]);
}

// ============================================================================
// iced-x86 Round-Trip Tests (3 tests)
// ============================================================================

/// Test iced-x86 round-trip for `bsf rax, rcx`.
#[test]
fn bsf_rax_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bsf_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bsf);
}

/// Test iced-x86 round-trip for `bsr rax, rcx`.
#[test]
fn bsr_rax_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    bsr_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Bsr);
}

/// Test iced-x86 round-trip for `tzcnt rax, rcx`.
#[test]
fn tzcnt_rax_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    tzcnt_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Tzcnt);
}
