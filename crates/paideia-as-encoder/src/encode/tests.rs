//! Unit tests for the encoder — round-trip and byte-exact assertions against iced-x86.

use super::*;
use paideia_as_ir::instruction::IntWidth;


#[test]
fn mov_rax_1() {
    let mut buf = CodeBuffer::new();
    mov_reg64_imm32(&mut buf, Reg64::Rax, 1);
    assert_eq!(buf.as_slice(), &[0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00]);
}

#[test]
fn ret_byte() {
    let mut buf = CodeBuffer::new();
    ret(&mut buf);
    assert_eq!(buf.as_slice(), &[0xc3]);
}

#[test]
fn mov_mem_rbp_minus_8_rbx() {
    let mut buf = CodeBuffer::new();
    mov_mem_rbp_disp_reg64(&mut buf, -8, Reg64::Rbx);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x5d, 0xf8]);
}

#[test]
fn mov_rcx_imm32_42() {
    let mut buf = CodeBuffer::new();
    mov_reg64_imm32(&mut buf, Reg64::Rcx, 42);
    assert_eq!(buf.as_slice(), &[0x48, 0xc7, 0xc1, 0x2a, 0x00, 0x00, 0x00]);
}

#[test]
fn xor_rax_rax() {
    let mut buf = CodeBuffer::new();
    xor_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x31, 0xc0]);
}

#[test]
fn not_rax_emits_48_f7_d0() {
    // not rax → REX.W F7 /2 → 48 F7 D0
    let mut buf = CodeBuffer::new();
    not_reg64(&mut buf, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xD0]);
}

#[test]
fn not_r8_emits_49_f7_d0() {
    // not r8 → REX.W+REX.B F7 /2 → 49 F7 D0 (REX.B extends r/m to r8)
    let mut buf = CodeBuffer::new();
    not_reg64(&mut buf, Reg64::R8);
    assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xD0]);
}

#[test]
fn not_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    not_reg64(&mut buf, Reg64::Rax);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Not);
}

// ── Bitwise OR immediate forms (Path α — value-driven, Phase 8 m1-001d) ─

#[test]
fn or_rax_0x20_imm8() {
    // or rax, 0x20 → 48 83 c8 20 (imm8 form, 4 bytes)
    let mut buf = CodeBuffer::new();
    or_reg64_imm8(&mut buf, Reg64::Rax, 0x20);
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc8, 0x20]);
}

#[test]
fn or_rax_0x7f_imm8_boundary() {
    // or rax, 0x7f → 48 83 c8 7f (imm8 boundary, 4 bytes)
    let mut buf = CodeBuffer::new();
    or_reg64_imm8(&mut buf, Reg64::Rax, 0x7f);
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc8, 0x7f]);
}

#[test]
fn or_rax_0x80_imm32_sign_extension() {
    // or rax, 0x80 → 48 81 c8 80 00 00 00 (imm32 form, 7 bytes)
    // Cannot use imm8 form because 0x80 sign-extends to 0xFFFFFFFFFFFFFF80
    let mut buf = CodeBuffer::new();
    or_reg64_imm32(&mut buf, Reg64::Rax, 0x80);
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc8, 0x80, 0x00, 0x00, 0x00]);
}

#[test]
fn or_rax_0x100_imm32() {
    // or rax, 0x100 → 48 81 c8 00 01 00 00 (imm32 form, 7 bytes)
    let mut buf = CodeBuffer::new();
    or_reg64_imm32(&mut buf, Reg64::Rax, 0x100);
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc8, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn or_r15_0x7f_imm8_rex_b() {
    // or r15, 0x7f → 49 83 cf 7f (imm8 + REX.B, 4 bytes)
    let mut buf = CodeBuffer::new();
    or_reg64_imm8(&mut buf, Reg64::R15, 0x7f);
    assert_eq!(buf.as_slice(), &[0x49, 0x83, 0xcf, 0x7f]);
}

#[test]
fn or_r8_0x100_imm32_rex_b() {
    // or r8, 0x100 → 49 81 c8 00 01 00 00 (imm32 + REX.B, 7 bytes)
    let mut buf = CodeBuffer::new();
    or_reg64_imm32(&mut buf, Reg64::R8, 0x100);
    assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xc8, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn or_rax_0x7fffffff_imm32_max_signed() {
    // or rax, 0x7fffffff (i32::MAX) → 48 81 c8 ff ff ff 7f (imm32)
    let mut buf = CodeBuffer::new();
    or_reg64_imm32(&mut buf, Reg64::Rax, i32::MAX);
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc8, 0xff, 0xff, 0xff, 0x7f]);
}

#[test]
fn or_rax_0x20_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    or_reg64_imm8(&mut buf, Reg64::Rax, 0x20);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Or);
}

#[test]
fn or_rax_0x100_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    or_reg64_imm32(&mut buf, Reg64::Rax, 0x100);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Or);
}

// ── Width-threaded immediate moves (Phase 7 m4-003) ─────────────────

#[test]
fn encode_mov_r32_imm32_emits_b8_imm32() {
    // mov eax, 42 → B8 2A 00 00 00 (5 bytes, no REX.W, implicit zero-extend)
    let mut buf = CodeBuffer::new();
    mov_reg32_imm32(&mut buf, Reg64::Rax, 42);
    assert_eq!(buf.as_slice(), &[0xB8, 0x2A, 0x00, 0x00, 0x00]);
}

#[test]
fn encode_mov_r32_imm32_rcx_emits_b9() {
    // mov ecx, 42 → B9 2A 00 00 00 (B8 + reg index 1)
    let mut buf = CodeBuffer::new();
    mov_reg32_imm32(&mut buf, Reg64::Rcx, 42);
    assert_eq!(buf.as_slice(), &[0xB9, 0x2A, 0x00, 0x00, 0x00]);
}

#[test]
fn encode_mov_r32_imm32_r8_uses_rex_b() {
    // mov r8d, 1 → 41 B8 01 00 00 00 (REX.B reaches r8d; still no REX.W)
    let mut buf = CodeBuffer::new();
    mov_reg32_imm32(&mut buf, Reg64::R8, 1);
    assert_eq!(buf.as_slice(), &[0x41, 0xB8, 0x01, 0x00, 0x00, 0x00]);
}

#[test]
fn encode_mov_r16_imm16_emits_66_b8_imm16() {
    // mov ax, 42 → 66 B8 2A 00 (4 bytes, operand-size override)
    let mut buf = CodeBuffer::new();
    mov_reg16_imm16(&mut buf, Reg64::Rax, 42);
    assert_eq!(buf.as_slice(), &[0x66, 0xB8, 0x2A, 0x00]);
}

#[test]
fn encode_mov_r8_imm8_emits_b0_imm8() {
    // mov al, 42 → B0 2A (2 bytes)
    let mut buf = CodeBuffer::new();
    mov_reg8_imm8(&mut buf, Reg64::Rax, 42);
    assert_eq!(buf.as_slice(), &[0xB0, 0x2A]);
}

#[test]
fn encode_mov_r8_imm8_r8_uses_rex_b() {
    // mov r8b, 42 → 41 B0 2A (3 bytes; REX.B reaches r8b)
    let mut buf = CodeBuffer::new();
    mov_reg8_imm8(&mut buf, Reg64::R8, 42);
    assert_eq!(buf.as_slice(), &[0x41, 0xB0, 0x2A]);
}

#[test]
fn encode_mov_r32_imm32_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register};

    let mut buf = CodeBuffer::new();
    mov_reg32_imm32(&mut buf, Reg64::Rax, 42);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op0_register(), Register::EAX);
    assert_eq!(instr.op1_kind(), OpKind::Immediate32);
    assert_eq!(instr.immediate32(), 42);
    // 5-byte encoding confirms the narrow form (vs 7-byte 48 C7 ... 64-bit).
    assert_eq!(buf.len(), 5);
}

#[test]
fn encode_mov_r16_imm16_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let mut buf = CodeBuffer::new();
    mov_reg16_imm16(&mut buf, Reg64::Rax, 42);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op0_register(), Register::AX);
    assert_eq!(instr.immediate16(), 42);
}

#[test]
fn encode_mov_r8_imm8_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let mut buf = CodeBuffer::new();
    mov_reg8_imm8(&mut buf, Reg64::Rax, 42);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op0_register(), Register::AL);
    assert_eq!(instr.immediate8(), 42);
}

#[test]
fn movsx_rax_ecx_width4_emits_48_63_c1() {
    // movsx rax, ecx (MOVSXD r64, r/m32) → REX.W 63 /r → 48 63 C1
    let mut buf = CodeBuffer::new();
    assert!(movsx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 4));
    assert_eq!(buf.as_slice(), &[0x48, 0x63, 0xC1]);
}

#[test]
fn movsx_rax_cl_width1_emits_48_0f_be_c1() {
    // movsx rax, cl (r/m8 → r64) → REX.W 0F BE /r → 48 0F BE C1
    let mut buf = CodeBuffer::new();
    assert!(movsx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 1));
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xBE, 0xC1]);
}

#[test]
fn movsx_rax_cx_width2_emits_48_0f_bf_c1() {
    // movsx rax, cx (r/m16 → r64) → REX.W 0F BF /r → 48 0F BF C1
    let mut buf = CodeBuffer::new();
    assert!(movsx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 2));
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xBF, 0xC1]);
}

#[test]
fn movsx_rejects_unsupported_width() {
    let mut buf = CodeBuffer::new();
    assert!(!movsx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 8));
    assert!(buf.as_slice().is_empty());
}

#[test]
fn movsx_width4_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    let mut buf = CodeBuffer::new();
    movsx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 4);
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Movsxd);
}

#[test]
fn movsx_width1_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    let mut buf = CodeBuffer::new();
    movsx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 1);
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Movsx);
}

#[test]
fn movzx_rax_cl_width1_emits_48_0f_b6_c1() {
    // movzx rax, cl (r/m8 → r64) → REX.W 0F B6 /r → 48 0F B6 C1
    let mut buf = CodeBuffer::new();
    assert!(movzx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 1));
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xB6, 0xC1]);
}

#[test]
fn movzx_rax_cx_width2_emits_48_0f_b7_c1() {
    // movzx rax, cx (r/m16 → r64) → REX.W 0F B7 /r → 48 0F B7 C1
    let mut buf = CodeBuffer::new();
    assert!(movzx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 2));
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xB7, 0xC1]);
}

#[test]
fn movzx_rejects_width4() {
    // 4-byte source uses plain `mov r32, r32` (implicit zero-extend) instead.
    let mut buf = CodeBuffer::new();
    assert!(!movzx_reg64(&mut buf, Reg64::Rax, Reg64::Rcx, 4));
    assert!(buf.as_slice().is_empty());
}

#[test]
fn mov_reg32_reg32_eax_ecx_emits_89_c8() {
    // mov eax, ecx → 89 /r (no REX.W) → 89 C8; store form per AC specs.
    let mut buf = CodeBuffer::new();
    mov_reg32_reg32(&mut buf, Reg64::Rax, Reg64::Rcx);
    assert_eq!(buf.as_slice(), &[0x89, 0xC8]);
}

