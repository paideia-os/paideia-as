//! R51-paideia-as-001 (#1315): MSI-X 64-bit atomic MMIO write encoder verification.
//!
//! The R51 NVMe/AHCI drivers program each MSI-X table entry as a 128-bit
//! quantity assembled from qword stores (message-address-low+high in the low
//! 8 bytes, message-data + vector_control in the high 8 bytes). PCIe requires
//! each 64-bit `mov qword ptr [<mmio>], <reg>` to be atomic w.r.t. the mask
//! bit — the CPU must issue a SINGLE 8-byte MMIO transaction, never two
//! 4-byte transactions, or a device may observe a torn write and latch a
//! half-updated vector while the mask bit is being written.
//!
//! These tests pin the encoder output for the byte-exact MSI-X programming
//! patterns and cross-check with iced-x86 that decoding produces exactly one
//! `mov r/m64, r64` (opcode 0x89 under REX.W) — never two `mov r/m32, r32`.
//!
//! See paideia-os design/hardware/nvme-and-disk-substrate.md §8.1.

use iced_x86::{Decoder, DecoderOptions, Instruction as IcedInst, Mnemonic as IcedMnem, OpKind, Register};
use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ── Byte-exact: base+disp, RAX → [RDI + 0x100] ────────────────────────────────
//
// This is the canonical MSI-X programming form: RDI holds the MMIO BAR base,
// RAX holds the 64-bit payload, and 0x100 offsets into the vector table. A
// single `48 89 87 00 01 00 00` (REX.W + MOV r/m64, r64 + ModR/M + disp32) is
// the ONLY correct emission. Two 4-byte stores (89 87 … 89 87 …) would tear
// the vector_control update.
#[test]
fn mov_q_rdi_disp_0x100_rax_single_8byte_store_1315() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0x100 },
            Operand::Reg(RegId(0))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rdi + 0x100], rax");

    // Exact bytes: REX.W (0x48), opcode 0x89 (MOV r/m64, r64), ModR/M 0x87
    // (mod=10 disp32, reg=000 rax, rm=111 rdi), disp32 LE = 00 01 00 00.
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x89, 0x87, 0x00, 0x01, 0x00, 0x00],
        "mov_q [rdi+0x100], rax must emit a single REX.W 89 store; \
         torn stores would appear as two 4-byte forms (no 0x48, opcode 0x89 twice)"
    );

    // Explicit anti-tearing invariants:
    // (a) exactly 7 bytes — a torn pair would emit ≥ 8 bytes (two 0x89 groups).
    assert_eq!(buf.as_slice().len(), 7, "encoding must be a single instruction");
    // (b) REX.W must be present — its absence would prove the emitter narrowed to 32-bit.
    assert_eq!(buf.as_slice()[0], 0x48, "REX.W required for 64-bit MMIO store");
    // (c) opcode 0x89 appears exactly once — a torn pair would repeat it.
    let opcode_count = buf.as_slice().iter().filter(|&&b| b == 0x89).count();
    assert_eq!(opcode_count, 1, "MOV r/m64, r64 opcode must appear exactly once");
}

// ── Byte-exact: base+disp8, RAX → [RDI + 8] ───────────────────────────────────
//
// Small displacement covers the message-data high half of an MSI-X entry
// (offset +8 from the entry base). Encoded as REX.W + 89 + ModR/M + disp8.
#[test]
fn mov_q_rdi_disp_8_rax_single_8byte_store_1315() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(0))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rdi + 8], rax");

    // 48 89 47 08 — REX.W, MOV r/m64 r64, ModR/M mod=01 disp8, disp8=0x08.
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x47, 0x08]);
    assert_eq!(buf.as_slice().len(), 4);
    assert_eq!(buf.as_slice()[0], 0x48, "REX.W required");
    assert_eq!(buf.as_slice().iter().filter(|&&b| b == 0x89).count(), 1);
}

