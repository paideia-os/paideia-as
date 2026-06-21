//! Per-IR-node instruction payload + side-table.
//!
//! This complements m1-006's LoadStoreSideTable: where Load/Store
//! handle the typed memory-access side, Instruction handles the
//! arbitrary x86_64 mnemonic + operand record that the m9 opt passes
//! need to consume to do real per-node rewrites (vs "would-fire"
//! markers).

use crate::node::IrNodeId;
use smallvec::SmallVec;
use std::collections::HashMap;

/// x86_64 mnemonics targeted by the m9 opt-pass catalog.
///
/// Phase-3-m2-001 minimum: the 10-mnemonic catalog the m9 passes
/// reference. Phase-5-m2-001 extension: 20 privileged + system-ISA mnemonics.
/// Wider coverage (full SDM subset) ships in a future PR.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Mnemonic {
    /// Move (register to register, register to memory, memory to register, immediate to register).
    Mov,
    /// Integer addition.
    Add,
    /// Integer subtraction.
    Sub,
    /// Compare (compute difference without storing).
    Cmp,
    /// Jcc with embedded condition code.
    Jcc(Cond),
    /// Unconditional jump.
    Jmp,
    /// Call (push return address and jump).
    Call,
    /// Return (pop return address and jump).
    Ret,
    /// REP-prefixed string MOVSB (the canonical bulk-copy primitive).
    RepMovsb,
    /// Load effective address.
    Lea,
    /// Load global descriptor table register.
    Lgdt,
    /// Load interrupt descriptor table register.
    Lidt,
    /// Move to/from control register (write indicates direction).
    MovCr {
        /// True for MOV-to-CR (write), false for MOV-from-CR (read).
        write: bool,
    },
    /// Move to/from debug register (write indicates direction).
    MovDr {
        /// True for MOV-to-DR (write), false for MOV-from-DR (read).
        write: bool,
    },
    /// Write to model-specific register.
    Wrmsr,
    /// Read from model-specific register.
    Rdmsr,
    /// Read from I/O port (width in bytes: 1, 2, or 4).
    In {
        /// Width of the I/O read: 1, 2, or 4 bytes.
        width: u8,
    },
    /// Write to I/O port (width in bytes: 1, 2, or 4).
    Out {
        /// Width of the I/O write: 1, 2, or 4 bytes.
        width: u8,
    },
    /// Interrupt return (32-bit).
    Iret,
    /// Interrupt return (64-bit).
    Iretq,
    /// System return from fast syscall.
    Sysret,
    /// System call to kernel (x86_64 syscall instruction).
    Syscall,
    /// Swap GS base register.
    Swapgs,
    /// CPU identification.
    Cpuid,
    /// Clear interrupt flag.
    Cli,
    /// Set interrupt flag.
    Sti,
    /// Halt processor.
    Hlt,
    /// Software interrupt.
    Int,
    /// No operation.
    Nop,
    /// REP-prefixed STOSQ (store to memory via RCX iterations).
    RepStosq,
    /// Far jump (intersegment).
    FarJmp,
    /// Move with zero-extend: zero-extend smaller operand to larger width.
    /// Phase 6 m3-002: used for u8 field access; emits movzx rax, byte [rdi + offset].
    Movzx,
}

/// Condition code for Jcc instructions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Cond {
    /// Equal (je).
    Eq,
    /// Not equal (jne).
    Ne,
    /// Signed less than (jl).
    Lt,
    /// Signed less than or equal (jle).
    Le,
    /// Signed greater than (jg).
    Gt,
    /// Signed greater than or equal (jge).
    Ge,
    /// Unsigned less than (jb).
    Below,
    /// Unsigned less than or equal (jbe).
    BelowOrEqual,
    /// Unsigned greater than (ja).
    Above,
    /// Unsigned greater than or equal (jae).
    AboveOrEqual,
    /// Zero (jz).
    Zero,
    /// Not zero (jnz).
    NonZero,
    /// Sign flag set (js).
    Sign,
    /// Sign flag not set (jns).
    NotSign,
    /// Overflow flag set (jo).
    Overflow,
    /// Overflow flag not set (jno).
    NotOverflow,
}

/// An operand to an x86_64 instruction.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Operand {
    /// Register operand.
    Reg(RegId),
    /// 64-bit immediate operand.
    Imm64(i64),
    /// SIB-form memory: base + index * scale + disp.
    MemSib {
        /// Base register.
        base: RegId,
        /// Optional index register.
        index: Option<RegId>,
        /// Scale factor for index.
        scale: Scale,
        /// Displacement offset.
        disp: i32,
    },
    /// Pure displacement memory (no base/index).
    MemDisp {
        /// Displacement offset.
        disp: i32,
    },
    /// RIP-relative memory: [rip + disp32].
    MemRipRel {
        /// 32-bit displacement (sign-extended).
        disp: i32,
    },
    /// Unresolved symbol reference with optional addend.
    /// Used during assembly for symbols that are resolved at link time.
    SymbolRef {
        /// Name of the symbol.
        name: String,
        /// Addend to apply to the symbol address.
        addend: i32,
    },
    /// Label reference: a forward or backward reference to a label within the unsafe block.
    /// Phase 6 m4-002: used by Jcc/Jmp instructions. The encoder emits a zero displacement
    /// placeholder and records the fixup in EncodeOutput.label_fixups for later resolution.
    /// Duplicate labels → U1609; unknown labels → U1610.
    LabelRef {
        /// Name of the label.
        name: String,
        /// Addend to apply to the label address (typically 0).
        addend: i32,
    },
}

/// x86_64 register identifier.
///
/// Valid range: 0..15 for RAX..R15. Encoder side handles the actual
/// register-encoding lookup table.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RegId(pub u8);

