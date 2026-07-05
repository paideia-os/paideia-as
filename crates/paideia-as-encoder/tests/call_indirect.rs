//! Tests for PA-R13-003: indirect call instructions (call reg / call [mem]).
//!
//! This test suite verifies that call forms emit the correct x86_64 bytecode:
//! - call reg64: FF /2 (register form)
//! - call [base + disp]: FF 14 [base + disp] (memory form)
//! - call [base + index*scale + disp]: FF 14 SIB [disp] (indexed memory form)
//! - call [rip + disp32]: FF 15 <disp32> (RIP-relative form)

use paideia_as_encoder::{CodeBuffer, Reg64};
use paideia_as_encoder::encode::{call_reg64, call_mem_base_disp, call_mem_sib_disp, call_mem_rip_rel};

// ── 7 byte-exact test cases ────────────────────────────────────────────

#[test]
fn pa_r13_003_call_rax() {
    // call rax → FF D0
    // Expected: 0xFF (opcode), 0xD0 (ModR/M: mod=11, reg=010, rm=000)
    let mut buf = CodeBuffer::new();
    call_reg64(&mut buf, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0xFF, 0xD0]);
}

#[test]
fn pa_r13_003_call_r8() {
    // call r8 → 41 FF D0
    // Expected: 0x41 (REX.B), 0xFF (opcode), 0xD0 (ModR/M: mod=11, reg=010, rm=000 for r8)
    let mut buf = CodeBuffer::new();
    call_reg64(&mut buf, Reg64::R8);
    assert_eq!(buf.as_slice(), &[0x41, 0xFF, 0xD0]);
}

#[test]
fn pa_r13_003_call_mem_rax() {
    // call [rax] → FF 10
    // Expected: 0xFF (opcode), 0x10 (ModR/M: mod=00, reg=010, rm=000)
    let mut buf = CodeBuffer::new();
    call_mem_base_disp(&mut buf, Reg64::Rax, 0);
    assert_eq!(buf.as_slice(), &[0xFF, 0x10]);
}

#[test]
fn pa_r13_003_call_mem_rdi_8() {
    // call [rdi + 8] → FF 57 08
    // Expected: 0xFF (opcode), 0x57 (ModR/M: mod=01, reg=010, rm=111), 0x08 (disp8)
    let mut buf = CodeBuffer::new();
    call_mem_base_disp(&mut buf, Reg64::Rdi, 8);
    assert_eq!(buf.as_slice(), &[0xFF, 0x57, 0x08]);
}

#[test]
fn pa_r13_003_call_mem_r12_rsi_8() {
    // call [r12 + rsi*8] → 41 FF 14 F4
    // Expected: 0x41 (REX.B for r12), 0xFF (opcode), 0x14 (ModR/M: mod=00, reg=010, rm=100 for SIB),
    // 0xF4 (SIB: scale=11 (×8), index=110 (rsi), base=100 (r12))
    let mut buf = CodeBuffer::new();
    call_mem_sib_disp(&mut buf, Reg64::R12, Reg64::Rsi, 3, 0);
    assert_eq!(buf.as_slice(), &[0x41, 0xFF, 0x14, 0xF4]);
}

#[test]
fn pa_r13_003_call_mem_r13_rsi_8() {
    // call [r13 + rsi*8] → 41 FF 54 F5 00
    // R13 is RBP with B=1 (r13 = rbp + 8, so bit 3 set = B escape for SIB base)
    // Expected: 0x41 (REX.B for r13), 0xFF (opcode), 0x54 (ModR/M: mod=01, reg=010, rm=100 for SIB),
    // 0xF5 (SIB: scale=11 (×8), index=110 (rsi), base=101 (r13)), 0x00 (disp8 = 0, but required due to RBP escape)
    let mut buf = CodeBuffer::new();
    call_mem_sib_disp(&mut buf, Reg64::R13, Reg64::Rsi, 3, 0);
    assert_eq!(buf.as_slice(), &[0x41, 0xFF, 0x54, 0xF5, 0x00]);
}

#[test]
fn pa_r13_003_call_mem_rip_rel() {
    // call [rip + sym] → FF 15 00 00 00 00 (+ relocation)
    // Expected: 0xFF (opcode), 0x15 (ModR/M: mod=00, reg=010, rm=101 for RIP-relative),
    // 0x00 0x00 0x00 0x00 (disp32 placeholder)
    let mut buf = CodeBuffer::new();
    call_mem_rip_rel(&mut buf, 0);
    assert_eq!(buf.as_slice(), &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);
}

// ── 3 iced-x86 round-trip verification tests ────────────────────────────

#[test]
fn pa_r13_003_roundtrip_call_rax() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    call_reg64(&mut buf, Reg64::Rax);
    let bytes = buf.as_slice();

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode call rax");

    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert_eq!(instr.op_count(), 1);
}

#[test]
fn pa_r13_003_roundtrip_call_mem_rdi_8() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    call_mem_base_disp(&mut buf, Reg64::Rdi, 8);
    let bytes = buf.as_slice();

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode call [rdi + 8]");

    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert_eq!(instr.op_count(), 1);
}

#[test]
fn pa_r13_003_roundtrip_call_mem_r12_rsi_8() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    call_mem_sib_disp(&mut buf, Reg64::R12, Reg64::Rsi, 3, 0);
    let bytes = buf.as_slice();

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode call [r12 + rsi*8]");

    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert_eq!(instr.op_count(), 1);
}