// ── Byte-exact: SIB-indexed, RAX → [RDI + RCX*8 + 0x100] ──────────────────────
//
// MSI-X vector-index addressing: RCX is the vector index, scaled by 8 to walk
// qword-sized fields. Still a single REX.W 89 store — the SIB form must not
// introduce a second transaction.
#[test]
fn mov_q_rdi_rcx_8_disp_0x100_rax_single_8byte_store_1315() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: Some(RegId(1)), scale: Scale::X8, disp: 0x100 },
            Operand::Reg(RegId(0))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rdi + rcx*8 + 0x100], rax");

    // 48 89 84 CF 00 01 00 00 — REX.W, MOV r/m64 r64, ModR/M mod=10 rm=100 (SIB),
    // SIB scale=11 index=001 base=111, disp32 LE = 00 01 00 00.
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x84, 0xCF, 0x00, 0x01, 0x00, 0x00]);
    assert_eq!(buf.as_slice().len(), 8);
    assert_eq!(buf.as_slice()[0], 0x48, "REX.W required");
    assert_eq!(buf.as_slice().iter().filter(|&&b| b == 0x89).count(), 1);
}

// ── Byte-exact: extended-reg source, R11 → [R12 + 0x40] ───────────────────────
//
// Extended registers force REX.R + REX.B; the store must still be a single
// 8-byte transaction. Covers the driver hot path where the payload is staged
// in a callee-saved extended register.
#[test]
fn mov_q_r12_disp_0x40_r11_single_8byte_store_1315() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: None, scale: Scale::X1, disp: 0x40 },
            Operand::Reg(RegId(11))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [r12 + 0x40], r11");

    // R12 as base triggers the SIB escape (base low = 4 = rsp/r12).
    // Bytes: 4D (REX.W+R+B) 89 (mov r/m64,r64) 5C (mod=01 reg=011 rm=100 SIB)
    //        24 (SIB scale=00 index=100 base=100) 40 (disp8).
    assert_eq!(buf.as_slice(), &[0x4D, 0x89, 0x5C, 0x24, 0x40]);
    assert_eq!(buf.as_slice().len(), 5);
    // REX prefix present in 0x40..=0x4F, REX.W bit (0x08) set.
    assert_eq!(buf.as_slice()[0] & 0xF0, 0x40, "REX prefix required");
    assert!(buf.as_slice()[0] & 0x08 != 0, "REX.W bit must be set for 64-bit MMIO store");
    assert_eq!(buf.as_slice().iter().filter(|&&b| b == 0x89).count(), 1);
}

// ── iced-x86 cross-check: one MOV instruction, 8-byte memory size ─────────────
//
// Round-trip against iced-x86 to confirm the encoded bytes decode as exactly
// one `MOV [mem64], r64` — an atomic 8-byte transaction — and that no further
// bytes remain that could be interpreted as a second store.
#[test]
fn mov_q_rdi_disp_0x100_rax_iced_single_qword_1315() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0x100 },
            Operand::Reg(RegId(0))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();

    let bytes = buf.as_slice().to_vec();
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded: IcedInst = decoder.decode();

    // Single MOV, memory destination, register source.
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
    assert_eq!(decoded.op0_kind(), OpKind::Memory);
    assert_eq!(decoded.op1_kind(), OpKind::Register);
    assert_eq!(decoded.op1_register(), Register::RAX,
        "source must be the full 64-bit RAX, not EAX (a torn store would show EAX)");
    assert_eq!(decoded.memory_base(), Register::RDI);
    // The whole encoding is consumed — no trailing bytes for a second store.
    assert_eq!(decoded.len(), bytes.len(),
        "encoder must emit exactly one instruction (no torn second 4-byte store follows)");

    // And the raw opcode byte after REX is 0x89 (MOV r/m64, r64), not two 0x89
    // separated by a second REX-less prefix — an explicit anti-tearing pin.
    assert_eq!(bytes[0], 0x48, "REX.W");
    assert_eq!(bytes[1], 0x89, "MOV r/m64, r64 opcode");
}