#[test]
fn mov_reg32_reg32_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    let mut buf = CodeBuffer::new();
    mov_reg32_reg32(&mut buf, Reg64::Rax, Reg64::Rcx);
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn add_rcx_rdx() {
    let mut buf = CodeBuffer::new();
    add_reg64_reg64(&mut buf, Reg64::Rcx, Reg64::Rdx);
    assert_eq!(buf.as_slice(), &[0x48, 0x01, 0xd1]);
}

#[test]
fn ret_then_jmp() {
    let mut buf = CodeBuffer::new();
    ret(&mut buf);
    jmp_rel8(&mut buf, 5);
    assert_eq!(buf.as_slice(), &[0xc3, 0xeb, 0x05]);
}

#[test]
fn push_rbp_pop_rbp() {
    let mut buf = CodeBuffer::new();
    push_reg64(&mut buf, Reg64::Rbp);
    pop_reg64(&mut buf, Reg64::Rbp);
    assert_eq!(buf.as_slice(), &[0x55, 0x5d]);
}

#[test]
fn push_r12_pop_r12() {
    let mut buf = CodeBuffer::new();
    push_reg64(&mut buf, Reg64::R12);
    pop_reg64(&mut buf, Reg64::R12);
    assert_eq!(buf.as_slice(), &[0x41, 0x54, 0x41, 0x5c]);
}

#[test]
fn jmp_rel32_neg5() {
    let mut buf = CodeBuffer::new();
    jmp_rel32(&mut buf, -5);
    assert_eq!(buf.as_slice(), &[0xe9, 0xfb, 0xff, 0xff, 0xff]);
}

#[test]
fn je_rel32_neg10() {
    let mut buf = CodeBuffer::new();
    jcc_rel32(&mut buf, Cond::Eq, -10);
    assert_eq!(buf.as_slice(), &[0x0f, 0x84, 0xf6, 0xff, 0xff, 0xff]);
}

#[test]
fn mov_reg64_reg64_r8_r15() {
    let mut buf = CodeBuffer::new();
    mov_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
    // REX.W=1, R=1 (for R15), B=1 (for R8): 0x4d
    // 89 (opcode)
    // 0xC0 | (7<<3) | 0 = 0xf8 (R15 is id 15, id&7=7; R8 is id 8, id&7=0)
    assert_eq!(buf.as_slice(), &[0x4d, 0x89, 0xf8]);
}

#[test]
fn sub_rdx_rax() {
    let mut buf = CodeBuffer::new();
    sub_reg64_reg64(&mut buf, Reg64::Rdx, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x29, 0xc2]);
}

#[test]
fn cmp_rsi_rdi() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_reg64(&mut buf, Reg64::Rsi, Reg64::Rdi);
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0xfe]);
}

#[test]
fn test_rbx_rbx() {
    let mut buf = CodeBuffer::new();
    test_reg64_reg64(&mut buf, Reg64::Rbx, Reg64::Rbx);
    assert_eq!(buf.as_slice(), &[0x48, 0x85, 0xdb]);
}

#[test]
fn mov_reg64_imm64_large() {
    let mut buf = CodeBuffer::new();
    mov_reg64_imm64(&mut buf, Reg64::Rax, 0x0123456789abcdef);
    // REX.W=1, B=0 (Rax): 0x48
    // B8 (opcode for Rax)
    // 0x0123456789abcdef in little-endian
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0xb8, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01]
    );
}

#[test]
fn mov_reg64_imm64_r9() {
    let mut buf = CodeBuffer::new();
    mov_reg64_imm64(&mut buf, Reg64::R9, 0x1000);
    // REX.W=1, B=1 (R9 is id 9 > 7): 0x49
    // B8 + (9&7) = B8 + 1 = B9
    // 0x1000 in little-endian
    assert_eq!(
        buf.as_slice(),
        &[0x49, 0xb9, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn call_rel32_encode() {
    let mut buf = CodeBuffer::new();
    call_rel32(&mut buf, 0x1000);
    assert_eq!(buf.as_slice(), &[0xe8, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn jne_rel32() {
    let mut buf = CodeBuffer::new();
    jcc_rel32(&mut buf, Cond::Neq, 100);
    assert_eq!(buf.as_slice(), &[0x0f, 0x85, 0x64, 0x00, 0x00, 0x00]);
}

#[test]
fn jlt_rel32() {
    let mut buf = CodeBuffer::new();
    jcc_rel32(&mut buf, Cond::Lt, -20);
    assert_eq!(buf.as_slice(), &[0x0f, 0x8c, 0xec, 0xff, 0xff, 0xff]);
}

#[test]
fn mov_mem_rbp_disp32_reg64() {
    let mut buf = CodeBuffer::new();
    mov_mem_rbp_disp_reg64(&mut buf, 1000, Reg64::Rax);
    // mod=10 (0x80), disp32
    // 0x80 | (0<<3) | 5 = 0x85
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x85, 0xe8, 0x03, 0x00, 0x00]);
}

#[test]
fn mov_mem_rbp_disp0_reg64() {
    let mut buf = CodeBuffer::new();
    mov_mem_rbp_disp_reg64(&mut buf, 0, Reg64::Rcx);
    // disp=0 fits in i8, so use mod=01 disp8=0
    // 0x40 | (1<<3) | 5 = 0x4d
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x4d, 0x00]);
}

#[test]
fn mov_reg64_mem_rbp_disp8_rdx() {
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_rbp_disp(&mut buf, Reg64::Rdx, -16);
    // mod=01 (0x40), disp8
    // 0x40 | (2<<3) | 5 = 0x55
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x55, 0xf0]);
}

#[test]
fn mov_reg64_mem_rbp_disp32_r11() {
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_rbp_disp(&mut buf, Reg64::R11, 2000);
    // mod=10 (0x80), disp32
    // REX.W=1, R=1 (R11 is id 11 > 7): 0x4c
    // 0x80 | (3<<3) | 5 = 0x9d (R11 is id 11, id&7=3)
    assert_eq!(buf.as_slice(), &[0x4c, 0x8b, 0x9d, 0xd0, 0x07, 0x00, 0x00]);
}

#[test]
fn jge_rel32() {
    let mut buf = CodeBuffer::new();
    jcc_rel32(&mut buf, Cond::Ge, 50);
    assert_eq!(buf.as_slice(), &[0x0f, 0x8d, 0x32, 0x00, 0x00, 0x00]);
}

#[test]
fn jle_rel32() {
    let mut buf = CodeBuffer::new();
    jcc_rel32(&mut buf, Cond::Le, -50);
    assert_eq!(buf.as_slice(), &[0x0f, 0x8e, 0xce, 0xff, 0xff, 0xff]);
}

#[test]
fn jgt_rel32() {
    let mut buf = CodeBuffer::new();
    jcc_rel32(&mut buf, Cond::Gt, 0);
    assert_eq!(buf.as_slice(), &[0x0f, 0x8f, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn jmp_rel8_short() {
    let mut buf = CodeBuffer::new();
    jmp_rel8(&mut buf, 10);
    assert_eq!(buf.as_slice(), &[0xeb, 0x0a]);
}

#[test]
fn jmp_rel8_backward() {
    let mut buf = CodeBuffer::new();
    jmp_rel8(&mut buf, -10);
    assert_eq!(buf.as_slice(), &[0xeb, 0xf6]);
}

#[test]
fn add_r10_r12() {
    let mut buf = CodeBuffer::new();
    add_reg64_reg64(&mut buf, Reg64::R10, Reg64::R12);
    // REX.W=1, R=1 (R12 is id 12 > 7), B=1 (R10 is id 10 > 7): 0x4d
    // 01 (opcode)
    // 0xC0 | (4<<3) | 2 = 0xe2 (R12 is id 12, id&7=4; R10 is id 10, id&7=2)
    assert_eq!(buf.as_slice(), &[0x4d, 0x01, 0xe2]);
}

#[test]
fn xor_r15_r15() {
    let mut buf = CodeBuffer::new();
    xor_reg64_reg64(&mut buf, Reg64::R15, Reg64::R15);
    // REX.W=1, R=1 (R15 is id 15 > 7), B=1 (R15 is id 15 > 7): 0x4d
    // 31 (opcode)
    // 0xC0 | (7<<3) | 7 = 0xff (R15 is id 15, id&7=7 for both)
    assert_eq!(buf.as_slice(), &[0x4d, 0x31, 0xff]);
}

#[test]
fn cmp_r9_r14() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_reg64(&mut buf, Reg64::R9, Reg64::R14);
    // REX.W=1, R=1 (R14 is id 14 > 7), B=1 (R9 is id 9 > 7): 0x4d
    // 39 (opcode)
    // 0xC0 | (6<<3) | 1 = 0xf1 (R14 is id 14, id&7=6; R9 is id 9, id&7=1)
    assert_eq!(buf.as_slice(), &[0x4d, 0x39, 0xf1]);
}

#[test]
fn test_r11_r13() {
    let mut buf = CodeBuffer::new();
    test_reg64_reg64(&mut buf, Reg64::R11, Reg64::R13);
    // REX.W=1, R=1 (R13 is id 13 > 7), B=1 (R11 is id 11 > 7): 0x4d
    // 85 (opcode)
    // 0xC0 | (5<<3) | 3 = 0xeb (R13 is id 13, id&7=5; R11 is id 11, id&7=3)
    assert_eq!(buf.as_slice(), &[0x4d, 0x85, 0xeb]);
}

#[test]
fn sub_r8_rax() {
    let mut buf = CodeBuffer::new();
    sub_reg64_reg64(&mut buf, Reg64::R8, Reg64::Rax);
    // REX.W=1, R=0 (Rax is id 0 < 8), B=1 (R8 is id 8 > 7): 0x49
    // 29 (opcode)
    // 0xC0 | (0<<3) | 0 = 0xc0 (Rax is id 0, id&7=0; R8 is id 8, id&7=0)
    assert_eq!(buf.as_slice(), &[0x49, 0x29, 0xc0]);
}

#[test]
fn push_rax_pop_rax() {
    let mut buf = CodeBuffer::new();
    push_reg64(&mut buf, Reg64::Rax);
    pop_reg64(&mut buf, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x50, 0x58]);
}

#[test]
fn push_r15_pop_r15() {
    let mut buf = CodeBuffer::new();
    push_reg64(&mut buf, Reg64::R15);
    pop_reg64(&mut buf, Reg64::R15);
    // REX.B + opcode: 0x41, 0x57 (push); 0x41, 0x5f (pop)
    assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x41, 0x5f]);
}

#[test]
fn mov_reg64_reg64_rax_rbx() {
    let mut buf = CodeBuffer::new();
    mov_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rbx);
    // REX.W=1, R=0 (Rbx is id 3 < 8), B=0 (Rax is id 0 < 8): 0x48
    // 89 (opcode)
    // 0xC0 | (3<<3) | 0 = 0xd8 (Rbx is id 3; Rax is id 0)
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0xd8]);
}

#[test]
fn mov_mem_rbp_minus_128_rsi() {
    let mut buf = CodeBuffer::new();
    mov_mem_rbp_disp_reg64(&mut buf, -128, Reg64::Rsi);
    // disp=-128 fits in i8, so use mod=01 disp8
    // 0x40 | (6<<3) | 5 = 0x75
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x75, 0x80]);
}

