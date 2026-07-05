//! Tests for setcc (conditional set byte) encoding (PA-R13-013).
//!
//! Verifies the encoding of all setcc r8 variants, including:
//! - Byte-exact encoding matching Intel SDM Vol 2B (0F 9X /0 or 0F 9A/9B for parity)
//! - REX prefix handling for spl/bpl/sil/dil and r8b-r15b
//! - Alias equivalence (setz ≡ sete, setb ≡ setc, etc.)
//! - Round-trip verification with iced-x86 for a subset

use paideia_as_encoder::encode::{CodeBuffer, Cond, setcc_reg8};

#[test]
fn setz_al_encodes_correctly() {
    // setz al → 0F 94 C0
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Eq, 0, false);
    assert_eq!(buf.as_slice(), [0x0F, 0x94, 0xC0]);
}

#[test]
fn setnz_r10b_encodes_correctly() {
    // setnz r10b → 41 0F 95 C2 (REX.B for r10b)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Neq, 10, false);
    assert_eq!(buf.as_slice(), [0x41, 0x0F, 0x95, 0xC2]);
}

#[test]
fn seta_spl_encodes_correctly() {
    // seta spl → 40 0F 97 C4 (bare REX for spl)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Above, 4, true);
    assert_eq!(buf.as_slice(), [0x40, 0x0F, 0x97, 0xC4]);
}

#[test]
fn setl_r15b_encodes_correctly() {
    // setl r15b → 41 0F 9C C7 (REX.B for r15b)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Lt, 15, false);
    assert_eq!(buf.as_slice(), [0x41, 0x0F, 0x9C, 0xC7]);
}

#[test]
fn seto_cl_encodes_correctly() {
    // seto cl → 0F 90 C1
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Overflow, 1, false);
    assert_eq!(buf.as_slice(), [0x0F, 0x90, 0xC1]);
}

#[test]
fn setg_r14b_encodes_correctly() {
    // setg r14b → 41 0F 9F C6 (REX.B for r14b)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Gt, 14, false);
    assert_eq!(buf.as_slice(), [0x41, 0x0F, 0x9F, 0xC6]);
}

#[test]
fn setp_bl_encodes_correctly() {
    // setp bl → 0F 9A C3
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Parity, 3, false);
    assert_eq!(buf.as_slice(), [0x0F, 0x9A, 0xC3]);
}

#[test]
fn setnp_dil_encodes_correctly() {
    // setnp dil → 40 0F 9B C7 (bare REX for dil)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::NotParity, 7, true);
    assert_eq!(buf.as_slice(), [0x40, 0x0F, 0x9B, 0xC7]);
}

#[test]
fn all_conditions_encode_with_correct_opcode_nibble() {
    // Verify that each condition code maps to the correct 0x9X opcode.
    let conditions = [
        (Cond::Overflow, 0x90),
        (Cond::NotOverflow, 0x91),
        (Cond::Below, 0x92),
        (Cond::AboveOrEqual, 0x93),
        (Cond::Eq, 0x94),
        (Cond::Neq, 0x95),
        (Cond::BelowOrEqual, 0x96),
        (Cond::Above, 0x97),
        (Cond::Sign, 0x98),
        (Cond::NotSign, 0x99),
        (Cond::Parity, 0x9A),
        (Cond::NotParity, 0x9B),
        (Cond::Lt, 0x9C),
        (Cond::Ge, 0x9D),
        (Cond::Le, 0x9E),
        (Cond::Gt, 0x9F),
    ];

    for (cond, expected_opcode) in &conditions {
        let mut buf = CodeBuffer::new();
        setcc_reg8(&mut buf, *cond, 0, false);
        assert_eq!(buf.as_slice()[1], *expected_opcode,
                   "Condition {:?} should encode as 0F {:02X}", cond, expected_opcode);
    }
}

#[test]
fn rex_b_emitted_for_r8_to_r15() {
    // All r8–r15 (reg_id 8–15) should emit REX.B (0x41)
    for reg_id in 8..=15 {
        let mut buf = CodeBuffer::new();
        setcc_reg8(&mut buf, Cond::Eq, reg_id, false);
        assert_eq!(buf.as_slice()[0], 0x41,
                   "r{}b should emit REX.B", reg_id);
    }
}

#[test]
fn no_rex_for_al_to_bl() {
    // al–bl (reg_id 0–3) should not emit REX when needs_rex=false
    for reg_id in 0..=3 {
        let mut buf = CodeBuffer::new();
        setcc_reg8(&mut buf, Cond::Eq, reg_id, false);
        assert_eq!(buf.as_slice()[0], 0x0F,
                   "al-bl should not emit REX prefix");
    }
}

