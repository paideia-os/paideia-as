//! Scalar SSE float instructions (paideia-os #1333, paideia-as#1333):
//! movsd/movss, addsd/addss, subsd/subss, mulsd/mulss, divsd/divss,
//! sqrtsd/sqrtss, ucomisd/ucomiss, comisd/comiss, cvtsi2sd/cvtsi2ss,
//! cvttsd2si/cvttss2si, movd/movq bitcast. Register-register only.
//!
//! XMM registers use the compact RegId band 53-68 (XMM0-XMM15); GP
//! registers keep the usual 0-15 band.

use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};
use paideia_as_ir::{InstrMode, Instruction, Mnemonic, Operand, RegId};
use smallvec::{SmallVec, smallvec};

const XMM0: RegId = RegId(53);
const XMM1: RegId = RegId(54);
const XMM7: RegId = RegId(60);
const XMM8: RegId = RegId(61);
const RAX: RegId = RegId(0);
const RDI: RegId = RegId(7);

fn encode(mnemonic: Mnemonic, operands: &[Operand]) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic,
        operands: SmallVec::from_iter(operands.iter().cloned()),
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    buf.bytes
}

#[test]
fn movsd_xmm0_xmm1_emits_f2_0f_10_c1() {
    let bytes = encode(Mnemonic::MovSd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x10, 0xC1]);
}

#[test]
fn movss_xmm0_xmm1_emits_f3_0f_10_c1() {
    let bytes = encode(Mnemonic::MovSs, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x10, 0xC1]);
}

#[test]
fn addsd_xmm0_xmm1_emits_f2_0f_58_c1() {
    let bytes = encode(Mnemonic::AddSd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x58, 0xC1]);
}

#[test]
fn addss_xmm0_xmm1_emits_f3_0f_58_c1() {
    let bytes = encode(Mnemonic::AddSs, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x58, 0xC1]);
}

#[test]
fn subsd_xmm0_xmm1_emits_f2_0f_5c_c1() {
    let bytes = encode(Mnemonic::SubSd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x5C, 0xC1]);
}

#[test]
fn mulsd_xmm0_xmm1_emits_f2_0f_59_c1() {
    let bytes = encode(Mnemonic::MulSd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x59, 0xC1]);
}

#[test]
fn mulss_xmm0_xmm1_emits_f3_0f_59_c1() {
    let bytes = encode(Mnemonic::MulSs, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x59, 0xC1]);
}

#[test]
fn divsd_xmm0_xmm1_emits_f2_0f_5e_c1() {
    let bytes = encode(Mnemonic::DivSd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x5E, 0xC1]);
}

#[test]
fn sqrtsd_xmm0_xmm1_emits_f2_0f_51_c1() {
    let bytes = encode(Mnemonic::Sqrtsd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x51, 0xC1]);
}

#[test]
fn sqrtss_xmm0_xmm1_emits_f3_0f_51_c1() {
    let bytes = encode(Mnemonic::Sqrtss, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x51, 0xC1]);
}

#[test]
fn ucomisd_xmm0_xmm1_emits_66_0f_2e_c1() {
    let bytes = encode(Mnemonic::Ucomisd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0x66, 0x0F, 0x2E, 0xC1]);
}

#[test]
fn ucomiss_xmm0_xmm1_emits_0f_2e_c1_no_prefix() {
    let bytes = encode(Mnemonic::Ucomiss, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0x0F, 0x2E, 0xC1]);
}

#[test]
fn comisd_xmm0_xmm1_emits_66_0f_2f_c1() {
    let bytes = encode(Mnemonic::Comisd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0x66, 0x0F, 0x2F, 0xC1]);
}

#[test]
fn comiss_xmm0_xmm1_emits_0f_2f_c1_no_prefix() {
    let bytes = encode(Mnemonic::Comiss, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0x0F, 0x2F, 0xC1]);
}