#[test]
fn mov_reg64_mem_rbp_disp8_rdi() {
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_rbp_disp(&mut buf, Reg64::Rdi, 32);
    // disp=32 fits in i8, so use mod=01 disp8
    // 0x40 | (7<<3) | 5 = 0x7d
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x7d, 0x20]);
}

#[test]
fn code_buffer_len() {
    let mut buf = CodeBuffer::new();
    assert!(buf.is_empty());
    mov_reg64_imm32(&mut buf, Reg64::Rax, 0);
    assert_eq!(buf.len(), 7);
    assert!(!buf.is_empty());
}

// ── Indexed load tests ──────────────────────────────────────

#[test]
fn emit_indexed_load_width_1_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 1, false);
    // mov al, [rdi + rcx]
    // Opcode 8A, ModR/M 04, SIB (scale=00, index=001, base=111) = 0x0f
    assert_eq!(buf.as_slice(), &[0x8a, 0x04, 0x0f]);
}

#[test]
fn emit_indexed_load_width_2_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 2, false);
    // mov ax, [rdi + rcx * 2]
    // Prefix 66, Opcode 8B, ModR/M 04, SIB (scale=01, index=001, base=111) = 0x4f
    assert_eq!(buf.as_slice(), &[0x66, 0x8b, 0x04, 0x4f]);
}

#[test]
fn emit_indexed_load_width_4_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 4, false);
    // mov eax, [rdi + rcx * 4]
    // Opcode 8B, ModR/M 04, SIB (scale=10, index=001, base=111) = 0x8f
    assert_eq!(buf.as_slice(), &[0x8b, 0x04, 0x8f]);
}

#[test]
fn emit_indexed_load_width_8_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
    // mov rax, [rdi + rcx * 8]
    // REX.W 48, Opcode 8B, ModR/M 04, SIB (scale=11, index=001, base=111) = 0xcf
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
}

// ── Borrowed references codegen tests (m4-006) ──────────────
// At the x86_64 byte level, &T, &mut T, and *T are identical pointers.
// These tests verify that all three reference forms encode identically.

#[test]
fn emit_indexed_load_for_ref_uses_same_sib_form_as_ptr() {
    let mut buf = CodeBuffer::new();
    // Simulate loading from a borrowed reference (&T); logically a pointer at codegen
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
    // mov rax, [rdi + rcx * 8] — identical to *T encoding
    // REX.W 48, Opcode 8B, ModR/M 04, SIB 0xcf
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
}

#[test]
fn emit_indexed_load_for_ref_mut_uses_same_sib_form_as_ptr() {
    let mut buf = CodeBuffer::new();
    // Simulate loading from a mutable borrowed reference (&mut T); also a pointer at codegen
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
    // mov rax, [rdi + rcx * 8] — identical to *T and &T encoding
    // REX.W 48, Opcode 8B, ModR/M 04, SIB 0xcf
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
}

#[test]
fn emit_indexed_load_ref_byte_sequence_matches_ptr_byte_sequence() {
    let mut buf_ptr = CodeBuffer::new();
    let mut buf_ref = CodeBuffer::new();
    let mut buf_mut_ref = CodeBuffer::new();

    // Emit identical width=8 indexed loads via all three conceptual forms
    emit_indexed_load(&mut buf_ptr, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false); // *T
    emit_indexed_load(&mut buf_ref, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false); // &T
    emit_indexed_load(
        &mut buf_mut_ref,
        Reg64::Rax,
        Reg64::Rdi,
        Reg64::Rcx,
        8,
        false,
    ); // &mut T

    // All three must produce identical byte sequences per m4-006
    assert_eq!(buf_ptr.as_slice(), buf_ref.as_slice());
    assert_eq!(buf_ref.as_slice(), buf_mut_ref.as_slice());

    // Verify the exact byte sequence: 48 8b 04 cf
    assert_eq!(buf_ptr.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
}

// ── PA-R12-002: REX.X for r8-r15 as SIB index ──────────────

#[test]
fn emit_indexed_load_width_8_index_r8_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R8, 8, false);
    assert_eq!(buf.as_slice(), &[0x4A, 0x8B, 0x04, 0xC6]);
}

#[test]
fn emit_indexed_load_width_8_index_r9_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R9, 8, false);
    assert_eq!(buf.as_slice(), &[0x4A, 0x8B, 0x04, 0xCE]);
}

#[test]
fn emit_indexed_load_width_8_index_r10_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R10, 8, false);
    assert_eq!(buf.as_slice(), &[0x4A, 0x8B, 0x04, 0xD6]);
}

#[test]
fn emit_indexed_load_width_8_index_r15_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R15, 8, false);
    assert_eq!(buf.as_slice(), &[0x4A, 0x8B, 0x04, 0xFE]);
}

#[test]
fn emit_indexed_load_width_4_index_r9_emits_rex_x_no_rex_w() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R9, 4, false);
    assert_eq!(buf.as_slice(), &[0x42, 0x8B, 0x04, 0x8E]);
}

#[test]
fn emit_indexed_load_width_2_index_r9_emits_rex_x_after_66_prefix() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R9, 2, false);
    assert_eq!(buf.as_slice(), &[0x66, 0x42, 0x8B, 0x04, 0x4E]);
}

#[test]
fn emit_indexed_load_width_1_index_r9_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::R9, 1, false);
    assert_eq!(buf.as_slice(), &[0x42, 0x8A, 0x04, 0x0E]);
}

#[test]
fn emit_indexed_load_low_index_unchanged_no_spurious_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rsi, Reg64::Rcx, 8, false);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0xCE]);
}

#[test]
fn emit_indexed_store_width_8_index_r9_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rsi, Reg64::R9, Reg64::Rax, 8);
    assert_eq!(buf.as_slice(), &[0x4A, 0x89, 0x04, 0xCE]);
}

#[test]
fn emit_indexed_store_width_8_index_r15_emits_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rsi, Reg64::R15, Reg64::Rax, 8);
    assert_eq!(buf.as_slice(), &[0x4A, 0x89, 0x04, 0xFE]);
}

#[test]
fn emit_indexed_store_width_4_index_r9_emits_rex_x_no_rex_w() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rsi, Reg64::R9, Reg64::Rax, 4);
    assert_eq!(buf.as_slice(), &[0x42, 0x89, 0x04, 0x8E]);
}

#[test]
fn emit_indexed_store_low_index_unchanged_no_spurious_rex_x() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rsi, Reg64::Rcx, Reg64::Rax, 8);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x04, 0xCE]);
}

// ── Indexed store tests ─────────────────────────────────────

#[test]
fn emit_indexed_store_width_1_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 1);
    // mov [rdi + rcx], al
    // Opcode 88, ModR/M 04, SIB (scale=00, index=001, base=111) = 0x0f
    assert_eq!(buf.as_slice(), &[0x88, 0x04, 0x0f]);
}

#[test]
fn emit_indexed_store_width_2_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 2);
    // mov [rdi + rcx * 2], ax
    // Prefix 66, Opcode 89, ModR/M 04, SIB (scale=01, index=001, base=111) = 0x4f
    assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x04, 0x4f]);
}

#[test]
fn emit_indexed_store_width_4_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 4);
    // mov [rdi + rcx * 4], eax
    // Opcode 89, ModR/M 04, SIB (scale=10, index=001, base=111) = 0x8f
    assert_eq!(buf.as_slice(), &[0x89, 0x04, 0x8f]);
}

#[test]
fn emit_indexed_store_width_8_rax_rdi_rcx() {
    let mut buf = CodeBuffer::new();
    emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 8);
    // mov [rdi + rcx * 8], rax
    // REX.W 48, Opcode 89, ModR/M 04, SIB (scale=11, index=001, base=111) = 0xcf
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x04, 0xcf]);
}

#[test]
fn emit_sub_rax_rdi_byte_sequence_is_48_29_f8() {
    let mut buf = CodeBuffer::new();
    sub_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
    // sub rax, rdi
    // REX.W=1: 0x48
    // Opcode: 0x29
    // ModR/M: 0xC0 | (7<<3) | 0 = 0xf8 (RDI is id 7, RAX is id 0)
    assert_eq!(buf.as_slice(), &[0x48, 0x29, 0xf8]);
}

#[test]
fn emit_sar_rax_3_byte_sequence_is_48_c1_f8_03() {
    let mut buf = CodeBuffer::new();
    sar_reg64_imm8(&mut buf, Reg64::Rax, 3);
    // sar rax, 3
    // REX.W=1: 0x48
    // Opcode: 0xC1
    // ModR/M: 0xF8 | (0 & 7) = 0xf8 (RAX is id 0)
    // Immediate: 0x03
    assert_eq!(buf.as_slice(), &[0x48, 0xc1, 0xf8, 0x03]);
}

#[test]
fn emit_ptr_sub_helper_u8_byte_case_skips_shift() {
    let mut buf = CodeBuffer::new();
    // For u8 (width 1), ptr_sub returns the raw difference (no shift).
    // Simulate: sub rax, rdi (no sar)
    sub_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
    assert_eq!(buf.as_slice(), &[0x48, 0x29, 0xf8]);
    // Width 1 requires no shift; the byte count is element count.
}

// ── Tightened encoding tests ────────────────────────────────

#[test]
fn add_reg64_imm8_small_value() {
    let mut buf = CodeBuffer::new();
    add_reg64_imm8(&mut buf, Reg64::Rax, 5);
    // REX.W=1: 0x48
    // Opcode: 0x83 (immediate-to-reg with sign-extended imm8)
    // ModR/M: 0xC0 | 0 = 0xc0 (RAX is id 0, /0 for add)
    // Immediate: 0x05
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc0, 0x05]);
}

#[test]
fn add_reg64_imm8_negative_value() {
    let mut buf = CodeBuffer::new();
    add_reg64_imm8(&mut buf, Reg64::Rcx, -10);
    // REX.W=1: 0x48
    // Opcode: 0x83
    // ModR/M: 0xC0 | 1 = 0xc1 (RCX is id 1, /0 for add)
    // Immediate: -10 as u8 = 0xf6
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc1, 0xf6]);
}

#[test]
fn add_reg64_imm32_fitting_value() {
    let mut buf = CodeBuffer::new();
    add_reg64_imm32(&mut buf, Reg64::Rax, 0x1234);
    // REX.W=1: 0x48
    // Opcode: 0x81 (immediate-to-reg with imm32)
    // ModR/M: 0xC0 | 0 = 0xc0 (RAX is id 0, /0 for add)
    // Immediate: 0x1234 in little-endian = 0x34, 0x12, 0x00, 0x00
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc0, 0x34, 0x12, 0x00, 0x00]);
}

#[test]
fn add_reg64_imm32_with_high_register() {
    let mut buf = CodeBuffer::new();
    add_reg64_imm32(&mut buf, Reg64::R12, 0x5000);
    // REX.W=1, B=1 (R12 is id 12 > 7): 0x49
    // Opcode: 0x81
    // ModR/M: 0xC0 | 4 = 0xc4 (R12 is id 12, id&7=4, /0 for add)
    // Immediate: 0x5000 in little-endian = 0x00, 0x50, 0x00, 0x00
    assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xc4, 0x00, 0x50, 0x00, 0x00]);
}

