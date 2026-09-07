//! Register, memory and immediate operand kinds for x86_64 instructions.
//!
//! Extracted from the monolithic `instruction.rs` (issue #1402, umbrella #1399).
//! Public paths preserved: `paideia_as_runtime::instruction::{RegId, Scale,
//! SegReg, SegPrefix, Operand}`.

use alloc::boxed::Box;
use alloc::string::String;

/// x86_64 register identifier.
///
/// Compact encoding (see `unsafe_walker::register::register_name_to_regid` for
/// the authoritative name table):
/// - 0–15: GPR (RAX–R15)
/// - 16–24: control registers (CR0–CR8)
/// - 25–32: debug registers (DR0–DR7)
/// - 33–36: extended low-byte GPRs (SPL/BPL/SIL/DIL)
/// - 37–52: YMM0–YMM15 (AVX2, issue #1004)
/// - 53–68: XMM0–XMM15 (scalar SSE float, paideia-os #1333 / paideia-as#1333).
///   Kept disjoint from the YMM band even though XMM and YMM name the same
///   physical register file — the two bands select different encoder paths
///   (legacy-SSE non-VEX vs. VEX-prefixed AVX2), so collapsing them would
///   make the mnemonic ambiguous from the RegId alone.
///
/// Encoder side handles the actual register-encoding lookup table.
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

/// x86_64 segment register identifier.
///
/// Valid segment registers: ES, CS, SS, DS, FS, GS.
/// These are used in MOV sreg, r16 instructions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SegReg {
    /// Extra segment.
    Es,
    /// Code segment.
    Cs,
    /// Stack segment.
    Ss,
    /// Data segment.
    Ds,
    /// File segment.
    Fs,
    /// General-purpose segment.
    Gs,
}

impl SegReg {
    /// Encode segment register to the numeric ID for ModR/M field.
    /// Matches Intel SDM Vol 2A: ES=0, CS=1, SS=2, DS=3, FS=4, GS=5.
    #[must_use]
    pub fn id(self) -> u8 {
        match self {
            SegReg::Es => 0,
            SegReg::Cs => 1,
            SegReg::Ss => 2,
            SegReg::Ds => 3,
            SegReg::Fs => 4,
            SegReg::Gs => 5,
        }
    }
}

/// Segment prefix override for memory operands (PA-R13-002, issue #915).
///
/// Long-mode only affords fs (0x64) and gs (0x65) as effective overrides;
/// cs/ds/es/ss overrides are legal but ignored for address computation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SegPrefix {
    /// File segment prefix (0x64).
    Fs,
    /// General-purpose segment prefix (0x65).
    Gs,
}

impl SegPrefix {
    /// Encode segment prefix to its byte representation.
    ///
    /// - Fs → 0x64
    /// - Gs → 0x65
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            Self::Fs => 0x64,
            Self::Gs => 0x65,
        }
    }
}

/// An operand to an x86_64 instruction.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Operand {
    /// Register operand.
    Reg(RegId),
    /// Segment register operand (Phase 15 m5-002).
    SegReg(SegReg),
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
    /// RIP-relative memory with symbol: [rip + sym + addend].
    /// Used to disambiguate bracketed `call [rip + sym]` from bare `call sym`.
    /// The encoder emits a rel32 relocation with the symbol name.
    MemRipRelSym {
        /// Name of the symbol.
        name: String,
        /// Addend to apply to the symbol address.
        addend: i32,
    },
    /// Segment-prefixed memory operand (PA-R13-002, issue #915).
    ///
    /// `inner` MUST be one of: MemSib, MemDisp, or MemRipRel.
    /// The segment prefix (0x64/0x65) is emitted before the inner operand.
    MemSeg {
        /// Segment prefix (fs/gs).
        seg: SegPrefix,
        /// Inner memory operand (boxed to avoid enum size bloat).
        inner: Box<Operand>,
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
    /// Unresolved local binding variable: resolve_var_operands pass rewrites to Operand::Reg.
    /// Phase 7 m2-003: used by unsafe.let-chain to reference local bindings.
    /// Unknown bindings → T0528; successfully resolved → replaced with Operand::Reg.
    Var {
        /// Name of the local binding.
        name: String,
    },
    /// Indexed memory with symbol (no base/RIP): [sym + index*scale + addend].
    /// PA-R15-009a: used for absolute-address jump tables via jmp [sym + rX*scale].
    /// Emits FF/4 ModRM with SIB, base=0b101 (no base), index and scale from operand.
    /// Relocation: RelocKind::Abs32 (absolute, not RIP-relative).
    MemSymIndexed {
        /// Name of the symbol.
        name: String,
        /// Addend to apply to the symbol address.
        addend: i32,
        /// Index register (cannot be RSP).
        index: RegId,
        /// Scale factor for index.
        scale: Scale,
    },
    /// Absolute-address indexed memory: [disp + index*scale], no base, no RIP.
    /// Produced by `resolve_symbols` from `MemSymIndexed`. Encoder emits FF/4
    /// ModRM with SIB, base=0b101 (no base), no relocation.
    MemDispIndexed {
        /// Absolute displacement offset.
        disp: i32,
        /// Index register (cannot be RSP).
        index: RegId,
        /// Scale factor for index.
        scale: Scale,
    },
}