#[test]
fn addsd_xmm8_xmm7_sets_rex_r_only() {
    // dst=xmm8 (high, REX.R), src=xmm7 (low): REX = 0100 0100 = 0x44.
    let bytes = encode(Mnemonic::AddSd, &[Operand::Reg(XMM8), Operand::Reg(XMM7)]);
    assert_eq!(bytes, vec![0xF2, 0x44, 0x0F, 0x58, 0xC7]);
}

#[test]
fn cvtsi2sd_xmm0_rax_emits_f2_rexw_0f_2a_c0() {
    let bytes = encode(Mnemonic::Cvtsi2sd, &[Operand::Reg(XMM0), Operand::Reg(RAX)]);
    assert_eq!(bytes, vec![0xF2, 0x48, 0x0F, 0x2A, 0xC0]);
}

#[test]
fn cvtsi2ss_xmm0_rdi_emits_f3_rexw_0f_2a_c7() {
    let bytes = encode(Mnemonic::Cvtsi2ss, &[Operand::Reg(XMM0), Operand::Reg(RDI)]);
    assert_eq!(bytes, vec![0xF3, 0x48, 0x0F, 0x2A, 0xC7]);
}

#[test]
fn cvttsd2si_rax_xmm1_emits_f2_rexw_0f_2c_c1() {
    let bytes = encode(Mnemonic::Cvttsd2si, &[Operand::Reg(RAX), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF2, 0x48, 0x0F, 0x2C, 0xC1]);
}

#[test]
fn cvttss2si_rax_xmm1_emits_f3_rexw_0f_2c_c1() {
    let bytes = encode(Mnemonic::Cvttss2si, &[Operand::Reg(RAX), Operand::Reg(XMM1)]);
    assert_eq!(bytes, vec![0xF3, 0x48, 0x0F, 0x2C, 0xC1]);
}

#[test]
fn movd_bitcast_load_xmm0_eax_emits_66_0f_6e_c0() {
    let bytes = encode(
        Mnemonic::MovdBitcast { to_xmm: true },
        &[Operand::Reg(XMM0), Operand::Reg(RAX)],
    );
    assert_eq!(bytes, vec![0x66, 0x0F, 0x6E, 0xC0]);
}

#[test]
fn movd_bitcast_store_eax_xmm0_emits_66_0f_7e_c0() {
    let bytes = encode(
        Mnemonic::MovdBitcast { to_xmm: false },
        &[Operand::Reg(RAX), Operand::Reg(XMM0)],
    );
    assert_eq!(bytes, vec![0x66, 0x0F, 0x7E, 0xC0]);
}

#[test]
fn movq_bitcast_load_xmm0_rax_emits_66_rexw_0f_6e_c0() {
    let bytes = encode(
        Mnemonic::MovqBitcast { to_xmm: true },
        &[Operand::Reg(XMM0), Operand::Reg(RAX)],
    );
    assert_eq!(bytes, vec![0x66, 0x48, 0x0F, 0x6E, 0xC0]);
}

#[test]
fn movq_bitcast_store_rax_xmm0_emits_66_rexw_0f_7e_c0() {
    let bytes = encode(
        Mnemonic::MovqBitcast { to_xmm: false },
        &[Operand::Reg(RAX), Operand::Reg(XMM0)],
    );
    assert_eq!(bytes, vec![0x66, 0x48, 0x0F, 0x7E, 0xC0]);
}

#[test]
fn wrong_operand_class_is_rejected() {
    // AddSd expects two XMM regs; a plain GPR operand must fail cleanly
    // rather than mis-encode.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::AddSd,
        operands: smallvec![Operand::Reg(XMM0), Operand::Reg(RAX)],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    assert!(encode_instruction(&inst, &mut buf, &mut stats).is_err());
}

#[test]
fn addsd_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode(Mnemonic::AddSd, &[Operand::Reg(XMM0), Operand::Reg(XMM1)]);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Addsd);
}

#[test]
fn cvtsi2sd_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode(Mnemonic::Cvtsi2sd, &[Operand::Reg(XMM0), Operand::Reg(RAX)]);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Cvtsi2sd);
}