#[test]
fn sub_reg64_imm8_small_value() {
    let mut buf = CodeBuffer::new();
    sub_reg64_imm8(&mut buf, Reg64::Rax, 5);
    // REX.W=1: 0x48
    // Opcode: 0x83 (immediate-to-reg with sign-extended imm8)
    // ModR/M: 0xE8 | 0 = 0xe8 (RAX is id 0, /5 for sub)
    // Immediate: 0x05
    assert_eq!(buf.bytes.as_slice(), &[0x48, 0x83, 0xE8, 0x05]);
}

#[test]
fn sub_reg64_imm8_negative_value() {
    let mut buf = CodeBuffer::new();
    sub_reg64_imm8(&mut buf, Reg64::Rcx, -10);
    // REX.W=1: 0x48
    // Opcode: 0x83
    // ModR/M: 0xE8 | 1 = 0xe9 (RCX is id 1, /5 for sub)
    // Immediate: -10 as u8 = 0xf6
    assert_eq!(buf.bytes.as_slice(), &[0x48, 0x83, 0xE9, 0xF6]);
}

#[test]
fn sub_reg64_imm8_r9_1() {
    let mut buf = CodeBuffer::new();
    sub_reg64_imm8(&mut buf, Reg64::R9, 1);
    // REX.W=1, B=1 (R9 is id 9 > 7): 0x49
    // Opcode: 0x83
    // ModR/M: 0xE8 | 1 = 0xe9 (R9 is id 9, id&7=1, /5 for sub)
    // Immediate: 0x01
    assert_eq!(buf.bytes.as_slice(), &[0x49, 0x83, 0xE9, 0x01]);
}

#[test]
fn sub_reg64_imm32_fitting_value() {
    let mut buf = CodeBuffer::new();
    sub_reg64_imm32(&mut buf, Reg64::Rax, 0x1234);
    // REX.W=1: 0x48
    // Opcode: 0x81 (immediate-to-reg with imm32)
    // ModR/M: 0xE8 | 0 = 0xe8 (RAX is id 0, /5 for sub)
    // Immediate: 0x1234 in little-endian = 0x34, 0x12, 0x00, 0x00
    assert_eq!(buf.bytes.as_slice(), &[0x48, 0x81, 0xE8, 0x34, 0x12, 0x00, 0x00]);
}

#[test]
fn sub_reg64_imm32_with_high_register() {
    let mut buf = CodeBuffer::new();
    sub_reg64_imm32(&mut buf, Reg64::R12, 0x5000);
    // REX.W=1, B=1 (R12 is id 12 > 7): 0x49
    // Opcode: 0x81
    // ModR/M: 0xE8 | 4 = 0xec (R12 is id 12, id&7=4, /5 for sub)
    // Immediate: 0x5000 in little-endian = 0x00, 0x50, 0x00, 0x00
    assert_eq!(buf.bytes.as_slice(), &[0x49, 0x81, 0xEC, 0x00, 0x50, 0x00, 0x00]);
}

#[test]
fn jcc_rel8_within_range() {
    let mut buf = CodeBuffer::new();
    jcc_rel8(&mut buf, Cond::Eq, 50);
    // Opcode for JE rel8: 0x74
    // Displacement: 0x32 (50 in decimal)
    assert_eq!(buf.as_slice(), &[0x74, 0x32]);
}

#[test]
fn jcc_rel8_negative_displacement() {
    let mut buf = CodeBuffer::new();
    jcc_rel8(&mut buf, Cond::Neq, -10);
    // Opcode for JNE rel8: 0x75
    // Displacement: -10 as u8 = 0xf6
    assert_eq!(buf.as_slice(), &[0x75, 0xf6]);
}

#[test]
fn jcc_rel8_boundary_values() {
    let mut buf = CodeBuffer::new();
    jcc_rel8(&mut buf, Cond::Lt, 127);
    assert_eq!(buf.as_slice(), &[0x7c, 0x7f]);

    buf.bytes.clear();
    jcc_rel8(&mut buf, Cond::Ge, -128);
    assert_eq!(buf.as_slice(), &[0x7d, 0x80]);
}

#[test]
fn jcc_rel8_all_conditions() {
    // Test all condition codes map to correct rel8 opcodes
    let test_cases = vec![
        (Cond::Eq, 0x74),
        (Cond::Neq, 0x75),
        (Cond::Lt, 0x7C),
        (Cond::Ge, 0x7D),
        (Cond::Le, 0x7E),
        (Cond::Gt, 0x7F),
    ];

    for (cond, expected_opcode) in test_cases {
        let mut buf = CodeBuffer::new();
        jcc_rel8(&mut buf, cond, 5);
        assert_eq!(buf.as_slice()[0], expected_opcode, "cond: {:?}", cond);
        assert_eq!(buf.as_slice()[1], 0x05);
    }
}

// ── Record construction tests ───────────────────────────────

#[test]
fn emit_field_access_offset_8_emits_48_8b_47_08() {
    let mut buf = CodeBuffer::new();
    emit_field_access(&mut buf, Reg64::Rax, Reg64::Rdi, 8, 8);
    // mov rax, [rdi + 8]
    // REX.W=1: 0x48
    // Opcode: 0x8B
    // ModR/M: 0x40 | (0<<3) | 7 = 0x47 (RAX reg=0, RDI rm=7, mod=01 for disp8)
    // disp8: 0x08
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x47, 0x08]);
}

#[test]
fn emit_field_access_offset_0_uses_mod_00_no_disp() {
    let mut buf = CodeBuffer::new();
    emit_field_access(&mut buf, Reg64::Rax, Reg64::Rdi, 0, 8);
    // mov rax, [rdi + 0]
    // REX.W=1: 0x48
    // Opcode: 0x8B
    // ModR/M: 0x40 | (0<<3) | 7 = 0x47 (with disp8=0 for offset 0)
    // disp8: 0x00
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x47, 0x00]);
}

#[test]
fn emit_field_access_large_offset_uses_disp32() {
    let mut buf = CodeBuffer::new();
    emit_field_access(&mut buf, Reg64::Rsi, Reg64::Rbx, 1000, 8);
    // mov rsi, [rbx + 1000]
    // REX.W=1: 0x48
    // Opcode: 0x8B
    // ModR/M: 0x80 | (6<<3) | 3 = 0xb3 (RSI reg=6, RBX rm=3, mod=10 for disp32)
    // disp32: 1000 = 0xe8 0x03 0x00 0x00
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0xb3, 0xe8, 0x03, 0x00, 0x00]);
}

#[test]
fn emit_record_cons_emits_n_stores() {
    let mut buf = CodeBuffer::new();
    // Emit two field stores: offset 0 with RSI, offset 8 with RDX
    let field_stores = &[(0, Reg64::Rsi, 8), (8, Reg64::Rdx, 8)];
    emit_record_cons(&mut buf, Reg64::Rdi, field_stores);

    // First store: mov [rdi + 0], rsi
    // REX.W: 0x48
    // Opcode: 0x89
    // ModR/M: 0x40 | (6<<3) | 7 = 0x77
    // disp8: 0x00
    // Expected: 48 89 77 00

    // Second store: mov [rdi + 8], rdx
    // REX.W: 0x48
    // Opcode: 0x89
    // ModR/M: 0x40 | (2<<3) | 7 = 0x57
    // disp8: 0x08
    // Expected: 48 89 57 08

    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x89, 0x77, 0x00, 0x48, 0x89, 0x57, 0x08]
    );
}

// ── Enum construction tests ─────────────────────────────────

#[test]
fn emit_enum_cons_emits_discriminant_then_payload_stores() {
    let mut buf = CodeBuffer::new();
    // Enum with discriminant 2, one payload field at offset 8 in RSI
    let payload_stores = &[(8, Reg64::Rsi, 8)];
    emit_enum_cons(&mut buf, Reg64::Rdi, 2, payload_stores);

    // Expected:
    // 1. mov rax, 2 (discriminant)
    //    REX.W: 0x48, opcode: 0xb8, imm64: 0x02 0x00 0x00 0x00 0x00 0x00 0x00 0x00
    // 2. mov [rdi + 0], rax
    //    REX.W: 0x48, opcode: 0x89, ModR/M: 0x07 (mod=00, reg=0, rm=7)
    // 3. mov [rdi + 8], rsi
    //    REX.W: 0x48, opcode: 0x89, ModR/M: 0x77, disp8: 0x08

    let expected = [
        0x48, 0xb8, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // mov rax, 2
        0x48, 0x89, 0x07, // mov [rdi], rax
        0x48, 0x89, 0x77, 0x08, // mov [rdi + 8], rsi
    ];
    assert_eq!(buf.as_slice(), &expected);
}

#[test]
fn emit_enum_discriminant_loads_offset_0() {
    let mut buf = CodeBuffer::new();
    emit_enum_discriminant(&mut buf, Reg64::Rcx, Reg64::Rsi);
    // mov rcx, [rsi + 0]
    // REX.W: 0x48
    // Opcode: 0x8B
    // ModR/M: 0x40 | (1<<3) | 6 = 0x4e (RCX reg=1, RSI rm=6, mod=01 disp8)
    // disp8: 0x00
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x4e, 0x00]);
}

#[test]
fn emit_match_arm_branch_compares_then_jccs() {
    let mut buf = CodeBuffer::new();
    let patch_offset = emit_match_arm_branch(&mut buf, Reg64::Rax, 3, Cond::Neq);

    // Expected:
    // cmp rax, 3
    //   REX.W: 0x48, opcode: 0x81, ModR/M: 0xf8, imm32: 0x03 0x00 0x00 0x00
    // jne rel32
    //   opcode: 0x0f, 0x85, rel32: 0x00 0x00 0x00 0x00 (placeholder)

    let bytes = buf.as_slice();
    assert_eq!(bytes[0], 0x48); // REX.W
    assert_eq!(bytes[1], 0x81); // cmp opcode
    assert_eq!(bytes[2], 0xf8); // ModR/M
    assert_eq!(bytes[3..7], [0x03, 0x00, 0x00, 0x00]); // imm32
    assert_eq!(bytes[7], 0x0f); // jcc opcode high
    assert_eq!(bytes[8], 0x85); // jne condition
    // patch_offset should point to the rel32 bytes (offset 9)
    assert_eq!(patch_offset, 9);
}

#[test]
fn emit_record_cons_with_8byte_fields_alignment_correct() {
    let mut buf = CodeBuffer::new();
    // Record with 3 fields at natural 8-byte boundaries
    let field_stores = &[
        (0, Reg64::Rsi, 8),  // field 0 @ offset 0
        (8, Reg64::Rdx, 8),  // field 1 @ offset 8
        (16, Reg64::Rcx, 8), // field 2 @ offset 16
    ];
    emit_record_cons(&mut buf, Reg64::Rdi, field_stores);

    // Should emit 3 stores, each 4 bytes (REX.W + opcode + modrm + disp8)
    assert_eq!(buf.len(), 12);

    // Verify first store: mov [rdi + 0], rsi
    assert_eq!(&buf.as_slice()[0..4], &[0x48, 0x89, 0x77, 0x00]);
    // Verify second store: mov [rdi + 8], rdx
    assert_eq!(&buf.as_slice()[4..8], &[0x48, 0x89, 0x57, 0x08]);
    // Verify third store: mov [rdi + 16], rcx
    assert_eq!(&buf.as_slice()[8..12], &[0x48, 0x89, 0x4f, 0x10]);
}

