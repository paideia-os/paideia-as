//! Static per-mnemonic tables: arity, feature gating, implicit register
//! reads/writes, and conservative estimated encoded byte length.
//!
//! Extracted from the monolithic `instruction.rs` (issue #1402, umbrella #1399).
//! Public paths preserved: methods hang off `Mnemonic` and reach the same
//! `paideia_as_runtime::instruction::…` path as before.

use super::cpu_feature::CpuFeature;
use super::mnemonic::Mnemonic;
use super::operand::{Operand, RegId};

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
            | Mnemonic::Endbr64
            | Mnemonic::Endbr32
            | Mnemonic::Swapgs
            | Mnemonic::Cpuid
            | Mnemonic::Wrmsr
            | Mnemonic::Rdmsr
            | Mnemonic::Iret
            | Mnemonic::Iretq
            | Mnemonic::Sysret
            | Mnemonic::Syscall
            | Mnemonic::RepStosq
            | Mnemonic::RepStosb
            | Mnemonic::RepMovsq
            | Mnemonic::Rdtsc
            | Mnemonic::Pushfq
            | Mnemonic::Popfq
            | Mnemonic::Int3
            | Mnemonic::Sfence
            | Mnemonic::Lfence
            | Mnemonic::Pause
            | Mnemonic::Wbinvd
            | Mnemonic::Invd
            | Mnemonic::Xgetbv
            | Mnemonic::Xsetbv => 0,

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
            // paideia-as#1398: mul r64 is one-operand (implicit rax multiplicand).
            | Mnemonic::Mul
            | Mnemonic::Ltr
            | Mnemonic::Fxsave
            | Mnemonic::Fxrstor
            | Mnemonic::Xsaveopt
            | Mnemonic::Xrstor
            | Mnemonic::Clflush
            | Mnemonic::Clflushopt
            | Mnemonic::Prefetchnta
            | Mnemonic::Prefetcht0
            | Mnemonic::Prefetcht1
            | Mnemonic::Prefetcht2
            | Mnemonic::Inc
            | Mnemonic::Dec
            | Mnemonic::Bswap
            | Mnemonic::Bswap32
            | Mnemonic::LockCmpxchg16b
            | Mnemonic::LockInc { .. } => 1,

            // Two-operand instructions
            Mnemonic::Mov
            | Mnemonic::Add
            | Mnemonic::Sub
            | Mnemonic::Adc { .. }
            | Mnemonic::Sbb { .. }
            | Mnemonic::Popcnt { .. }
            | Mnemonic::Crc32 { .. }
            | Mnemonic::Bsf { .. }
            | Mnemonic::Bsr { .. }
            | Mnemonic::Tzcnt { .. }
            | Mnemonic::Bt { .. }
            | Mnemonic::Bts { .. }
            | Mnemonic::Btr { .. }
            | Mnemonic::Btc { .. }
            | Mnemonic::LockBts { .. }
            | Mnemonic::LockBtr { .. }
            | Mnemonic::LockBtc { .. }
            | Mnemonic::LockAnd { .. }
            | Mnemonic::LockOr { .. }
            | Mnemonic::LockXor { .. }
            | Mnemonic::Cmp
            | Mnemonic::CmpSized { .. }
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
            | Mnemonic::LockCmpxchg32
            | Mnemonic::LockXadd { .. }
            | Mnemonic::LockAdd { .. }
            | Mnemonic::LockSub { .. }
            | Mnemonic::Movnti { .. }
            | Mnemonic::Vpmovmskb
            | Mnemonic::Vmovdqu { .. }
            // v0.21-009-followup (#1297): invpcid r64, m128 — arity 2.
            | Mnemonic::Invpcid => 2,

            // Three-operand instructions (Phase R18 PA-R18-011 issue #1004)
            Mnemonic::Vpxor | Mnemonic::Vpcmpeqb => 3,

            // Zero-operand instructions (continued)
            Mnemonic::Mfence => 0,

            // Scalar SSE float instructions (paideia-os #1333, paideia-as#1333): all two-operand.
            Mnemonic::MovSd
            | Mnemonic::MovSs
            | Mnemonic::AddSd
            | Mnemonic::AddSs
            | Mnemonic::SubSd
            | Mnemonic::SubSs
            | Mnemonic::MulSd
            | Mnemonic::MulSs
            | Mnemonic::DivSd
            | Mnemonic::DivSs
            | Mnemonic::Sqrtsd
            | Mnemonic::Sqrtss
            | Mnemonic::Ucomisd
            | Mnemonic::Ucomiss
            | Mnemonic::Comisd
            | Mnemonic::Comiss
            | Mnemonic::Cvtsi2sd
            | Mnemonic::Cvtsi2ss
            | Mnemonic::Cvttsd2si
            | Mnemonic::Cvttss2si
            | Mnemonic::MovdBitcast { .. }
            | Mnemonic::MovqBitcast { .. } => 2,
        }
    }

    /// Return the CPU feature this mnemonic requires, if any.
    /// `None` means the mnemonic is baseline x86_64-v1.
    ///
    /// PA-r16-004-backtrack-a (#1033): compile-time CPU-feature declaration + gating mechanism.
    #[must_use]
    pub fn required_feature(&self) -> Option<CpuFeature> {
        match self {
            Self::LockCmpxchg16b => Some(CpuFeature::Cx16),
            Self::Popcnt { .. } => Some(CpuFeature::Popcnt),
            Self::Crc32 { .. } => Some(CpuFeature::Sse42),
            Self::Tzcnt { .. } => Some(CpuFeature::Bmi1),
            Self::Endbr64 | Self::Endbr32 => Some(CpuFeature::Cet),
            Self::Xsaveopt => Some(CpuFeature::Xsaveopt),
            Self::Xrstor => Some(CpuFeature::Xsave),
            _ => None,
        }
    }

    /// Registers this mnemonic implicitly reads (beyond the explicit
    /// operand list). Static tables per mnemonic. Empty slice for
    /// mnemonics that only read their explicit operands.
    ///
    /// PA-r16-004-backtrack-b (#1034): register-clobber and implicit-operand tracking for LOCK-prefixed mnemonics.
    #[must_use]
    pub fn implicit_reads(&self) -> &'static [RegId] {
        // SysV RAX/RCX/RDX/RBX — see paideia-as-ir::abi
        const RAX: RegId = RegId(0);
        const RCX: RegId = RegId(1);
        const RDX: RegId = RegId(2);
        const RBX: RegId = RegId(3);
        match self {
            Self::LockCmpxchg => &[RAX],
            Self::LockCmpxchg32 => &[RAX],
            Self::LockCmpxchg16b => &[RAX, RDX, RBX, RCX],
            _ => &[],
        }
    }

    /// Registers this mnemonic implicitly writes (beyond the explicit
    /// operand list). Static tables per mnemonic. Empty slice for
    /// mnemonics that only write their explicit operands.
    ///
    /// PA-r16-004-backtrack-b (#1034): register-clobber and implicit-operand tracking for LOCK-prefixed mnemonics.
    #[must_use]
    pub fn implicit_writes(&self) -> &'static [RegId] {
        // SysV RAX/RDX — see paideia-as-ir::abi
        const RAX: RegId = RegId(0);
        const RDX: RegId = RegId(2);
        match self {
            Self::LockCmpxchg => &[RAX],
            Self::LockCmpxchg32 => &[RAX],
            Self::LockCmpxchg16b => &[RAX, RDX],
            _ => &[],
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

            // Unconditional jump: 5 bytes (rel32) or 7-8 bytes (MemSymIndexed/MemDispIndexed with SIB)
            Mnemonic::Jmp => {
                // PA-R15-009a: check if operand is MemSymIndexed or MemDispIndexed; if so, return 7-8 bytes based on index register
                if _operands.len() == 1 {
                    match &_operands[0] {
                        Operand::MemSymIndexed { index, .. } | Operand::MemDispIndexed { index, .. } => {
                            // 7 bytes if index < R8, 8 bytes if index >= R8 (REX.X)
                            return if index.0 < 8 { 7 } else { 8 };
                        }
                        _ => {}
                    }
                }
                5 // default for Imm64 or LabelRef
            },

            // Call: 5 bytes (1-byte opcode + 4-byte offset)
            Mnemonic::Call => 5,

            // Privilege table loads: 7 bytes (REX + opcode + SIB + disp)
            Mnemonic::Lgdt | Mnemonic::Lidt => 7,

            // TLB invalidate: 7 bytes (opcode + SIB + disp)
            Mnemonic::Invlpg => 7,

            // v0.21-009-followup (#1297): invpcid r64, m128.
            // Upper bound: 66 + REX + 0F 38 82 + ModR/M + SIB + disp32 = 11 bytes.
            Mnemonic::Invpcid => 11,

            // String operations: 2 bytes (prefix + opcode)
            Mnemonic::RepMovsb => 2,

            // Move: 10 bytes (REX + opcode + ModRM + SIB + disp32)
            Mnemonic::Mov => 10,

            // Move with zero-extend: 10 bytes
            Mnemonic::Movzx => 10,
            Mnemonic::Movsx => 10,

            // Two-operand arithmetic/logic: 10 bytes
            Mnemonic::Add | Mnemonic::Sub | Mnemonic::Adc { .. } | Mnemonic::Sbb { .. } | Mnemonic::Cmp | Mnemonic::Test => 10,

            // Width-threaded compare: size depends on the operand width.
            // W8=2 (3C ib for AL, 80 F8 ib for R8B), W16=4, W32=4, W64=10.
            Mnemonic::CmpSized { width } => width.estimated_size(),

            // Population count: 9 bytes (F3 + REX + 0F + B8 + ModR/M + disp32 max)
            Mnemonic::Popcnt { .. } => 9,

            // CRC32 checksum: 9 bytes (F2 + REX + 0F + 38 + F1 + ModR/M + disp32 max)
            Mnemonic::Crc32 { .. } => 9,

            // Bit scan forward/reverse and trailing-zero count: 10 bytes
            // Phase R16 PA-R16-008 (issue #974): bsf, bsr, tzcnt
            // (F3 + REX + 0F + opcode + ModR/M + SIB + disp32 max for tzcnt; no F3 for bsf/bsr)
            Mnemonic::Bsf { .. } | Mnemonic::Bsr { .. } | Mnemonic::Tzcnt { .. } => 10,

            // Bit test operations: 9 bytes (REX + 0F + opcode + ModR/M + disp32 max)
            // Phase R16 PA-R16-001 (issue #967): bt, bts, btr, btc
            // Phase R16 PA-R16-002 (issue #968): lock bts, lock btr, lock btc (9 bytes: LOCK + REX + 0F + opcode + ModR/M + disp32 + imm8 max)
            Mnemonic::Bt { .. } | Mnemonic::Bts { .. } | Mnemonic::Btr { .. } | Mnemonic::Btc { .. }
            | Mnemonic::LockBts { .. } | Mnemonic::LockBtr { .. } | Mnemonic::LockBtc { .. } => 9,

            // Phase R16 PA-R16-006 (issue #972): lock and/or/xor to memory, 9 bytes upper bound
            // (LOCK + REX + opcode + ModR/M + disp32 worst-case)
            Mnemonic::LockAnd { .. } | Mnemonic::LockOr { .. } | Mnemonic::LockXor { .. } => 9,

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

            // Intel CET End Branch: 4 bytes
            Mnemonic::Endbr64 | Mnemonic::Endbr32 => 4,

            // RepStosq: 2 bytes
            Mnemonic::RepStosq => 2,

            // RepStosb: 2 bytes (F3 AA)
            Mnemonic::RepStosb => 2,

            // RepMovsq: 3 bytes (F3 48 A5)
            Mnemonic::RepMovsq => 3,

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

            // Divide/Multiply: 4 bytes upper bound (REX.W F7 /4|/6|/7 ModR/M)
            // paideia-as#1398: mul r64 shares the F7 opcode family with div/idiv.
            Mnemonic::Div | Mnemonic::Idiv | Mnemonic::Mul => 4,

            // Load Task Register: 4 bytes upper bound (REX.B 0F 00 /3 ModR/M)
            Mnemonic::Ltr => 4,

            // Push: 5 bytes upper bound (imm32 form: 68 id)
            Mnemonic::Push => 5,
            // Pop: 2 bytes upper bound (REX.W 58+r or 41 58+r for r8–r15)
            Mnemonic::Pop => 2,

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

            // Phase R16 PA-R16-003: lock cmpxchg32 register with memory, 10 bytes upper bound
            Mnemonic::LockCmpxchg32 => 10,

            // Phase R16 PA-R16-004: lock cmpxchg16b register with memory, 10 bytes upper bound
            Mnemonic::LockCmpxchg16b => 10,

            // Phase R13 PA-R13-005: memory fence, 3 bytes
            Mnemonic::Mfence => 3,

            // Phase R14 PA-R14-004: lfence/sfence, 3 bytes each
            Mnemonic::Lfence | Mnemonic::Sfence => 3,

            // Phase R16 PA-R16-007: pause (spinloop hint), 2 bytes
            Mnemonic::Pause => 2,

            // Phase R14 PA-R14-005: wbinvd/invd, 2 bytes each
            Mnemonic::Wbinvd | Mnemonic::Invd => 2,

            // Phase R13 PA-R13-007: fxsave/fxrstor to memory, 9 bytes upper bound
            // (two-byte opcode + REX.B + SIB + disp32 worst-case)
            Mnemonic::Fxsave | Mnemonic::Fxrstor | Mnemonic::Xsaveopt | Mnemonic::Xrstor => 9,

            // v0.21-015 (paideia-as#1294): xgetbv/xsetbv, exact 3 bytes (0F 01 D0/D1)
            Mnemonic::Xgetbv | Mnemonic::Xsetbv => 3,

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

            // Phase R16 PA-R16-007 (issue #1060): lock inc to memory, 9 bytes upper bound
            // (LOCK + REX + FF + ModR/M + SIB + disp32 worst-case for absolute form)
            Mnemonic::LockInc { .. } => 9,

            // Phase R18 PA-R18-011 (issue #1004): AVX2 VEX-prefixed mnemonics
            // 2-byte or 3-byte VEX prefix (3 bytes worst case) + 2-byte opcode + ModR/M = 6 bytes min
            // Add SIB (1) + disp32 (4) for memory forms → 11 bytes max, conservative upper bound
            Mnemonic::Vpxor => 7,
            Mnemonic::Vpcmpeqb => 7,
            Mnemonic::Vpmovmskb => 7,
            Mnemonic::Vmovdqu { .. } => 11,

            // paideia-os #1333, paideia-as#1333: scalar SSE float, register-register
            // only. Upper bound: mandatory prefix (1) + REX (1) + 0F (1) + opcode (1)
            // + ModR/M (1) = 5 bytes (ucomiss/comiss have no mandatory prefix, so
            // their real max is 4; 5 stays a safe conservative bound).
            Mnemonic::MovSd
            | Mnemonic::MovSs
            | Mnemonic::AddSd
            | Mnemonic::AddSs
            | Mnemonic::SubSd
            | Mnemonic::SubSs
            | Mnemonic::MulSd
            | Mnemonic::MulSs
            | Mnemonic::DivSd
            | Mnemonic::DivSs
            | Mnemonic::Sqrtsd
            | Mnemonic::Sqrtss
            | Mnemonic::Ucomisd
            | Mnemonic::Ucomiss
            | Mnemonic::Comisd
            | Mnemonic::Comiss
            | Mnemonic::Cvtsi2sd
            | Mnemonic::Cvtsi2ss
            | Mnemonic::Cvttsd2si
            | Mnemonic::Cvttss2si
            | Mnemonic::MovdBitcast { .. }
            | Mnemonic::MovqBitcast { .. } => 5,
        }
    }
}