/// Scale factor for SIB addressing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Scale {
    /// Scale by 1x.
    X1,
    /// Scale by 2x.
    X2,
    /// Scale by 4x.
    X4,
    /// Scale by 8x.
    X8,
}

impl Scale {
    /// Convert scale to numeric factor.
    #[must_use]
    pub fn factor(self) -> u32 {
        match self {
            Scale::X1 => 1,
            Scale::X2 => 2,
            Scale::X4 => 4,
            Scale::X8 => 8,
        }
    }

    /// Construct a Scale from a numeric factor.
    ///
    /// Returns `None` if the factor is not a valid scale (1, 2, 4, or 8).
    #[must_use]
    pub fn from_factor(f: u32) -> Option<Self> {
        match f {
            1 => Some(Scale::X1),
            2 => Some(Scale::X2),
            4 => Some(Scale::X4),
            8 => Some(Scale::X8),
            _ => None,
        }
    }
}

impl Mnemonic {
    /// Return the expected operand count (arity) for this mnemonic.
    ///
    /// Zero-arity mnemonics (cli, sti, hlt, nop, swapgs, cpuid, wrmsr, rdmsr,
    /// iret, iretq, sysret, rep_stosq) take no operands. UnsafeWalker uses this
    /// to skip operand-parsing and emit U1607 if the source has operands.
    #[must_use]
    pub fn arity(self) -> u8 {
        match self {
            // Zero-arity instructions (Phase 6 m1-005)
            Mnemonic::Cli
            | Mnemonic::Sti
            | Mnemonic::Hlt
            | Mnemonic::Nop
            | Mnemonic::Swapgs
            | Mnemonic::Cpuid
            | Mnemonic::Wrmsr
            | Mnemonic::Rdmsr
            | Mnemonic::Iret
            | Mnemonic::Iretq
            | Mnemonic::Sysret
            | Mnemonic::Syscall
            | Mnemonic::RepStosq => 0,

            // One-operand instructions
            Mnemonic::Call
            | Mnemonic::Ret
            | Mnemonic::Jmp
            | Mnemonic::Jcc(_)
            | Mnemonic::RepMovsb
            | Mnemonic::Lgdt
            | Mnemonic::Lidt
            | Mnemonic::MovCr { .. }
            | Mnemonic::MovDr { .. }
            | Mnemonic::In { .. }
            | Mnemonic::Out { .. }
            | Mnemonic::Int
            | Mnemonic::FarJmp => 1,

            // Two-operand instructions
            Mnemonic::Mov
            | Mnemonic::Add
            | Mnemonic::Sub
            | Mnemonic::Cmp
            | Mnemonic::Lea
            | Mnemonic::Movzx => 2,
        }
    }
}

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
}

/// Side-table mapping IrNodeId → Instruction payload.
///
/// Pattern: m3-007 HandlerSideTable / m1-006 LoadStoreSideTable.
/// Keeps IrNodeData ≤ 48 bytes (const_assert pinned).
#[derive(Default, Debug, Clone)]
pub struct InstructionSideTable {
    /// Sparse mapping: instruction node id -> Instruction.
    entries: HashMap<IrNodeId, Instruction>,
}

impl InstructionSideTable {
    /// Construct an empty instruction side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) an instruction payload.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, inst: Instruction) -> Option<Instruction> {
        self.entries.insert(id, inst)
    }

    /// Look up an instruction payload.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&Instruction> {
        self.entries.get(&id)
    }

    /// Look up (mutable) an instruction payload.
    ///
    /// Allows phases to mutate the payload (operands, hints) in place.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut Instruction> {
        self.entries.get_mut(&id)
    }

    /// Number of instructions registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no instructions are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove an instruction entry.
    ///
    /// Returns the payload if one existed.
    pub fn remove(&mut self, id: IrNodeId) -> Option<Instruction> {
        self.entries.remove(&id)
    }

    /// Borrow the underlying HashMap (read-only).
    #[must_use]
    pub fn entries(&self) -> &std::collections::HashMap<IrNodeId, Instruction> {
        &self.entries
    }
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
        };

        assert_eq!(inst.operands.len(), 3);
        assert_eq!(inst.operands[0], op1);
        assert_eq!(inst.operands[1], op2);
        assert_eq!(inst.operands[2], op3);
    }

    // ── InstructionSideTable tests ──────────────────────────────────

    #[test]
    fn instruction_side_table_insert_and_get() {
        let mut table = InstructionSideTable::new();
        let inst_id = IrNodeId::new(1).unwrap();

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(RegId(0)));
                ops.push(Operand::Reg(RegId(1)));
                ops
            },
            encoding_hint: Some(EncodingHint {
                opcode: 0x8B,
                operand_size: 8,
            }),
        };

        table.insert(inst_id, inst.clone());
        let retrieved = table.get(inst_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().mnemonic, Mnemonic::Mov);
        assert_eq!(retrieved.unwrap().operands.len(), 2);
    }

    #[test]
    fn instruction_side_table_remove_returns_payload() {
        let mut table = InstructionSideTable::new();
        let inst_id = IrNodeId::new(1).unwrap();

        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(RegId(0)));
                ops
            },
            encoding_hint: None,
        };

        table.insert(inst_id, inst.clone());
        assert_eq!(table.len(), 1);

        let removed = table.remove(inst_id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().mnemonic, Mnemonic::Add);
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    // ── Mnemonic size constraint ────────────────────────────────────────

    #[test]
    fn mnemonic_size_fits_in_four_bytes() {
        use std::mem::size_of;
        // Mnemonic includes Jcc(Cond) (1 byte tag + 1 byte data) and
        // MovCr/MovDr/In/Out with bool or u8 payloads. Max size is 4 bytes.
        assert!(size_of::<Mnemonic>() <= 4);
    }
}
