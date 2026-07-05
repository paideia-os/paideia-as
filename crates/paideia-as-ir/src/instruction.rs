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

/// Instruction execution mode (bit-width).
///
/// Phase 15 m2-002: instruction mode (64-bit or 32-bit) propagated from
/// module-level #![bits=...] inner_attrs through the emit walk.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum InstrMode {
    /// 64-bit mode (default).
    #[default]
    Mode64,
    /// 32-bit mode.
    Mode32,
}

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
    /// Test (bitwise AND with register/memory, sets flags without storing).
    Test,
    /// Jcc with embedded condition code.
    Jcc(Cond),
    /// Setcc with embedded condition code.
    Setcc(Cond),
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
    /// Clear direction flag.
    Cld,
    /// Set interrupt flag.
    Sti,
    /// Set direction flag.
    Std,
    /// Halt processor.
    Hlt,
    /// Undefined instruction (causes #UD exception).
    Ud2,
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
    /// Move with sign-extend: sign-extend smaller operand to larger width.
    /// Phase 7 m4-002: used for widening signed casts; emits e.g.
    /// `movsx rax, r32` (REX.W 63 /r for r/m32 → r64).
    Movsx,
    /// Bitwise NOT (one's complement) of a 64-bit register.
    /// Phase 7 m4-001: emits `not r64` (REX.W F7 /2). One operand.
    Not,
    /// Byte-swap 64-bit register (endianness conversion).
    /// Phase R13 PA-R13-014 (issue #943): emits `bswap r64` (REX.W 0F C8+rd). One operand.
    Bswap,
    /// Byte-swap 32-bit register (endianness conversion).
    /// Phase R15 PA-R15-001 (issue #956): emits `bswap r32` (0F C8+rd, no REX.W). One operand.
    Bswap32,
    /// Push 64-bit register onto stack.
    /// Phase R9 m2-001 (PA-R9-001): emits `push r64` (REX.W 50+rd or 41 50+rd for r8–r15).
    /// One operand.
    Push,
    /// Pop 64-bit register from stack.
    /// Phase R9 m2-001 (PA-R9-001): emits `pop r64` (REX.W 58+rd or 41 58+rd for r8–r15).
    /// One operand.
    Pop,
    /// Push flags register onto stack.
    /// Phase R9 m2-002 (PA-R9-002): emits `pushfq` (0x9C). Zero operands.
    Pushfq,
    /// Pop flags register from stack.
    /// Phase R9 m2-002 (PA-R9-002): emits `popfq` (0x9D). Zero operands.
    Popfq,
    /// Breakpoint interrupt (INT 3).
    /// Phase R9 m2-003 (PA-R9-003): emits `int3` (0xCC). Zero operands.
    Int3,
    /// Width-aware immediate-to-register move.
    /// Phase 7 m4-003 (PA7C-m4-003): used for typed integer-literal `let`
    /// bindings so `let x : u32 = 42` emits the 5-byte `B8 imm32` form
    /// (implicit zero-extend, no REX.W) instead of the generic 10-byte
    /// `48 C7 C0 imm32` 64-bit move. The `width` selects the operand size:
    /// - W64 delegates to the existing generic `Mov` encoding path.
    /// - W32 → `B8+rd imm32` (5 bytes, implicit zero-extend to r64).
    /// - W16 → `66 B8+rd imm16` (4 bytes).
    /// - W8  → `B0+rb imm8` (2 bytes; 3 with REX.B for r8–r15).
    ///
    /// Arity 2 (dst reg, imm). Only the literal-`let` site emits this today;
    /// peer `Mov` immediate sites are a deferred follow-up.
    MovSized {
        /// Operand width selecting the encoded form.
        width: IntWidth,
    },
    /// Shift left (logical). Operands: dst, shift_count.
    /// Phase 8 m1-001d: emits `shl r64, imm8` or `shl r64, cl`.
    Shl,
    /// Shift right (logical). Operands: dst, shift_count.
    /// Phase 8 m1-001d: emits `shr r64, imm8` or `shr r64, cl`.
    Shr,
    /// Arithmetic shift right. Operands: dst, shift_count.
    /// Phase 8 m1-001d: emits `sar r64, imm8` or `sar r64, cl`.
    Sar,
    /// Rotate left. Operands: dst, rotate_count.
    /// Phase R15 PA-R15-004 (issue #959): emits `rol r32/r64, imm8` or `rol r32/r64, cl`.
    Rol {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Rotate right. Operands: dst, rotate_count.
    /// Phase R15 PA-R15-004 (issue #959): emits `ror r32/r64, imm8` or `ror r32/r64, cl`.
    Ror {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Integer multiply. Operands: dst, src1 [, src2/imm].
    /// Phase 8 m1-001d: emits `imul r64, r64` (2 operands) or `imul r64, r64, imm` (3 operands).
    Imul,
    /// Bitwise AND. Operands: dst, src.
    /// Phase 8 m1-001d: emits `and r64, r64` or `and r64, imm`.
    And,
    /// Bitwise OR. Operands: dst, src.
    /// Phase 8 m1-001d: emits `or r64, r64` or `or r64, imm`.
    Or,
    /// Bitwise XOR. Operands: dst, src.
    /// Phase 8 m1-001d: emits `xor r64, r64` or `xor r64, imm`.
    Xor,
    /// Invalidate TLB entry. Phase 8 m5-001: emits `invlpg [mem]` (0F 01 /7).
    Invlpg,
    /// Read time-stamp counter. Phase 8 m5-001: emits `rdtsc` (0F 31), returns in RDX:RAX.
    Rdtsc,
    /// Unsigned integer divide (64-bit). Phase R11 PA-R11-006: emits `div r64` (REX.W F7 /6).
    /// One operand (the divisor register).
    Div,
    /// Signed integer divide (64-bit). Phase R11 PA-R11-006: emits `idiv r64` (REX.W F7 /7).
    /// One operand (the divisor register).
    Idiv,
    /// LTR — Load Task Register (PA-R13-001, #914). Encoding: 0F 00 /3 per Intel SDM Vol 2A.
    /// One operand (the source register, 16-bit).
    Ltr,
    /// xchg r/m64, r64 (PA-R13-003, #916). Memory form is implicitly locked
    /// per Intel SDM Vol 2A — no LOCK prefix required. Encoding: REX.W 87 /r.
    /// Two operands (mem, reg). Reg-reg form not supported in R13.
    Xchg,
    /// lock cmpxchg r/m64, r64 (PA-R13-004, #917). Encoding: F0 REX.W 0F B1 /r.
    /// Compares implicit RAX with r/m64; if equal writes reg64, else loads r/m64
    /// into RAX. Two operands (mem, reg). Reg-reg form not supported in R13.
    LockCmpxchg,
    /// mfence (PA-R13-005, #918). Serializing memory barrier: all preceding
    /// loads and stores complete before subsequent ones. Encoding: 0F AE F0.
    /// Zero operands.
    Mfence,
    /// sfence (PA-R14-004, #947). Store fence: preceding stores complete
    /// before subsequent ones. Encoding: 0F AE F8. Zero operands.
    Sfence,
    /// lfence (PA-R14-004, #947). Load fence: preceding loads complete
    /// before subsequent ones. Encoding: 0F AE E8. Zero operands.
    Lfence,
    /// wbinvd (PA-R14-005, #948). Write-back and invalidate cache.
    /// Encoding: 0F 09. Zero operands. Privileged.
    Wbinvd,
    /// invd (PA-R14-005, #948). Invalidate cache (no write-back).
    /// Encoding: 0F 08. Zero operands. Privileged.
    Invd,
    /// fxsave [base + disp] (PA-R13-007, #920). Saves x87/MMX/SSE state to memory.
    /// Instruction: 0F AE /0 (reg field = 000). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Fxsave,
    /// fxrstor [base + disp] (PA-R13-007, #920). Restores state from a matching fxsave frame.
    /// Instruction: 0F AE /1 (reg field = 001). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Fxrstor,
    /// clflush [base + disp] (PA-R14-005, #948). Flush cache line to main memory.
    /// Instruction: 0F AE /7 (reg field = 111). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Clflush,
    /// clflushopt [base + disp] (PA-R14-005, #948). Optimized cache line flush.
    /// Instruction: 66 0F AE /7 (reg field = 111). 0x66 prefix; REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Clflushopt,
    /// prefetchnta [base + disp] (PA-R14-006, #949). Prefetch for non-temporal access.
    /// Instruction: 0F 18 /0 (reg field = 000). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Prefetchnta,
    /// prefetcht0 [base + disp] (PA-R14-006, #949). Prefetch temporal (all cache levels).
    /// Instruction: 0F 18 /1 (reg field = 001). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Prefetcht0,
    /// prefetcht1 [base + disp] (PA-R14-006, #949). Prefetch temporal (L2 down).
    /// Instruction: 0F 18 /2 (reg field = 010). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Prefetcht1,
    /// prefetcht2 [base + disp] (PA-R14-006, #949). Prefetch temporal (L3 down).
    /// Instruction: 0F 18 /3 (reg field = 011). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Prefetcht2,
    /// `inc r64` (PA-R13-005, #934). Increment 64-bit register by 1.
    /// Encoding: REX.W FF /0 (ModR/M reg field = 000 → 0xC0 | (reg & 7)).
    /// One operand (the destination register).
    Inc,
    /// `dec r64` (PA-R13-005, #934). Decrement 64-bit register by 1.
    /// Encoding: REX.W FF /1 (ModR/M reg field = 001 → 0xC8 | (reg & 7)).
    /// One operand (the destination register).
    Dec,
    /// Non-temporal store: `movnti [mem], r32/r64` (PA-R14-003, #946).
    /// Bypasses cache; used for streaming stores. Two operands (mem, src reg).
    /// Encoding: `0F C3 /r` (W32) or `REX.W 0F C3 /r` (W64).
    Movnti {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// LOCK-prefixed fetch-and-add: `lock xadd [mem], r32/r64` (PA-R15-002, #957).
    /// Atomically adds register to memory and stores old value in register.
    /// Two operands (mem, src reg). Encoding: `F0 [REX.W] 0F C1 /r`.
    /// Effect: !{Atomic}. Per Intel SDM Vol 2B XADD; LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockXadd {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// LOCK-prefixed add: `lock add [mem], imm8/imm32/r32/r64` (PA-R15-003, #958).
    /// Atomically adds immediate or register to memory. Two operands (mem, src).
    /// Encoding: `F0 [REX.W] 83/81/01 /0 [imm8/imm32]` per Intel SDM Vol 2A ADD.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockAdd {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// LOCK-prefixed sub: `lock sub [mem], imm8/imm32/r32/r64` (PA-R15-003, #958).
    /// Atomically subtracts immediate or register from memory. Two operands (mem, src).
    /// Encoding: `F0 [REX.W] 83/81/29 /5 [imm8/imm32]` per Intel SDM Vol 2A SUB.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockSub {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Add with carry: `adc r32/r64, r/m32/r/m64` (PA-R15-005, #960).
    /// Adds register/memory to register + carry flag. Two operands (dst, src).
    /// Encoding: `[REX.W] 13 /r` (opcode 0x13 for adc reg-dst) per Intel SDM Vol 2A ADC.
    /// Effect: reads/writes CF.
    Adc {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Subtract with borrow: `sbb r32/r64, r/m32/r/m64` (PA-R15-005, #960).
    /// Subtracts register/memory from register - carry flag. Two operands (dst, src).
    /// Encoding: `[REX.W] 1B /r` (opcode 0x1B for sbb reg-dst) per Intel SDM Vol 2A SBB.
    /// Effect: reads/writes CF.
    Sbb {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
}

/// Integer operand width for width-threaded immediate moves.
///
/// Phase 7 m4-003 (PA7C-m4-003): maps a bound integer literal's bit-width
/// (from its declared type) to the encoded move form. `from_bits` converts
/// a layout bit-width (8/16/32/64) into the corresponding variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum IntWidth {
    /// 8-bit operand (`B0+rb imm8`).
    W8,
    /// 16-bit operand (`66 B8+rd imm16`).
    W16,
    /// 32-bit operand (`B8+rd imm32`, implicit zero-extend).
    W32,
    /// 64-bit operand (delegates to the generic `Mov` path).
    W64,
}

impl IntWidth {
    /// Map a bit-width (8/16/32/64) to an `IntWidth`.
    ///
    /// Returns `None` for any other bit-width (e.g. 128, or non-power-of-two
    /// widths), in which case callers fall back to the generic 64-bit path.
    #[must_use]
    pub fn from_bits(bits: u16) -> Option<Self> {
        match bits {
            8 => Some(IntWidth::W8),
            16 => Some(IntWidth::W16),
            32 => Some(IntWidth::W32),
            64 => Some(IntWidth::W64),
            _ => None,
        }
    }

    /// Conservative upper bound on the encoded byte length for this width.
    ///
    /// - W8  → 3 bytes (`REX.B B0+rb imm8`, REX present only for r8–r15)
    /// - W16 → 4 bytes (`66 B8+rd imm16`)
    /// - W32 → 5 bytes (`B8+rd imm32`)
    /// - W64 → 10 bytes (generic `Mov` upper bound)
    #[must_use]
    pub fn estimated_size(self) -> u32 {
        match self {
            IntWidth::W8 => 3,
            IntWidth::W16 => 4,
            IntWidth::W32 => 5,
            IntWidth::W64 => 10,
        }
    }
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
    /// Parity flag set (jp/setp).
    Parity,
    /// Parity flag not set (jnp/setnp).
    NotParity,
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
            | Mnemonic::Cld
            | Mnemonic::Sti
            | Mnemonic::Std
            | Mnemonic::Hlt
            | Mnemonic::Ud2
            | Mnemonic::Nop
            | Mnemonic::Swapgs
            | Mnemonic::Cpuid
            | Mnemonic::Wrmsr
            | Mnemonic::Rdmsr
            | Mnemonic::Iret
            | Mnemonic::Iretq
            | Mnemonic::Sysret
            | Mnemonic::Syscall
            | Mnemonic::RepStosq
            | Mnemonic::Rdtsc
            | Mnemonic::Pushfq
            | Mnemonic::Popfq
            | Mnemonic::Int3
            | Mnemonic::Sfence
            | Mnemonic::Lfence
            | Mnemonic::Wbinvd
            | Mnemonic::Invd => 0,

            // One-operand instructions
            Mnemonic::Call
            | Mnemonic::Ret
            | Mnemonic::Jmp
            | Mnemonic::Jcc(_)
            | Mnemonic::Setcc(_)
            | Mnemonic::RepMovsb
            | Mnemonic::Lgdt
            | Mnemonic::Lidt
            | Mnemonic::MovCr { .. }
            | Mnemonic::MovDr { .. }
            | Mnemonic::In { .. }
            | Mnemonic::Out { .. }
            | Mnemonic::Int
            | Mnemonic::Not
            | Mnemonic::Push
            | Mnemonic::Pop
            | Mnemonic::FarJmp
            | Mnemonic::Invlpg
            | Mnemonic::Div
            | Mnemonic::Idiv
            | Mnemonic::Ltr
            | Mnemonic::Fxsave
            | Mnemonic::Fxrstor
            | Mnemonic::Clflush
            | Mnemonic::Clflushopt
            | Mnemonic::Prefetchnta
            | Mnemonic::Prefetcht0
            | Mnemonic::Prefetcht1
            | Mnemonic::Prefetcht2
            | Mnemonic::Inc
            | Mnemonic::Dec
            | Mnemonic::Bswap
            | Mnemonic::Bswap32 => 1,

            // Two-operand instructions
            Mnemonic::Mov
            | Mnemonic::Add
            | Mnemonic::Sub
            | Mnemonic::Adc { .. }
            | Mnemonic::Sbb { .. }
            | Mnemonic::Cmp
            | Mnemonic::Test
            | Mnemonic::Lea
            | Mnemonic::Movzx
            | Mnemonic::Movsx
            | Mnemonic::MovSized { .. }
            | Mnemonic::Shl
            | Mnemonic::Shr
            | Mnemonic::Sar
            | Mnemonic::Rol { .. }
            | Mnemonic::Ror { .. }
            | Mnemonic::Imul
            | Mnemonic::And
            | Mnemonic::Or
            | Mnemonic::Xor
            | Mnemonic::Xchg
            | Mnemonic::LockCmpxchg
            | Mnemonic::LockXadd { .. }
            | Mnemonic::LockAdd { .. }
            | Mnemonic::LockSub { .. }
            | Mnemonic::Movnti { .. } => 2,

            // Zero-operand instructions (continued)
            Mnemonic::Mfence => 0,
        }
    }

    /// Return a conservative upper bound on the encoded size in bytes for this mnemonic.
    ///
    /// Phase 7 m2-001 (PA7C-m2-001): This is used to estimate per-instruction byte offsets
    /// during IR traversal before the encoding pass. The estimates are intentionally
    /// conservative (upper bounds) to avoid off-by-one errors in offset calculations.
    ///
    /// The bounds cover:
    /// - Zero-operand: Hlt/Cli/Nop = 1 byte
    /// - System: Cpuid/Wrmsr/Rdmsr/Syscall = 2 bytes
    /// - I/O: In/Out = 2 bytes
    /// - Jumps: Jcc = 6 bytes (conditional), Jmp = 5 bytes (unconditional)
    /// - Calls: Call = 5 bytes, Ret = 1 byte
    /// - Privilege: Lgdt/Lidt = 7 bytes
    /// - Moves: Mov = 10 bytes, Movzx = 10 bytes
    /// - Arithmetic: Add/Sub/Cmp/Test = 10 bytes
    /// - Addressing: Lea = 10 bytes
    /// - String: RepMovsb = 2 bytes
    /// - Others: 10 bytes (conservative default)
    #[must_use]
    pub fn estimated_size(&self, _operands: &[Operand]) -> u32 {
        match self {
            // Zero-arity, 1 byte
            Mnemonic::Hlt | Mnemonic::Cli | Mnemonic::Cld | Mnemonic::Sti | Mnemonic::Std | Mnemonic::Nop => 1,

            // Zero-arity system instructions, 2 bytes
            Mnemonic::Cpuid
            | Mnemonic::Wrmsr
            | Mnemonic::Rdmsr
            | Mnemonic::Syscall
            | Mnemonic::Rdtsc => 2,

            // One-arity I/O, 2 bytes
            Mnemonic::In { .. } | Mnemonic::Out { .. } => 2,

            // Ret, 1 byte
            Mnemonic::Ret => 1,

            // Conditional jump: 6 bytes (2-byte opcode + 4-byte offset)
            Mnemonic::Jcc(_) => 6,

            // Conditional set byte: 4 bytes (REX + 0F + 9X + ModR/M)
            Mnemonic::Setcc(_) => 4,

            // Unconditional jump: 5 bytes (1-byte opcode + 4-byte offset)
            Mnemonic::Jmp => 5,

            // Call: 5 bytes (1-byte opcode + 4-byte offset)
            Mnemonic::Call => 5,

            // Privilege table loads: 7 bytes (REX + opcode + SIB + disp)
            Mnemonic::Lgdt | Mnemonic::Lidt => 7,

            // TLB invalidate: 7 bytes (opcode + SIB + disp)
            Mnemonic::Invlpg => 7,

            // String operations: 2 bytes (prefix + opcode)
            Mnemonic::RepMovsb => 2,

            // Move: 10 bytes (REX + opcode + ModRM + SIB + disp32)
            Mnemonic::Mov => 10,

            // Move with zero-extend: 10 bytes
            Mnemonic::Movzx => 10,
            Mnemonic::Movsx => 10,

            // Two-operand arithmetic/logic: 10 bytes
            Mnemonic::Add | Mnemonic::Sub | Mnemonic::Adc { .. } | Mnemonic::Sbb { .. } | Mnemonic::Cmp | Mnemonic::Test => 10,

            // Load effective address: 10 bytes
            Mnemonic::Lea => 10,

            // Control register moves: assume 10 bytes
            Mnemonic::MovCr { .. } | Mnemonic::MovDr { .. } => 10,

            // Interrupt return: 1 byte
            Mnemonic::Iret => 1,

            // Interrupt return 64-bit: 1 byte
            Mnemonic::Iretq => 1,

            // System return: 1 byte
            Mnemonic::Sysret => 1,

            // Swap GS: 2 bytes
            Mnemonic::Swapgs => 2,

            // Undefined instruction: 2 bytes
            Mnemonic::Ud2 => 2,

            // RepStosq: 2 bytes
            Mnemonic::RepStosq => 2,

            // Software interrupt: 2 bytes
            Mnemonic::Int => 2,

            // Far jump: 7 bytes (1-byte opcode + 6-byte far address)
            Mnemonic::FarJmp => 7,

            // Bitwise NOT: 4 bytes upper bound (REX.W F7 /2 ModR/M)
            Mnemonic::Not => 4,

            // Byte-swap: 3 bytes upper bound (REX.W 0F C8+rd, no ModR/M)
            Mnemonic::Bswap => 3,

            // Byte-swap 32-bit: 3 bytes upper bound (REX.B 0F C8+rd, no ModR/M)
            Mnemonic::Bswap32 => 3,

            // Divide: 4 bytes upper bound (REX.W F7 /6 or /7 ModR/M)
            Mnemonic::Div | Mnemonic::Idiv => 4,

            // Load Task Register: 4 bytes upper bound (REX.B 0F 00 /3 ModR/M)
            Mnemonic::Ltr => 4,

            // Push/Pop: 2 bytes upper bound (REX.W 50+r or 41 50+r for r8–r15)
            Mnemonic::Push | Mnemonic::Pop => 2,

            // Pushfq/Popfq: 1 byte (0x9C or 0x9D)
            Mnemonic::Pushfq | Mnemonic::Popfq => 1,

            // Int3: 1 byte (0xCC)
            Mnemonic::Int3 => 1,

            // Shift operations: 4 bytes upper bound (REX.W C1 r/m, imm8 or REX.W D3 r/m for CL variant)
            // Rotate operations: 4 bytes upper bound (REX.W C1 r/m, imm8 or REX.W D3 r/m for CL variant)
            Mnemonic::Shl | Mnemonic::Shr | Mnemonic::Sar | Mnemonic::Rol { .. } | Mnemonic::Ror { .. } => 4,

            // Multiply (imul): 10 bytes upper bound for r64, r64, imm32
            Mnemonic::Imul => 10,

            // Bitwise AND/OR/XOR: 10 bytes upper bound
            Mnemonic::And | Mnemonic::Or | Mnemonic::Xor => 10,

            // Width-threaded immediate move: size depends on the operand width.
            // W8=3 (REX.B B0+rb imm8), W16=4 (66 B8 imm16), W32=5 (B8 imm32),
            // W64=10 (generic Mov upper bound).
            Mnemonic::MovSized { width } => width.estimated_size(),

            // Phase R13 PA-R13-003: exchange register with memory, 8 bytes upper bound
            Mnemonic::Xchg => 8,

            // Phase R13 PA-R13-004: lock cmpxchg register with memory, 10 bytes upper bound
            Mnemonic::LockCmpxchg => 10,

            // Phase R13 PA-R13-005: memory fence, 3 bytes
            Mnemonic::Mfence => 3,

            // Phase R14 PA-R14-004: lfence/sfence, 3 bytes each
            Mnemonic::Lfence | Mnemonic::Sfence => 3,

            // Phase R14 PA-R14-005: wbinvd/invd, 2 bytes each
            Mnemonic::Wbinvd | Mnemonic::Invd => 2,

            // Phase R13 PA-R13-007: fxsave/fxrstor to memory, 9 bytes upper bound
            // (two-byte opcode + REX.B + SIB + disp32 worst-case)
            Mnemonic::Fxsave | Mnemonic::Fxrstor => 9,

            // Phase R14 PA-R14-005: clflush/clflushopt, 9 bytes upper bound
            // (0x66 prefix + two-byte opcode + REX.B + SIB + disp32 worst-case)
            Mnemonic::Clflush | Mnemonic::Clflushopt => 9,

            // Phase R14 PA-R14-006: prefetchnta/prefetcht0/prefetcht1/prefetcht2, 9 bytes upper bound
            // (two-byte opcode 0F 18 + REX.B + SIB + disp32 worst-case)
            Mnemonic::Prefetchnta | Mnemonic::Prefetcht0 | Mnemonic::Prefetcht1 | Mnemonic::Prefetcht2 => 9,

            // Phase R13 PA-R13-005 (issue #934): inc/dec r64, 3 bytes exact
            // (REX.W FF ModR/M — REX.B for r8..r15 replaces REX.W's high nibble bit
            // but total remains 3 bytes).
            Mnemonic::Inc | Mnemonic::Dec => 3,

            // Phase R14 PA-R14-003 (issue #946): non-temporal store movnti, 8 bytes upper bound
            // (REX + 0F + C3 + ModR/M + disp32 worst-case)
            Mnemonic::Movnti { .. } => 8,

            // Phase R15 PA-R15-002 (issue #957): lock xadd to memory, 8 bytes upper bound
            // (LOCK + REX + 0F + C1 + ModR/M + disp32 worst-case)
            Mnemonic::LockXadd { .. } => 8,

            // Phase R15 PA-R15-003 (issue #958): lock add/sub to memory, 9 bytes upper bound
            // (LOCK + REX + 81 + ModR/M + disp32 + imm32 worst-case)
            Mnemonic::LockAdd { .. } | Mnemonic::LockSub { .. } => 9,
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
    /// Byte offset in .text section where this instruction was emitted.
    /// Populated during encoding pass (phase-7-m1-003). Used to compute
    /// relocation offsets precisely, avoiding off-by-one errors that occur
    /// when encoder reads buf.bytes.len() after encoding.
    pub byte_offset_in_text: Option<u32>,
    /// Instruction mode (Mode64 or Mode32).
    pub mode: InstrMode,
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
            byte_offset_in_text: None,
            mode: InstrMode::default(),
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
            byte_offset_in_text: None,
            mode: InstrMode::default(),
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
            byte_offset_in_text: None,
            mode: InstrMode::default(),
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
