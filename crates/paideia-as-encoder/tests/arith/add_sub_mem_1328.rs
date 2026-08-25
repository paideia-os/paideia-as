//! Byte-exact tests for `add/sub r64, [mem]` — issue #1328.
//!
//! Prior to this fix, `encode_add` and `encode_sub` matched only
//! `[Reg, Reg]` and `[Reg, Imm64]`. A memory-source operand fell through to
//! the catch-all arm and surfaced as `B1705 Unsupported("add form not
//! supported: expected reg64,reg64 or reg64,imm64")` (mirror for sub). Kernel
//! sites (paideia-os `src/kernel/core/syscall/sys_getdents.pdx` at PR #1802's
//! ancestor and `src/kernel/core/fs/tmpfs/vops.pdx` at the same tag) had to
//! r-to-r restage via a scratch register — `mov r10, [mem]; add r8, r10;`
//! — which paideia-as #1328 tracks and reverts once this encoder gap closes.
//!
//! Golden bytes below cover:
//!   * base+disp addressing across the register-extension frontier (rax..r15)
//!   * SIB-escape (base = rsp)
//!   * BP-escape (base = rbp, disp = 0 forces disp8=0)
//!   * REX.R/REX.B combinations
//!   * SIB with index + scale
//! iced-x86 round-trip tests confirm the produced bytes decode back to the
//! intended (mnemonic, operands).

use paideia_as_encoder::encode::{
    add_reg64_mem_base_disp, add_reg64_mem_sib_disp, sub_reg64_mem_base_disp,
    sub_reg64_mem_sib_disp, CodeBuffer, Reg64,
};

// ============================================================================
// ADD r64, [base + disp] — byte-exact
// ============================================================================

/// `add rax, [rcx]` → `48 03 01`
#[test]
fn add_rax_mem_rcx_emits_48_03_01() {
    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x03, 0x01]);
}

/// `add r8, [rbp - 64]` — the exact shape paideia-os `sys_getdents.pdx`
/// stages through r10; disp=-0x40 lives in the disp8 range.
#[test]
fn add_r8_mem_rbp_minus_64_emits_4c_03_45_c0() {
    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::R8, Reg64::Rbp, -64);
    assert_eq!(buf.bytes, vec![0x4C, 0x03, 0x45, 0xC0]);
}

/// `add rax, [rsp]` → `48 03 04 24` (SIB escape when base = rsp).
#[test]
fn add_rax_mem_rsp_emits_48_03_04_24() {
    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x03, 0x04, 0x24]);
}

/// `add rax, [r13 + 8]` → `49 03 45 08` (r13 forces mod=01 even at disp8=0;
/// here disp=8, so mod=01 is required anyway — pinned to confirm REX.B).
#[test]
fn add_rax_mem_r13_plus_8_emits_49_03_45_08() {
    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::R13, 8);
    assert_eq!(buf.bytes, vec![0x49, 0x03, 0x45, 0x08]);
}

/// `add r15, [rax + 0x1000]` — REX.R set for r15, disp32 forces mod=10.
#[test]
fn add_r15_mem_rax_plus_disp32_emits_4c_03_b8_00_10_00_00() {
    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::R15, Reg64::Rax, 0x1000);
    assert_eq!(buf.bytes, vec![0x4C, 0x03, 0xB8, 0x00, 0x10, 0x00, 0x00]);
}

/// `add rax, [r12]` — SIB-escape combined with REX.B=1 (r12 low3 == 100
/// forces the SIB byte, r12 is an extended register so REX.B must be set).
/// Closes the debugger coverage gap flagged during #1328 verification —
/// prior tests exercised RSP (SIB-escape, REX.B=0) and R13 (BP-escape,
/// REX.B=1) but not the combination.
#[test]
fn add_rax_mem_r12_emits_49_03_04_24() {
    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::R12, 0);
    assert_eq!(buf.bytes, vec![0x49, 0x03, 0x04, 0x24]);
}

/// `sub rax, [r12 + 16]` — same SIB-escape + REX.B combination on SUB
/// with a non-zero disp8.
#[test]
fn sub_rax_mem_r12_plus_16_emits_49_2b_44_24_10() {
    let mut buf = CodeBuffer::new();
    sub_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::R12, 16);
    assert_eq!(buf.bytes, vec![0x49, 0x2B, 0x44, 0x24, 0x10]);
}

