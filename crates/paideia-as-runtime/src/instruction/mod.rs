//! Per-IR-node instruction payload + side-table.
//!
//! This complements m1-006's LoadStoreSideTable: where Load/Store
//! handle the typed memory-access side, Instruction handles the
//! arbitrary x86_64 mnemonic + operand record that the m9 opt passes
//! need to consume to do real per-node rewrites (vs "would-fire"
//! markers).
//!
//! The module was split from a single 1,816-line file into cohesive
//! sub-modules (issue #1402, umbrella #1399). Every previously public
//! item is re-exported here so the public path
//! `paideia_as_runtime::instruction::…` is unchanged.

use smallvec::SmallVec;

mod cpu_feature;
mod mnemonic;
mod mnemonic_tables;
mod operand;
mod types;

pub use cpu_feature::CpuFeature;
pub use mnemonic::Mnemonic;
pub use operand::{Operand, RegId, Scale, SegPrefix, SegReg};
pub use types::{Cond, InstrMode, IntWidth};

/// Encoding hint that the encoder may consult.
///
/// Phase-3-m2-001 minimum: opcode + operand-size override. Future
/// PRs expand to REX/EVEX prefix planning, segment overrides, etc.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EncodingHint {
    /// Primary opcode (0x8B for MOV r64, r/m64; etc).
    pub opcode: u16,
    /// Operand size override: 1, 2, 4, or 8 bytes.
    pub operand_size: u8,
}