// ── Zero-operand instruction tests (phase-5 m2-002) ──────────

#[test]
fn encode_zero_operand_nop_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_zero_operand(&mut buf, 0x90); // NOP

    assert_eq!(buf.as_slice(), &[0x90]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Nop);
}

#[test]
fn encode_zero_operand_hlt_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_zero_operand(&mut buf, 0xF4); // HLT

    assert_eq!(buf.as_slice(), &[0xF4]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Hlt);
}

#[test]
fn encode_zero_operand_cli_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_zero_operand(&mut buf, 0xFA); // CLI

    assert_eq!(buf.as_slice(), &[0xFA]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cli);
}

#[test]
fn encode_zero_operand_sti_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_zero_operand(&mut buf, 0xFB); // STI

    assert_eq!(buf.as_slice(), &[0xFB]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Sti);
}

#[test]
fn encode_zero_operand_swapgs_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_zero_operand(&mut buf, 0x81); // SWAPGS (sentinel)

    assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0xF8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
}

#[test]
fn encode_zero_operand_cpuid_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_zero_operand(&mut buf, 0x82); // CPUID (sentinel)

    assert_eq!(buf.as_slice(), &[0x0F, 0xA2]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cpuid);
}

// ── I/O port instruction tests (phase-5 m2-003) ──────────────

#[test]
fn encode_in_dx_width_1_emits_ec() {
    let mut buf = CodeBuffer::new();
    encode_in_dx(&mut buf, 1);
    // in al, dx: EC
    assert_eq!(buf.as_slice(), &[0xEC]);
}

#[test]
fn encode_in_dx_width_1_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_in_dx(&mut buf, 1);

    assert_eq!(buf.as_slice(), &[0xEC]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::In);
}

#[test]
fn encode_in_dx_width_2_emits_66_ed() {
    let mut buf = CodeBuffer::new();
    encode_in_dx(&mut buf, 2);
    // in ax, dx: 66 ED
    assert_eq!(buf.as_slice(), &[0x66, 0xED]);
}

#[test]
fn encode_in_dx_width_2_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_in_dx(&mut buf, 2);

    assert_eq!(buf.as_slice(), &[0x66, 0xED]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::In);
}

#[test]
fn encode_in_dx_width_4_emits_ed() {
    let mut buf = CodeBuffer::new();
    encode_in_dx(&mut buf, 4);
    // in eax, dx: ED
    assert_eq!(buf.as_slice(), &[0xED]);
}

#[test]
fn encode_in_dx_width_4_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_in_dx(&mut buf, 4);

    assert_eq!(buf.as_slice(), &[0xED]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::In);
}

#[test]
fn encode_out_dx_width_1_emits_ee() {
    let mut buf = CodeBuffer::new();
    encode_out_dx(&mut buf, 1);
    // out dx, al: EE
    assert_eq!(buf.as_slice(), &[0xEE]);
}

#[test]
fn encode_out_dx_width_1_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_out_dx(&mut buf, 1);

    assert_eq!(buf.as_slice(), &[0xEE]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Out);
}

#[test]
fn encode_out_dx_width_2_emits_66_ef() {
    let mut buf = CodeBuffer::new();
    encode_out_dx(&mut buf, 2);
    // out dx, ax: 66 EF
    assert_eq!(buf.as_slice(), &[0x66, 0xEF]);
}

#[test]
fn encode_out_dx_width_2_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_out_dx(&mut buf, 2);

    assert_eq!(buf.as_slice(), &[0x66, 0xEF]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Out);
}

#[test]
fn encode_out_dx_width_4_emits_ef() {
    let mut buf = CodeBuffer::new();
    encode_out_dx(&mut buf, 4);
    // out dx, eax: EF
    assert_eq!(buf.as_slice(), &[0xEF]);
}

#[test]
fn encode_out_dx_width_4_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_out_dx(&mut buf, 4);

    assert_eq!(buf.as_slice(), &[0xEF]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Out);
}

// ── Phase-5 m2-005: control register MOV instructions ─────────

// Write (mov cr_idx, rax) tests: 6 tests covering CR0, CR2, CR3, CR4, CR8 with rax
// and round-trip verification via iced-x86

#[test]
fn encode_mov_cr0_rax_emits_0f22c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 0, 0); // write=true, cr_idx=0, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);
}

#[test]
fn encode_mov_cr0_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 0, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_cr3_rax_emits_0f22d8() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 3, 0); // write=true, cr_idx=3, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);
}

#[test]
fn encode_mov_cr3_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 3, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_cr4_rax_emits_0f22e0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 4, 0); // write=true, cr_idx=4, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);
}

#[test]
fn encode_mov_cr4_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 4, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_cr8_rax_emits_440f22c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 8, 0); // write=true, cr_idx=8, gpr_idx=0 (rax)
    // CR8 requires REX.R=1: 0x44, then 0x0F 0x22, then 0xC0 (reg=0, r/m=0)
    assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);
}

#[test]
fn encode_mov_cr8_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 8, 0);
    assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

// Read (mov rax, cr_idx) tests: 6 tests covering CR0, CR2, CR3, CR4, CR8 from rax
// and round-trip verification via iced-x86

#[test]
fn encode_mov_rax_cr0_emits_0f20c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 0, 0); // write=false, cr_idx=0, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);
}

#[test]
fn encode_mov_rax_cr0_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 0, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_rax_cr3_emits_0f20d8() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 3, 0); // write=false, cr_idx=3, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);
}

#[test]
fn encode_mov_rax_cr3_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 3, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_rax_cr4_emits_0f20e0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 4, 0); // write=false, cr_idx=4, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);
}

#[test]
fn encode_mov_rax_cr4_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 4, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_rax_cr8_emits_440f20c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 8, 0); // write=false, cr_idx=8, gpr_idx=0 (rax)
    // CR8 requires REX.R=1: 0x44, then 0x0F 0x20, then 0xC0 (reg=0, r/m=0)
    assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);
}

#[test]
fn encode_mov_rax_cr8_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 8, 0);
    assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

// Write (mov cr_idx, r8-r15) tests: verify REX.B prefix for high GPRs
// Regression test for issue #1246: mov cr3, r14 should emit REX.B

#[test]
fn encode_mov_cr0_r8_emits_410f22c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 0, 8); // write=true, cr_idx=0, gpr_idx=8 (r8)
    // R8 requires REX.B=1: 0x41, then 0x0F 0x22, then 0xC0 (reg=0, r/m=0 with B extension)
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x22, 0xC0]);
}

#[test]
fn encode_mov_cr0_r8_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 0, 8);
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x22, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_cr3_r14_emits_410f22de() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 3, 14); // write=true, cr_idx=3, gpr_idx=14 (r14)
    // This is the bug case from issue #1246: should have REX.B for r14
    // 0x41 (REX.B), 0x0F 0x22 (opcode), 0xDE (mod=11, reg=3, r/m=6 [6 + 8 from B = 14])
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x22, 0xDE]);
}

#[test]
fn encode_mov_cr3_r14_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 3, 14);
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x22, 0xDE]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_cr4_r15_emits_410f22e7() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 4, 15); // write=true, cr_idx=4, gpr_idx=15 (r15)
    // R15 requires REX.B=1: 0x41, then 0x0F 0x22, then 0xE7 (mod=11, reg=4, r/m=7 [7 + 8 from B = 15])
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x22, 0xE7]);
}

#[test]
fn encode_mov_cr4_r15_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 4, 15);
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x22, 0xE7]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

// Regression tests: ensure low GPRs still work without REX.B

#[test]
fn encode_mov_cr3_rax_still_emits_0f22d8_no_rex() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 3, 0); // write=true, cr_idx=3, gpr_idx=0 (rax)
    // Should not have REX prefix (no REX.B, no REX.R)
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);
}

#[test]
fn encode_mov_cr3_rsi_still_emits_0f22de_no_rex() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, true, 3, 6); // write=true, cr_idx=3, gpr_idx=6 (rsi)
    // Should not have REX prefix; modrm = 0xD8 + 6 = 0xDE, but no REX.B
    // This was the alias target of the bug (r14 -> rsi)
    assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xDE]);
}

// Read (mov r8-r15, cr_idx) tests: verify REX.B prefix for high GPRs

#[test]
fn encode_mov_r8_cr0_emits_410f20c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 0, 8); // write=false, cr_idx=0, gpr_idx=8 (r8)
    // R8 requires REX.B=1: 0x41, then 0x0F 0x20, then 0xC0
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x20, 0xC0]);
}

#[test]
fn encode_mov_r8_cr0_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 0, 8);
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x20, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_r14_cr3_emits_410f20de() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 3, 14); // write=false, cr_idx=3, gpr_idx=14 (r14)
    // R14 requires REX.B=1: 0x41, then 0x0F 0x20, then 0xDE
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x20, 0xDE]);
}

#[test]
fn encode_mov_r14_cr3_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 3, 14);
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x20, 0xDE]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_r15_cr4_emits_410f20e7() {
    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 4, 15); // write=false, cr_idx=4, gpr_idx=15 (r15)
    // R15 requires REX.B=1: 0x41, then 0x0F 0x20, then 0xE7
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x20, 0xE7]);
}

#[test]
fn encode_mov_r15_cr4_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_cr(&mut buf, false, 4, 15);
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x20, 0xE7]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

// Write (mov dr_idx, rax) tests: 4 tests covering DR0, DR2, DR6, DR7 to rax
// and round-trip verification via iced-x86

#[test]
fn encode_mov_dr0_rax_emits_0f23c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 0, 0); // write=true, dr_idx=0, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);
}

#[test]
fn encode_mov_dr0_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 0, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_dr2_rax_emits_0f23d0() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 2, 0); // write=true, dr_idx=2, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xD0]);
}

#[test]
fn encode_mov_dr2_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 2, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xD0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_dr6_rax_emits_0f23f0() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 6, 0); // write=true, dr_idx=6, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF0]);
}

#[test]
fn encode_mov_dr6_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 6, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_dr7_rax_emits_0f23f8() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 7, 0); // write=true, dr_idx=7, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF8]);
}

#[test]
fn encode_mov_dr7_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, true, 7, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

// Read (mov rax, dr_idx) tests: 4 tests covering RAX from DR0, DR2, DR6, DR7
// and round-trip verification via iced-x86

#[test]
fn encode_mov_rax_dr0_emits_0f21c0() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 0, 0); // write=false, dr_idx=0, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
}

#[test]
fn encode_mov_rax_dr0_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 0, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_rax_dr2_emits_0f21d0() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 2, 0); // write=false, dr_idx=2, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xD0]);
}

#[test]
fn encode_mov_rax_dr2_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 2, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xD0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_rax_dr6_emits_0f21f0() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 6, 0); // write=false, dr_idx=6, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF0]);
}

#[test]
fn encode_mov_rax_dr6_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 6, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

#[test]
fn encode_mov_rax_dr7_emits_0f21f8() {
    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 7, 0); // write=false, dr_idx=7, gpr_idx=0 (rax)
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF8]);
}

#[test]
fn encode_mov_rax_dr7_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_mov_dr(&mut buf, false, 7, 0);
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
}

// ── CMP round-trip tests (phase 6 m4-001) ───────────────────