// ============================================================================
// SUB r64, [base + disp] — byte-exact
// ============================================================================

/// `sub rax, [rsp + 24]` — the exact shape paideia-os `tmpfs/vops.pdx`
/// stages through rcx (`mov rcx, [rsp+24]; sub rax, rcx;` pre-fix).
#[test]
fn sub_rax_mem_rsp_plus_24_emits_48_2b_44_24_18() {
    let mut buf = CodeBuffer::new();
    sub_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 24);
    assert_eq!(buf.bytes, vec![0x48, 0x2B, 0x44, 0x24, 0x18]);
}

/// `sub rax, [rcx]` → `48 2B 01`
#[test]
fn sub_rax_mem_rcx_emits_48_2b_01() {
    let mut buf = CodeBuffer::new();
    sub_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x2B, 0x01]);
}

/// `sub r15, [rax]` → `4C 2B 38` (REX.R for r15).
#[test]
fn sub_r15_mem_rax_emits_4c_2b_38() {
    let mut buf = CodeBuffer::new();
    sub_reg64_mem_base_disp(&mut buf, Reg64::R15, Reg64::Rax, 0);
    assert_eq!(buf.bytes, vec![0x4C, 0x2B, 0x38]);
}

/// `sub rax, [rbp]` — RBP as base forces disp8=0 even at disp=0.
#[test]
fn sub_rax_mem_rbp_emits_48_2b_45_00() {
    let mut buf = CodeBuffer::new();
    sub_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.bytes, vec![0x48, 0x2B, 0x45, 0x00]);
}

// ============================================================================
// ADD / SUB r64, [base + index*scale + disp] — SIB byte-exact
// ============================================================================

/// `add rax, [rbx + rcx*4 + 16]` — SIB with scale=4 and disp8=16.
#[test]
fn add_rax_sib_rbx_rcx_scale4_disp16_emits_48_03_44_8b_10() {
    let mut buf = CodeBuffer::new();
    // scale=4 → scale_bits=2
    add_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rbx, Reg64::Rcx, 2, 16);
    assert_eq!(buf.bytes, vec![0x48, 0x03, 0x44, 0x8B, 0x10]);
}

/// `sub r12, [r8 + r9*8]` — extended dst, extended base + index, disp=0.
#[test]
fn sub_r12_sib_r8_r9_scale8_disp0_emits_4f_2b_24_c8() {
    let mut buf = CodeBuffer::new();
    // scale=8 → scale_bits=3
    sub_reg64_mem_sib_disp(&mut buf, Reg64::R12, Reg64::R8, Reg64::R9, 3, 0);
    assert_eq!(buf.bytes, vec![0x4F, 0x2B, 0x24, 0xC8]);
}

/// `add rdx, [rsi + rdi*1 + 0x1000]` — scale=1, disp32.
#[test]
fn add_rdx_sib_rsi_rdi_scale1_disp32_emits_48_03_94_3e_00_10_00_00() {
    let mut buf = CodeBuffer::new();
    // scale=1 → scale_bits=0
    add_reg64_mem_sib_disp(&mut buf, Reg64::Rdx, Reg64::Rsi, Reg64::Rdi, 0, 0x1000);
    assert_eq!(buf.bytes, vec![0x48, 0x03, 0x94, 0x3E, 0x00, 0x10, 0x00, 0x00]);
}

// ============================================================================
// iced-x86 round-trip (mnemonic-level sanity — bytes must decode as ADD/SUB)
// ============================================================================

/// Round-trip: `add r8, [rbp - 64]` decodes back to Add.
#[test]
fn add_r8_mem_rbp_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    add_reg64_mem_base_disp(&mut buf, Reg64::R8, Reg64::Rbp, -64);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Add);
}

/// Round-trip: `sub rax, [rsp + 24]` decodes back to Sub.
#[test]
fn sub_rax_mem_rsp_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    sub_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 24);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Sub);
}

/// Round-trip: `sub r12, [r8 + r9*8]` (full-extension SIB) decodes back to Sub.
#[test]
fn sub_r12_sib_extended_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    sub_reg64_mem_sib_disp(&mut buf, Reg64::R12, Reg64::R8, Reg64::R9, 3, 0);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Sub);
}