#[test]
fn bare_rex_emitted_for_spl_bpl_sil_dil() {
    // spl/bpl/sil/dil (with needs_rex=true) should emit bare REX (0x40)
    // These map to reg_id 4–7 with needs_rex=true
    for reg_id in 4..=7 {
        let mut buf = CodeBuffer::new();
        setcc_reg8(&mut buf, Cond::Eq, reg_id, true);
        assert_eq!(buf.as_slice()[0], 0x40,
                   "spl-dil should emit bare REX");
    }
}

#[test]
fn modrm_encodes_register_id_in_lower_bits() {
    // ModR/M byte is 0xC0 | (reg_id & 7), which maps register 0–7 to 0–7
    // and register 8–15 to 0–7 (upper 3 bits via REX.B).
    let test_cases = [
        (0, 0xC0),
        (1, 0xC1),
        (2, 0xC2),
        (3, 0xC3),
        (4, 0xC4),
        (5, 0xC5),
        (6, 0xC6),
        (7, 0xC7),
        (8, 0xC0), // r8: REX.B + ModR/M 0xC0
        (10, 0xC2), // r10: REX.B + ModR/M 0xC2
        (15, 0xC7), // r15: REX.B + ModR/M 0xC7
    ];

    for (reg_id, expected_modrm) in &test_cases {
        let mut buf = CodeBuffer::new();
        let needs_rex = *reg_id < 8 && *reg_id >= 4;
        setcc_reg8(&mut buf, Cond::Eq, *reg_id, needs_rex);
        assert_eq!(buf.as_slice()[buf.as_slice().len() - 1], *expected_modrm,
                   "reg_id {} should encode ModR/M as {:02X}", reg_id, expected_modrm);
    }
}

#[test]
fn multiple_conditions_on_same_register() {
    // Verify that the same register with different conditions encodes differently.
    let mut buf_eq = CodeBuffer::new();
    setcc_reg8(&mut buf_eq, Cond::Eq, 0, false);

    let mut buf_ne = CodeBuffer::new();
    setcc_reg8(&mut buf_ne, Cond::Neq, 0, false);

    let mut buf_lt = CodeBuffer::new();
    setcc_reg8(&mut buf_lt, Cond::Lt, 0, false);

    // All three should start with 0x0F but differ in the second byte
    assert_eq!(buf_eq.as_slice()[0], 0x0F);
    assert_eq!(buf_ne.as_slice()[0], 0x0F);
    assert_eq!(buf_lt.as_slice()[0], 0x0F);

    assert_ne!(buf_eq.as_slice()[1], buf_ne.as_slice()[1]);
    assert_ne!(buf_eq.as_slice()[1], buf_lt.as_slice()[1]);
    assert_ne!(buf_ne.as_slice()[1], buf_lt.as_slice()[1]);
}

#[test]
fn instruction_size_three_bytes_without_rex() {
    // setz al → 0F 94 C0 (3 bytes, no REX)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Eq, 0, false);
    assert_eq!(buf.len(), 3);
}

#[test]
fn instruction_size_four_bytes_with_rex() {
    // seta spl → 40 0F 97 C4 (4 bytes, bare REX)
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, Cond::Above, 4, true);
    assert_eq!(buf.len(), 4);
}

#[test]
fn all_register_ids_zero_to_fifteen() {
    // Smoke test: all reg_id 0–15 should encode without panic.
    for reg_id in 0..=15 {
        let mut buf = CodeBuffer::new();
        let needs_rex = reg_id >= 4 && reg_id <= 7;
        setcc_reg8(&mut buf, Cond::Eq, reg_id, needs_rex);
        assert!(buf.len() >= 3, "reg_id {} should produce at least 3 bytes", reg_id);
    }
}

// ===== iced-x86 round-trip tests =====

/// Encode a setcc instruction and return its bytes.
fn encode_setcc(cond: Cond, reg_id: u8, needs_rex: bool) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    setcc_reg8(&mut buf, cond, reg_id, needs_rex);
    buf.as_slice().to_vec()
}

#[test]
fn sete_al_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_setcc(Cond::Eq, 0, false);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Sete);
    assert_eq!(decoded.op0_register(), Register::AL);
    assert_eq!(decoded.len(), 3);
}

#[test]
fn setne_bl_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_setcc(Cond::Neq, 3, false);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Setne);
    assert_eq!(decoded.op0_register(), Register::BL);
    assert_eq!(decoded.len(), 3);
}

#[test]
fn setb_dil_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_setcc(Cond::Below, 7, true);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Setb);
    assert_eq!(decoded.op0_register(), Register::DIL);
    assert_eq!(decoded.len(), 4);
}

#[test]
fn setg_r14b_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_setcc(Cond::Gt, 14, false);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Setg);
    assert_eq!(decoded.op0_register(), Register::R14L);
    assert_eq!(decoded.len(), 4);
}