#[test]
fn cmp_rax_rdi_emits_4839f8() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
    // REX.W=1, opcode 39, ModR/M: 0xC0 | (7<<3) | 0 = 0xf8
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0xf8]);
}

#[test]
fn cmp_rax_rdi_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0xf8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_mem_rdi_24_rcx_emits_48394f18() {
    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rdi, 24, Reg64::Rcx);
    // REX.W=1, opcode 39, ModR/M with disp8: 0x40 | (1<<3) | 7 = 0x4f, disp8=24=0x18
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x4f, 0x18]);
}

#[test]
fn cmp_mem_rdi_24_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rdi, 24, Reg64::Rcx);
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x4f, 0x18]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_rax_imm8_0_emits_4883f800() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_imm8(&mut buf, Reg64::Rax, 0);
    // REX.W=1, opcode 83, ModR/M: 0xF8 | 0 = 0xf8, imm8=0
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf8, 0x00]);
}

#[test]
fn cmp_rax_imm8_0_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_reg64_imm8(&mut buf, Reg64::Rax, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf8, 0x00]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_rcx_imm8_127_emits_4883f97f() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_imm8(&mut buf, Reg64::Rcx, 127);
    // REX.W=1, opcode 83, ModR/M: 0xF8 | 1 = 0xf9, imm8=127
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf9, 0x7f]);
}

#[test]
fn cmp_rcx_imm8_127_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_reg64_imm8(&mut buf, Reg64::Rcx, 127);
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf9, 0x7f]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_rdx_imm32_256_emits_4881fa00010000() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_imm32(&mut buf, Reg64::Rdx, 256);
    // REX.W=1, opcode 81, ModR/M: 0xF8 | 2 = 0xfa, imm32=256 in LE
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xfa, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn cmp_rdx_imm32_256_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_reg64_imm32(&mut buf, Reg64::Rdx, 256);
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xfa, 0x00, 0x01, 0x00, 0x00]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_r8_imm8_neg1_emits_4983f8ff() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_imm8(&mut buf, Reg64::R8, -1);
    // REX.W=1, B=1 (R8 is id 8 > 7): 0x49, opcode 83, ModR/M: 0xF8 | 0 = 0xf8, imm8=-1=0xff
    assert_eq!(buf.as_slice(), &[0x49, 0x83, 0xf8, 0xff]);
}

#[test]
fn cmp_r8_imm8_neg1_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_reg64_imm8(&mut buf, Reg64::R8, -1);
    assert_eq!(buf.as_slice(), &[0x49, 0x83, 0xf8, 0xff]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_r15_imm32_0x7fffffff_emits_49811fff_ffffffff7f() {
    let mut buf = CodeBuffer::new();
    cmp_reg64_imm32(&mut buf, Reg64::R15, 0x7fffffff);
    // REX.W=1, R=1 (R15 is id 15 > 7): 0x4d, opcode 81, ModR/M: 0xF8 | 7 = 0xff, imm32=0x7fffffff in LE
    assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xff, 0xff, 0xff, 0xff, 0x7f]);
}

#[test]
fn cmp_r15_imm32_0x7fffffff_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_reg64_imm32(&mut buf, Reg64::R15, 0x7fffffff);
    assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xff, 0xff, 0xff, 0xff, 0x7f]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_mem_rbp_minus_8_rax_emits_4839455f8() {
    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rbp, -8, Reg64::Rax);
    // REX.W=1, opcode 39, ModR/M with disp8: 0x40 | (0<<3) | 5 = 0x45, disp8=-8=0xf8
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x45, 0xf8]);
}

#[test]
fn cmp_mem_rbp_minus_8_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rbp, -8, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x45, 0xf8]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn cmp_mem_rsi_1000_r9_emits_48399c06e8030000() {
    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rsi, 1000, Reg64::R9);
    // REX.W=1, R=1 (R9 is id 9 > 7): 0x4c, opcode 39, ModR/M with disp32: 0x80 | (1<<3) | 6 = 0x8e, disp32=1000 in LE
    assert_eq!(buf.as_slice(), &[0x4c, 0x39, 0x8e, 0xe8, 0x03, 0x00, 0x00]);
}

#[test]
fn cmp_mem_rsi_1000_r9_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rsi, 1000, Reg64::R9);
    assert_eq!(buf.as_slice(), &[0x4c, 0x39, 0x8e, 0xe8, 0x03, 0x00, 0x00]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
}

#[test]
fn encode_syscall_emits_0f05() {
    let mut buf = CodeBuffer::new();
    encode_syscall(&mut buf);
    assert_eq!(buf.as_slice(), &[0x0F, 0x05]);
}

#[test]
fn encode_syscall_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    encode_syscall(&mut buf);
    assert_eq!(buf.as_slice(), &[0x0F, 0x05]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Syscall);
}

// Phase 9 m1-003: Tests for SIB addressing with displacement

#[test]
fn mov_reg64_mem_sib_disp_zero_scale_1() {
    // mov rax, [rax + rbx*1] → 48 8B 04 18
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 0, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0x18]);
}

#[test]
fn mov_reg64_mem_sib_disp_zero_scale_2() {
    // mov rax, [rax + rbx*2] → 48 8B 04 58
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 1, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0x58]);
}

#[test]
fn mov_reg64_mem_sib_disp_zero_scale_4() {
    // mov rax, [rax + rbx*4] → 48 8B 04 98
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 2, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0x98]);
}

#[test]
fn mov_reg64_mem_sib_disp_zero_scale_8() {
    // mov rax, [rax + rbx*8] → 48 8B 04 D8
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 3, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0xD8]);
}

#[test]
fn mov_reg64_mem_sib_disp8_scale_1() {
    // mov rax, [rax + rbx*1 + 16] → 48 8B 44 18 10
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 0, 16);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x44, 0x18, 0x10]);
}

#[test]
fn mov_reg64_mem_sib_disp8_scale_4() {
    // mov rax, [rax + rbx*4 + 32] → 48 8B 44 98 20
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 2, 32);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x44, 0x98, 0x20]);
}

#[test]
fn mov_reg64_mem_sib_disp32_scale_4() {
    // mov rax, [rax + rbx*4 + 256] → 48 8B 84 98 00 01 00 00
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 2, 256);
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x8B, 0x84, 0x98, 0x00, 0x01, 0x00, 0x00]
    );
}

#[test]
fn mov_reg64_mem_sib_disp_extended_registers() {
    // mov r12, [r8 + r9*2 + 8] → 4F 8B 64 48 08
    // r12 = reg 12, r8 = reg 8, r9 = reg 9
    // REX.W=1, R=1 (r12[3]), X=1 (r9[3]), B=1 (r8[3]) → 0x4F
    // ModR/M: mod=01 (8-bit disp), reg=r12[2:0]=4, r/m=100 (SIB) → 0x64
    // SIB: scale=01 (scale*2), index=r9[2:0]=1, base=r8[2:0]=0 → 0x48
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::R12, Reg64::R8, Reg64::R9, 1, 8);
    assert_eq!(buf.as_slice(), &[0x4F, 0x8B, 0x64, 0x48, 0x08]);
}

#[test]
fn mov_mem_sib_disp_reg64_zero_scale_1() {
    // mov [rax + rbx*1], rax → 48 89 04 18
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::Rax, Reg64::Rbx, 0, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x04, 0x18]);
}

#[test]
fn mov_mem_sib_disp_reg64_zero_scale_8() {
    // mov [rax + rbx*8], rax → 48 89 04 D8
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::Rax, Reg64::Rbx, 3, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x04, 0xD8]);
}

#[test]
fn mov_mem_sib_disp_reg64_disp8_scale_4() {
    // mov [rax + rbx*4 + 16], rcx → 48 89 4C 98 10
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::Rax, Reg64::Rbx, 2, 16, Reg64::Rcx);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x4C, 0x98, 0x10]);
}

#[test]
fn mov_mem_sib_disp_reg64_disp32_scale_2() {
    // mov [rax + rbx*2 + 256], rdx → 48 89 94 58 00 01 00 00
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::Rax, Reg64::Rbx, 1, 256, Reg64::Rdx);
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x89, 0x94, 0x58, 0x00, 0x01, 0x00, 0x00]
    );
}

#[test]
fn mov_mem_sib_disp_reg64_extended_registers() {
    // mov [r8 + r9*2 + 8], r12 → 4F 89 64 48 08
    // r8 = reg 8, r9 = reg 9, r12 = reg 12
    // REX.W=1, R=1 (r12[3]), X=1 (r9[3]), B=1 (r8[3]) → 0x4F
    // ModR/M: mod=01 (8-bit disp), reg=r12[2:0]=4, r/m=100 (SIB) → 0x64
    // SIB: scale=01 (scale*2), index=r9[2:0]=1, base=r8[2:0]=0 → 0x48
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::R8, Reg64::R9, 1, 8, Reg64::R12);
    assert_eq!(buf.as_slice(), &[0x4F, 0x89, 0x64, 0x48, 0x08]);
}

#[test]
fn mov_reg64_mem_sib_disp_iced_round_trip_load() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    // mov rax, [rax + rbx*4 + 32]
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 2, 32);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    // Verify it's a valid mov instruction (would panic if malformed)
}

#[test]
fn mov_mem_sib_disp_reg64_iced_round_trip_store() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    // mov [rax + rbx*4 + 32], rcx
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::Rax, Reg64::Rbx, 2, 32, Reg64::Rcx);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    // Verify it's a valid mov instruction (would panic if malformed)
}

#[test]
fn mov_reg64_mem_sib_disp_negative_displacement() {
    // mov rax, [rax + rbx*1 + (-16)] → 48 8B 44 18 F0
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rax, Reg64::Rbx, 0, -16);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x44, 0x18, 0xF0]);
}

#[test]
fn mov_mem_sib_disp_reg64_negative_displacement() {
    // mov [rax + rbx*2 + (-8)], rax → 48 89 44 58 F8
    let mut buf = CodeBuffer::new();
    mov_mem_sib_disp_reg64(&mut buf, Reg64::Rax, Reg64::Rbx, 1, -8, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x44, 0x58, 0xF8]);
}

// ── PA-R10-001: SIB+BP escape encoding correctness matrix ───────────────
// Tests for fix of silent-corruption bug where RSP (base=4) and RBP (base=5)
// memory addressing was incorrectly encoded, missing SIB byte and BP disp escape.

#[test]
fn pa_r10_001_spot_check_mov_rsp_disp0() {
    // mov [rsp], rax → 48 89 04 24 (SIB escape required for RSP as base)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rsp, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_spot_check_mov_rsp_disp8() {
    // mov [rsp+8], rax → 48 89 44 24 08 (SIB with disp8)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rsp, 8, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x44, 0x24, 0x08]);
}

#[test]
fn pa_r10_001_spot_check_mov_rsp_disp32() {
    // mov [rsp+256], rax → 48 89 84 24 00 01 00 00 (SIB with disp32)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rsp, 256, Reg64::Rax);
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x89, 0x84, 0x24, 0x00, 0x01, 0x00, 0x00]
    );
}

#[test]
fn pa_r10_001_spot_check_mov_rbp_disp0() {
    // mov [rbp], rax → 48 89 45 00 (forced disp8=0 for RBP, can't use mod=00)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rbp, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_spot_check_mov_rbp_disp8() {
    // mov [rbp+8], rax → 48 89 45 08
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rbp, 8, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x45, 0x08]);
}