/// An instruction payload: the rich record m9 opt passes consume.
///
/// Carries the mnemonic, operands, and optional encoding hint for
/// per-node instruction rewriting passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// Mnemonic (Mov, Add, Jcc, etc).
    pub mnemonic: Mnemonic,
    /// Operands (typically 0–3; SmallVec avoids heap for common cases).
    pub operands: SmallVec<[Operand; 3]>,
    /// Optional encoding hint for the encoder.
    pub encoding_hint: Option<EncodingHint>,
    /// Byte offset in .text section where this instruction was emitted.
    /// Populated during encoding pass (phase-7-m1-003). Used to compute
    /// relocation offsets precisely, avoiding off-by-one errors that occur
    /// when encoder reads buf.bytes.len() after encoding.
    pub byte_offset_in_text: Option<u32>,
    /// Instruction mode (Mode64 or Mode32).
    pub mode: InstrMode,
    /// Monotonic emission order assigned during emit walk.
    /// Used as primary sort key in text section emission to break virtual-IrNodeId collisions.
    /// 0 is reserved sentinel for test-only instructions; production code always assigns > 0.
    pub emission_order: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mnemonic + Cond tests ────────────────────────────────────────

    #[test]
    fn mnemonic_jcc_with_eq_constructs_cleanly() {
        let mnem = Mnemonic::Jcc(Cond::Eq);
        assert_eq!(mnem, Mnemonic::Jcc(Cond::Eq));
    }

    #[test]
    fn cond_variants_count() {
        // Verify Cond has 16 variants (sanity check).
        let variants = [
            Cond::Eq,
            Cond::Ne,
            Cond::Lt,
            Cond::Le,
            Cond::Gt,
            Cond::Ge,
            Cond::Below,
            Cond::BelowOrEqual,
            Cond::Above,
            Cond::AboveOrEqual,
            Cond::Zero,
            Cond::NonZero,
            Cond::Sign,
            Cond::NotSign,
            Cond::Overflow,
            Cond::NotOverflow,
        ];
        assert_eq!(variants.len(), 16);
    }

    // ── Operand tests ───────────────────────────────────────────────

    #[test]
    fn operand_reg_roundtrips_through_clone() {
        let op1 = Operand::Reg(RegId(5));
        let op2 = op1.clone();
        assert_eq!(op1, op2);
    }

    #[test]
    fn operand_mem_sib_constructs_with_optional_index() {
        let op_with_index = Operand::MemSib {
            base: RegId(0),
            index: Some(RegId(1)),
            scale: Scale::X4,
            disp: 8,
        };
        let op_without_index = Operand::MemSib {
            base: RegId(0),
            index: None,
            scale: Scale::X1,
            disp: 0,
        };
        assert_eq!(op_with_index, op_with_index);
        assert_eq!(op_without_index, op_without_index);
        assert_ne!(op_with_index, op_without_index);
    }

    // ── Scale tests ─────────────────────────────────────────────────

    #[test]
    fn scale_factor_returns_expected() {
        assert_eq!(Scale::X1.factor(), 1);
        assert_eq!(Scale::X2.factor(), 2);
        assert_eq!(Scale::X4.factor(), 4);
        assert_eq!(Scale::X8.factor(), 8);
    }

    #[test]
    fn scale_from_factor_handles_canonical_values() {
        assert_eq!(Scale::from_factor(1), Some(Scale::X1));
        assert_eq!(Scale::from_factor(2), Some(Scale::X2));
        assert_eq!(Scale::from_factor(4), Some(Scale::X4));
        assert_eq!(Scale::from_factor(8), Some(Scale::X8));
    }

    #[test]
    fn scale_from_factor_returns_none_for_invalid() {
        assert_eq!(Scale::from_factor(0), None);
        assert_eq!(Scale::from_factor(3), None);
        assert_eq!(Scale::from_factor(5), None);
        assert_eq!(Scale::from_factor(6), None);
        assert_eq!(Scale::from_factor(7), None);
        assert_eq!(Scale::from_factor(16), None);
    }

    // ── Instruction tests ───────────────────────────────────────────

    #[test]
    fn instruction_with_three_operands_uses_smallvec_inline() {
        let op1 = Operand::Reg(RegId(0));
        let op2 = Operand::Reg(RegId(1));
        let op3 = Operand::Imm64(42);

        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(op1.clone());
        operands.push(op2.clone());
        operands.push(op3.clone());

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: Some(EncodingHint {
                opcode: 0x8B,
                operand_size: 8,
            }),
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        };

        assert_eq!(inst.operands.len(), 3);
        assert_eq!(inst.operands[0], op1);
        assert_eq!(inst.operands[1], op2);
        assert_eq!(inst.operands[2], op3);
    }

    // ── Mnemonic size constraint ────────────────────────────────────────

    #[test]
    fn mnemonic_size_fits_in_four_bytes() {
        use core::mem::size_of;
        // Mnemonic includes Jcc(Cond) (1 byte tag + 1 byte data) and
        // MovCr/MovDr/In/Out with bool or u8 payloads. Max size is 4 bytes.
        assert!(size_of::<Mnemonic>() <= 4);
    }

    // ── Mnemonic::estimated_size tests ──────────────────────────────────

    #[test]
    fn estimated_size_nop_is_one_byte() {
        let ops = [];
        assert_eq!(Mnemonic::Nop.estimated_size(&ops), 1);
    }

    #[test]
    fn estimated_size_mov_reg_reg_is_ten_bytes() {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(0))); // rax
        ops.push(Operand::Reg(RegId(1))); // rcx
        assert_eq!(Mnemonic::Mov.estimated_size(&ops), 10);
    }

    #[test]
    fn estimated_size_mov_reg_imm_is_ten_bytes() {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(0))); // rax
        ops.push(Operand::Imm64(0x80));
        assert_eq!(Mnemonic::Mov.estimated_size(&ops), 10);
    }

    #[test]
    fn estimated_size_jcc_is_six_bytes() {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Imm64(0));
        assert_eq!(Mnemonic::Jcc(Cond::Eq).estimated_size(&ops), 6);
    }

    #[test]
    fn estimated_size_jmp_is_five_bytes() {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Imm64(0));
        assert_eq!(Mnemonic::Jmp.estimated_size(&ops), 5);
    }

    // ── Implicit reads/writes tests (PA-r16-004-backtrack-b, #1034) ──────

    #[test]
    fn implicit_reads_lock_cmpxchg_returns_rax() {
        let reads = Mnemonic::LockCmpxchg.implicit_reads();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0], RegId(0)); // RAX
    }

    #[test]
    fn implicit_writes_lock_cmpxchg_returns_rax() {
        let writes = Mnemonic::LockCmpxchg.implicit_writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0], RegId(0)); // RAX
    }

    #[test]
    fn implicit_reads_lock_cmpxchg32_returns_rax() {
        let reads = Mnemonic::LockCmpxchg32.implicit_reads();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0], RegId(0)); // RAX
    }

    #[test]
    fn implicit_writes_lock_cmpxchg32_returns_rax() {
        let writes = Mnemonic::LockCmpxchg32.implicit_writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0], RegId(0)); // RAX
    }

    #[test]
    fn implicit_reads_lock_cmpxchg16b_returns_rax_rdx_rbx_rcx() {
        let reads = Mnemonic::LockCmpxchg16b.implicit_reads();
        assert_eq!(reads.len(), 4);
        assert_eq!(reads[0], RegId(0)); // RAX
        assert_eq!(reads[1], RegId(2)); // RDX
        assert_eq!(reads[2], RegId(3)); // RBX
        assert_eq!(reads[3], RegId(1)); // RCX
    }

    #[test]
    fn implicit_writes_lock_cmpxchg16b_returns_rax_rdx() {
        let writes = Mnemonic::LockCmpxchg16b.implicit_writes();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0], RegId(0)); // RAX
        assert_eq!(writes[1], RegId(2)); // RDX
    }

    #[test]
    fn implicit_reads_lock_add_is_empty() {
        let reads = Mnemonic::LockAdd { width: IntWidth::W64 }.implicit_reads();
        assert!(reads.is_empty());
    }

    #[test]
    fn implicit_writes_lock_add_is_empty() {
        let writes = Mnemonic::LockAdd { width: IntWidth::W64 }.implicit_writes();
        assert!(writes.is_empty());
    }

    #[test]
    fn implicit_reads_lock_xadd_is_empty() {
        let reads = Mnemonic::LockXadd { width: IntWidth::W64 }.implicit_reads();
        assert!(reads.is_empty());
    }

    #[test]
    fn implicit_writes_lock_xadd_is_empty() {
        let writes = Mnemonic::LockXadd { width: IntWidth::W64 }.implicit_writes();
        assert!(writes.is_empty());
    }

    // ── IntWidth / MovSized tests (Phase 7 m4-003) ──────────────────────

    #[test]
    fn int_width_from_bits_maps_canonical_widths() {
        assert_eq!(IntWidth::from_bits(8), Some(IntWidth::W8));
        assert_eq!(IntWidth::from_bits(16), Some(IntWidth::W16));
        assert_eq!(IntWidth::from_bits(32), Some(IntWidth::W32));
        assert_eq!(IntWidth::from_bits(64), Some(IntWidth::W64));
    }

    #[test]
    fn int_width_from_bits_rejects_non_canonical() {
        assert_eq!(IntWidth::from_bits(1), None);
        assert_eq!(IntWidth::from_bits(24), None);
        assert_eq!(IntWidth::from_bits(128), None);
    }

    #[test]
    fn int_width_estimated_sizes() {
        assert_eq!(IntWidth::W8.estimated_size(), 3);
        assert_eq!(IntWidth::W16.estimated_size(), 4);
        assert_eq!(IntWidth::W32.estimated_size(), 5);
        assert_eq!(IntWidth::W64.estimated_size(), 10);
    }

    #[test]
    fn mov_sized_arity_is_two() {
        assert_eq!(
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
            .arity(),
            2
        );
    }

    #[test]
    fn mov_sized_estimated_size_tracks_width() {
        let ops = [Operand::Reg(RegId(0)), Operand::Imm64(42)];
        assert_eq!(
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
            .estimated_size(&ops),
            5
        );
        assert_eq!(
            Mnemonic::MovSized {
                width: IntWidth::W8
            }
            .estimated_size(&ops),
            3
        );
    }

    #[test]
    fn estimated_size_call_is_five_bytes() {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Imm64(0));
        assert_eq!(Mnemonic::Call.estimated_size(&ops), 5);
    }
}