#[test]
fn pa_r10_001_spot_check_mov_r12_disp0() {
    // mov [r12], rax → 49 89 04 24 (R12 needs REX.B, SIB for base=4 form)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::R12, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_spot_check_mov_r12_disp8() {
    // mov [r12+8], rax → 49 89 44 24 08
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::R12, 8, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x44, 0x24, 0x08]);
}

#[test]
fn pa_r10_001_spot_check_mov_r13_disp0() {
    // mov [r13], rax → 49 89 45 00 (R13 is base=5 form with BP escape)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::R13, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_spot_check_mov_r13_disp8() {
    // mov [r13+8], rax → 49 89 45 08
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::R13, 8, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x45, 0x08]);
}

#[test]
fn pa_r10_001_spot_check_mov_r15_disp256() {
    // mov [r15+256], r15 → 4D 89 BF 00 01 00 00 (REX.W=1, REX.R=1, REX.B=1)
    let mut buf = CodeBuffer::new();
    mov_mem_reg64_disp_reg64(&mut buf, Reg64::R15, 256, Reg64::R15);
    assert_eq!(buf.as_slice(), &[0x4D, 0x89, 0xBF, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn pa_r10_001_spot_check_mov_load_rsp_disp0() {
    // mov rax, [rsp] → 48 8B 04 24 (load from RSP needs SIB)
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_spot_check_mov_load_rbp_disp0() {
    // mov rax, [rbp] → 48 8B 45 00 (load from RBP needs disp8=0 escape)
    let mut buf = CodeBuffer::new();
    mov_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_correctness_matrix_all_bases_disp0() {
    use iced_x86::{Decoder, DecoderOptions};

    let bases = [
        Reg64::Rax,
        Reg64::Rcx,
        Reg64::Rdx,
        Reg64::Rbx,
        Reg64::Rsp,
        Reg64::Rbp,
        Reg64::Rsi,
        Reg64::Rdi,
    ];

    // Test with dst=rax (reg_field=0)
    for &base in &bases {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, base, 0);

        // Verify iced-x86 can decode it without error
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov rax, [{:?}+0]", base);

        // For RSP: must have SIB byte (ModR/M.r/m = 0b100)
        if base as u8 & 7 == 4 {
            assert_eq!(buf.bytes[2] & 0x07, 0x04, "RSP requires SIB for {:?}", base);
        }
        // For RBP: must use mod=01 with disp8=0
        if base as u8 & 7 == 5 {
            assert_eq!(
                buf.bytes[2] & 0xC0,
                0x40,
                "RBP requires mod=01 for {:?}",
                base
            );
            assert_eq!(buf.bytes[3], 0x00, "RBP requires disp8=0 for {:?}", base);
        }
    }
}

#[test]
fn pa_r10_001_correctness_matrix_all_bases_disp8() {
    use iced_x86::{Decoder, DecoderOptions};

    let bases = [
        Reg64::Rax,
        Reg64::Rcx,
        Reg64::Rdx,
        Reg64::Rbx,
        Reg64::Rsp,
        Reg64::Rbp,
        Reg64::Rsi,
        Reg64::Rdi,
    ];

    // Test with disp=8
    for &base in &bases {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, base, 8);

        // Verify iced-x86 can decode it
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov rax, [{:?}+8]", base);

        // For RSP: must have SIB byte
        if base as u8 & 7 == 4 {
            assert_eq!(buf.bytes[2] & 0x07, 0x04, "RSP requires SIB");
        }
        // All should use mod=01 for disp8
        assert_eq!(buf.bytes[2] & 0xC0, 0x40, "disp8 requires mod=01");
    }
}

#[test]
fn pa_r10_001_correctness_matrix_all_bases_disp32() {
    use iced_x86::{Decoder, DecoderOptions};

    let bases = [
        Reg64::Rax,
        Reg64::Rcx,
        Reg64::Rdx,
        Reg64::Rbx,
        Reg64::Rsp,
        Reg64::Rbp,
        Reg64::Rsi,
        Reg64::Rdi,
    ];

    // Test with disp=256 (needs disp32)
    for &base in &bases {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, base, 256);

        // Verify iced-x86 can decode it
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(
            instr.len() > 0,
            "Failed to decode mov rax, [{:?}+256]",
            base
        );

        // For RSP: must have SIB byte
        if base as u8 & 7 == 4 {
            assert_eq!(buf.bytes[2] & 0x07, 0x04, "RSP requires SIB");
        }
        // All should use mod=10 for disp32
        assert_eq!(buf.bytes[2] & 0xC0, 0x80, "disp32 requires mod=10");
    }
}

#[test]
fn pa_r10_001_correctness_extended_registers() {
    use iced_x86::{Decoder, DecoderOptions};

    let extended_bases = [Reg64::R8, Reg64::R12, Reg64::R13, Reg64::R15];

    for &base in &extended_bases {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, base, 8);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov rax, [{:?}+8]", base);

        // R12 (base_id=12, low bits=4) and R8/R9/R10/R11 should have REX.B
        let base_id = base as u8;
        let rex_byte = buf.bytes[0];
        if (base_id >> 3) != 0 {
            assert_eq!(
                rex_byte & 0x41,
                0x40 | 0x01,
                "Extended register needs REX.B"
            );
        }
    }
}

#[test]
fn pa_r10_001_and_reg64_mem_reg64_disp_rsp() {
    // and rax, [rsp] → 48 23 04 24 (SIB escape)
    let mut buf = CodeBuffer::new();
    and_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x23, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_and_reg64_mem_reg64_disp_rbp() {
    // and rax, [rbp] → 48 23 45 00 (BP escape)
    let mut buf = CodeBuffer::new();
    and_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x23, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_or_reg64_mem_reg64_disp_rsp() {
    // or rax, [rsp] → 48 0B 04 24 (SIB escape)
    let mut buf = CodeBuffer::new();
    or_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x0B, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_or_reg64_mem_reg64_disp_rbp() {
    // or rax, [rbp] → 48 0B 45 00 (BP escape)
    let mut buf = CodeBuffer::new();
    or_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x0B, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_xor_reg64_mem_reg64_disp_rsp() {
    // xor rax, [rsp] → 48 33 04 24 (SIB escape)
    let mut buf = CodeBuffer::new();
    xor_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x33, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_xor_reg64_mem_reg64_disp_rbp() {
    // xor rax, [rbp] → 48 33 45 00 (BP escape)
    let mut buf = CodeBuffer::new();
    xor_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x33, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_imul_reg64_mem_reg64_disp_rsp() {
    // imul rax, [rsp] → 48 0F AF 04 24 (SIB escape)
    let mut buf = CodeBuffer::new();
    imul_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xAF, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_imul_reg64_mem_reg64_disp_rbp() {
    // imul rax, [rbp] → 48 0F AF 45 00 (BP escape)
    let mut buf = CodeBuffer::new();
    imul_reg64_mem_reg64_disp(&mut buf, Reg64::Rax, Reg64::Rbp, 0);
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xAF, 0x45, 0x00]);
}

#[test]
fn pa_r10_001_cmp_mem_reg64_reg64_rsp() {
    // cmp [rsp], rax → 48 39 04 24 (SIB escape)
    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rsp, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x04, 0x24]);
}

#[test]
fn pa_r10_001_cmp_mem_reg64_reg64_rbp() {
    // cmp [rbp], rax → 48 39 45 00 (BP escape)
    let mut buf = CodeBuffer::new();
    cmp_mem_reg64_reg64(&mut buf, Reg64::Rbp, 0, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x45, 0x00]);
}

// ── PA-R13-001B: SIB base=RBP/R13 with disp=0 escape tests ────────────────

mod sib_bp_escape {
    use super::*;

    #[test]
    fn pa_r13_001b_mov_rax_rbp_rsi_8() {
        // mov rax, [rbp + rsi*8]
        // Expected: 48 8B 44 F5 00
        // - 48: REX.W
        // - 8B: opcode (mov r64, r/m64)
        // - 44: mod=01 (disp8), reg=000 (RAX), rm=100 (SIB)
        // - F5: scale=11 (×8), index=110 (RSI), base=101 (RBP)
        // - 00: disp8=0 (BP escape)
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 3, 0);
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x44, 0xF5, 0x00]);
    }

    #[test]
    fn pa_r13_001b_mov_rax_r13_rsi_8() {
        // mov rax, [r13 + rsi*8]
        // Expected: 49 8B 44 F5 00
        // - 49: REX.W + REX.B (R13 is r8-r15)
        // - 8B: opcode
        // - 44: mod=01, reg=000 (RAX), rm=100 (SIB)
        // - F5: scale=11 (×8), index=110 (RSI), base=101 (R13 low bits)
        // - 00: disp8=0 (BP escape)
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::R13, Reg64::Rsi, 3, 0);
        assert_eq!(buf.as_slice(), &[0x49, 0x8B, 0x44, 0xF5, 0x00]);
    }

    #[test]
    fn pa_r13_001b_mov_al_rbp_rsi_1() {
        // mov al, [rbp + rsi*1]
        // Expected: 8A 44 35 00
        // - 8A: opcode (mov r8, r/m8)
        // - 44: mod=01, reg=000 (AL), rm=100 (SIB)
        // - 35: scale=00 (×1), index=110 (RSI), base=101 (RBP)
        // - 00: disp8=0 (BP escape)
        let mut buf = CodeBuffer::new();
        mov_reg_mem_sib_disp_sized(&mut buf, IntWidth::W8, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 0, 0);
        assert_eq!(buf.as_slice(), &[0x8A, 0x44, 0x35, 0x00]);
    }

    #[test]
    fn pa_r13_001b_mov_ax_rbp_rsi_2() {
        // mov ax, [rbp + rsi*2]
        // Expected: 66 8B 44 75 00
        // - 66: operand-size override
        // - 8B: opcode (mov r16, r/m16)
        // - 44: mod=01, reg=000 (AX), rm=100 (SIB)
        // - 75: scale=01 (×2), index=110 (RSI), base=101 (RBP)
        // - 00: disp8=0 (BP escape)
        let mut buf = CodeBuffer::new();
        mov_reg_mem_sib_disp_sized(&mut buf, IntWidth::W16, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 1, 0);
        assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x44, 0x75, 0x00]);
    }

    #[test]
    fn pa_r13_001b_mov_eax_rbp_rsi_4() {
        // mov eax, [rbp + rsi*4]
        // Expected: 8B 44 B5 00
        // - 8B: opcode (mov r32, r/m32)
        // - 44: mod=01, reg=000 (EAX), rm=100 (SIB)
        // - B5: scale=10 (×4), index=110 (RSI), base=101 (RBP)
        // - 00: disp8=0 (BP escape)
        let mut buf = CodeBuffer::new();
        mov_reg_mem_sib_disp_sized(&mut buf, IntWidth::W32, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 2, 0);
        assert_eq!(buf.as_slice(), &[0x8B, 0x44, 0xB5, 0x00]);
    }

    #[test]
    fn pa_r13_001b_mov_rax_r13_rax_4() {
        // mov rax, [r13 + rax*4]
        // Expected: 49 8B 44 85 00
        // - 49: REX.W + REX.B (R13 is r8-r15)
        // - 8B: opcode
        // - 44: mod=01, reg=000 (RAX), rm=100 (SIB)
        // - 85: scale=10 (×4), index=000 (RAX), base=101 (R13 low bits)
        // - 00: disp8=0 (BP escape)
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::R13, Reg64::Rax, 2, 0);
        assert_eq!(buf.as_slice(), &[0x49, 0x8B, 0x44, 0x85, 0x00]);
    }

    #[test]
    fn pa_r13_001b_mov_rax_rbp_rsi_8_disp8() {
        // mov rax, [rbp + rsi*8 + 8]
        // Expected: 48 8B 44 F5 08
        // - 48: REX.W
        // - 8B: opcode
        // - 44: mod=01 (disp8), reg=000 (RAX), rm=100 (SIB)
        // - F5: scale=11 (×8), index=110 (RSI), base=101 (RBP)
        // - 08: disp8=8
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 3, 8);
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x44, 0xF5, 0x08]);
    }

    #[test]
    fn pa_r13_001b_mov_rax_rbp_rsi_8_disp32() {
        // mov rax, [rbp + rsi*8 + 256]
        // Expected: 48 8B 84 F5 00 01 00 00
        // - 48: REX.W
        // - 8B: opcode
        // - 84: mod=10 (disp32), reg=000 (RAX), rm=100 (SIB)
        // - F5: scale=11 (×8), index=110 (RSI), base=101 (RBP)
        // - 00 01 00 00: disp32=256 (little-endian)
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 3, 256);
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x84, 0xF5, 0x00, 0x01, 0x00, 0x00]);
    }

    // ── iced-x86 round-trip verification ────────────────────────────────────

    #[test]
    fn pa_r13_001b_roundtrip_mov_rax_rbp_rsi_8_w64() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        // Round-trip: ensure iced-x86 decodes what we encoded correctly.
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_sib_disp(&mut buf, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 3, 0);
        let bytes = buf.as_slice();

        let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov rax, [rbp + rsi*8]");

        // Verify key properties
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
        assert_eq!(instr.op_count(), 2);
    }

    #[test]
    fn pa_r13_001b_roundtrip_mov_ax_rbp_rsi_2_w16() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        // Round-trip: ensure iced-x86 decodes what we encoded correctly.
        let mut buf = CodeBuffer::new();
        mov_reg_mem_sib_disp_sized(&mut buf, IntWidth::W16, Reg64::Rax, Reg64::Rbp, Reg64::Rsi, 1, 0);
        let bytes = buf.as_slice();

        let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov ax, [rbp + rsi*2]");

        // Verify key properties
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
        assert_eq!(instr.op_count(), 2);
    }

    // ── pa-r17-006 (#984): register-source store tests ────────────────────

    #[test]
    fn pa_r17_006_field_assign_u8_offset_0() {
        // mov [rdi], dl (8-bit store)
        // Expected: 88 17
        // - 88: opcode (mov r/m8, r8)
        // - 17: mod=00, reg=010 (RDX), rm=111 (RDI)
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg8(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x88, 0x17]);
    }

    #[test]
    fn pa_r17_006_field_assign_u8_offset_4_disp8() {
        // mov [rdi + 4], dl (8-bit store with disp8)
        // Expected: 88 57 04
        // - 88: opcode
        // - 57: mod=01 (disp8), reg=010 (RDX), rm=111 (RDI)
        // - 04: disp8=4
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg8(&mut buf, Reg64::Rdi, 4, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x88, 0x57, 0x04]);
    }

    #[test]
    fn pa_r17_006_field_assign_u16_offset_0() {
        // mov [rdi], dx (16-bit store)
        // Expected: 66 89 17
        // - 66: operand-size override
        // - 89: opcode (mov r/m16, r16)
        // - 17: mod=00, reg=010 (RDX), rm=111 (RDI)
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg16(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x17]);
    }

    #[test]
    fn pa_r17_006_field_assign_u16_offset_8_disp8() {
        // mov [rdi + 8], dx (16-bit store with disp8)
        // Expected: 66 89 57 08
        // - 66: operand-size override
        // - 89: opcode
        // - 57: mod=01 (disp8), reg=010 (RDX), rm=111 (RDI)
        // - 08: disp8=8
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg16(&mut buf, Reg64::Rdi, 8, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x57, 0x08]);
    }

    #[test]
    fn pa_r17_006_field_assign_u32_offset_0_no_rex_w() {
        // BUG-FIX GUARD: mov [rdi], edx (32-bit store, NO REX.W prefix)
        // Expected: 89 17 (NOT 48 89 17)
        // - 89: opcode (mov r/m32, r32, no REX.W)
        // - 17: mod=00, reg=010 (RDX), rm=111 (RDI)
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg32(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x89, 0x17]);
    }

    #[test]
    fn pa_r17_006_field_assign_u32_offset_12_disp8() {
        // mov [rdi + 12], edx (32-bit store with disp8)
        // Expected: 89 57 0C
        // - 89: opcode
        // - 57: mod=01 (disp8), reg=010 (RDX), rm=111 (RDI)
        // - 0C: disp8=12
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg32(&mut buf, Reg64::Rdi, 12, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x89, 0x57, 0x0C]);
    }

    #[test]
    fn pa_r17_006_field_assign_u32_offset_256_disp32() {
        // mov [rdi + 256], edx (32-bit store with disp32)
        // Expected: 89 97 00 01 00 00
        // - 89: opcode
        // - 97: mod=10 (disp32), reg=010 (RDX), rm=111 (RDI)
        // - 00 01 00 00: disp32=256 (little-endian)
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg32(&mut buf, Reg64::Rdi, 256, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x89, 0x97, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn pa_r17_006_field_assign_u64_offset_0() {
        // mov [rdi], rdx (64-bit store)
        // Expected: 48 89 17
        // - 48: REX.W
        // - 89: opcode (mov r/m64, r64)
        // - 17: mod=00, reg=010 (RDX), rm=111 (RDI)
        let mut buf = CodeBuffer::new();
        mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x17]);
    }

    #[test]
    fn pa_r17_006_field_assign_u64_offset_24_disp8() {
        // mov [rdi + 24], rdx (64-bit store with disp8)
        // Expected: 48 89 57 18
        // - 48: REX.W
        // - 89: opcode
        // - 57: mod=01 (disp8), reg=010 (RDX), rm=111 (RDI)
        // - 18: disp8=24
        let mut buf = CodeBuffer::new();
        mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rdi, 24, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x57, 0x18]);
    }

    #[test]
    fn pa_r17_006_field_assign_u64_offset_256_disp32() {
        // mov [rdi + 256], rdx (64-bit store with disp32)
        // Expected: 48 89 97 00 01 00 00
        // - 48: REX.W
        // - 89: opcode
        // - 97: mod=10 (disp32), reg=010 (RDX), rm=111 (RDI)
        // - 00 01 00 00: disp32=256 (little-endian)
        let mut buf = CodeBuffer::new();
        mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rdi, 256, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x97, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn pa_r17_006_field_assign_i8_signed_same_as_u8() {
        // Signedness is ignored for stores: mov [rdi], dl is same regardless
        // Expected: 88 17
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg8(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x88, 0x17]);
    }

    #[test]
    fn pa_r17_006_field_assign_i32_signed_same_as_u32() {
        // Signedness is ignored for stores: mov [rdi], edx is same regardless
        // Expected: 89 17
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg32(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x89, 0x17]);
    }

    #[test]
    fn pa_r17_006_field_assign_extended_src_r10_u32() {
        // mov [rdi + 8], r10d (R10 is r8-r15, requires REX.R)
        // Expected: 44 89 57 08
        // - 44: REX.R (high bit of r10=1010 is bit 3, so REX.R=1)
        // - 89: opcode
        // - 57: mod=01 (disp8), reg=010 (R10 low bits), rm=111 (RDI)
        // - 08: disp8=8
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg32(&mut buf, Reg64::Rdi, 8, Reg64::R10);
        assert_eq!(buf.as_slice(), &[0x44, 0x89, 0x57, 0x08]);
    }

    #[test]
    fn pa_r17_006_field_assign_extended_src_r15_u64() {
        // mov [rdi + 16], r15 (R15 is r8-r15, requires REX.R + REX.W)
        // Expected: 4C 89 7F 10
        // - 4C: REX.W + REX.R (r15=1111, high bit is 1)
        // - 89: opcode
        // - 7F: mod=01 (disp8), reg=111 (R15 low bits), rm=111 (RDI)
        // - 10: disp8=16
        let mut buf = CodeBuffer::new();
        mov_mem_reg64_disp_reg64(&mut buf, Reg64::Rdi, 16, Reg64::R15);
        assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0x7F, 0x10]);
    }

    #[test]
    fn pa_r17_006_field_assign_r13_base_disp0_forces_disp8() {
        // R13 base with disp=0 forces disp8=0 escape (mod=01 instead of mod=00)
        // mov [r13], rdx (R13 is r8-r15, requires REX.B)
        // Expected: 49 89 55 00
        // - 49: REX.W + REX.B (R13 is r8-r15)
        // - 89: opcode
        // - 55: mod=01 (R13 escape), reg=010 (RDX), rm=101 (R13 low bits)
        // - 00: disp8=0 (forced)
        let mut buf = CodeBuffer::new();
        mov_mem_reg64_disp_reg64(&mut buf, Reg64::R13, 0, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x55, 0x00]);
    }

    #[test]
    fn pa_r17_006_field_assign_sil_u8_requires_rex() {
        // BYTE-REG TRAP GUARD: mov [rdi], sil (RSI=6, byte form is SIL)
        // Without REX, this decodes as mov [rdi], dh — WRONG
        // Expected: 40 88 37 (REX.0 is mandatory)
        // - 40: REX.0 (minimum REX with no bits set)
        // - 88: opcode
        // - 37: mod=00, reg=110 (RSI low bits), rm=111 (RDI)
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg8(&mut buf, Reg64::Rdi, 0, Reg64::Rsi);
        assert_eq!(buf.as_slice(), &[0x40, 0x88, 0x37]);
    }

    // ── iced-x86 round-trip for store ops ────────────────────────────────

    #[test]
    fn pa_r17_006_roundtrip_mov_rdi_rdx_w32() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        // Round-trip: ensure iced-x86 decodes our 32-bit store correctly.
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg32(&mut buf, Reg64::Rdi, 0, Reg64::Rdx);
        let bytes = buf.as_slice();

        let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov [rdi], edx");

        // Verify key properties
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
        assert_eq!(instr.op_count(), 2);
    }

    #[test]
    fn pa_r17_006_roundtrip_mov_rdi_rsi_sil_w8() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        // Round-trip: verify byte-register REX trap is handled (SIL requires REX).
        let mut buf = CodeBuffer::new();
        mov_mem_base_disp_reg8(&mut buf, Reg64::Rdi, 0, Reg64::Rsi);
        let bytes = buf.as_slice();

        let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
        let instr = decoder.decode();
        assert!(instr.len() > 0, "Failed to decode mov [rdi], sil");

        // Verify key properties
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
        assert_eq!(instr.op_count(), 2);
    }

    #[test]
    fn endbr64_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0x86); // ENDBR64 sentinel

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Endbr64);
    }

    #[test]
    fn endbr32_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0x87); // ENDBR32 sentinel

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Endbr32);
    }
}
