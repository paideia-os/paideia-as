//! x86_64 mnemonics targeted by the m9 opt-pass catalog.
//!
//! Extracted from the monolithic `instruction.rs` (issue #1402, umbrella #1399).
//! Public path preserved: `paideia_as_runtime::instruction::Mnemonic`.
//! Impl blocks (arity, required_feature, implicit_reads/writes, estimated_size)
//! live in `mnemonic_tables.rs` alongside the enum.

use super::types::{Cond, IntWidth};

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
    /// Intel CET End Branch 64 (indirect branch target for 64-bit mode).
    /// Phase R15 PA-R15-M4-005 (issue #1021): emits `endbr64` (F3 0F 1E FA). Zero operands.
    Endbr64,
    /// Intel CET End Branch 32 (indirect branch target for 32-bit mode).
    /// Phase R15 PA-R15-M4-005 (issue #1021): emits `endbr32` (F3 0F 1E FB). Zero operands.
    Endbr32,
    /// REP-prefixed STOSQ (store to memory via RCX iterations).
    RepStosq,
    /// REP-prefixed STOSB (store byte AL to [RDI] via RCX iterations).
    /// Zero-arity; operands (AL/RDI/RCX) are implicit. Emits `F3 AA`.
    RepStosb,
    /// REP-prefixed MOVSQ (copy qword [RSI]->[RDI] via RCX iterations).
    /// Zero-arity; operands (RSI/RDI/RCX) are implicit. Emits `F3 48 A5`.
    RepMovsq,
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
    /// Compare with explicit operand width (Phase-N #1248).
    /// Distinct from `Cmp` (which is width-implicit 64-bit) so `cmp al, 0` emits
    /// the 8-bit 3C ib / 80 F8 ib forms rather than the 64-bit 48 83 F8 ib form.
    CmpSized { width: IntWidth },
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
    /// Unsigned wide multiply (64-bit): emits `mul r64` (REX.W F7 /4).
    /// One operand (the source register). Implicit multiplicand is rax;
    /// the 128-bit product lands in rdx:rax (low 64 bits in rax, high 64 in rdx).
    /// paideia-as#1398 (postui unblock): postui#43 32×32-split Fixed64 multiply
    /// needs a real unsigned full 128-bit product; `imul` (signed low-64) and
    /// `div` (unsigned 128÷64) already exist — this closes the arithmetic
    /// substrate for wide-integer software emulation.
    Mul,
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
    /// Invalidate process-context identifier — v0.21-009-followup (#1297).
    /// Emits `invpcid r64, m128` (66 0F 38 82 /r) per Intel SDM Vol 2A.
    /// The register operand carries the INVPCID type (0/1/2/3 in the low
    /// 2 bits of r64); the m128 memory operand supplies a 128-bit
    /// descriptor `[pcid_low12:64][linear_addr:64]`. Two operands
    /// (reg, mem). Ring-0 only (#GP outside CPL 0).
    Invpcid,
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
    /// lock cmpxchg r/m32, r32 (PA-R16-003, #969). Encoding: F0 0F B1 /r (no REX.W).
    /// Compares implicit EAX with r/m32; if equal writes reg32, else loads r/m32
    /// into EAX. Two operands (mem, reg). Reg-reg form not supported.
    LockCmpxchg32,
    /// lock cmpxchg16b m128 (PA-R16-004, #970). Encoding: F0 REX.W 0F C7 /1.
    /// Compares implicit RDX:RAX with the 16-byte memory operand; if equal writes
    /// RCX:RBX to memory (ZF=1), else loads memory into RDX:RAX (ZF=0). One explicit
    /// operand (memory). Implicit register operands: RDX:RAX (expected), RCX:RBX (new).
    /// Requires 16-byte aligned memory operand (unaligned #GP). Requires
    /// CPUID.01H:ECX.CMPXCHG16B[bit 13].
    LockCmpxchg16b,
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
    /// pause (PA-R16-007, #973). Spinloop hint per Intel SDM Vol 2A PAUSE.
    /// Architecturally a NOP; on modern cores reduces power and prevents
    /// memory-ordering violations in spin-wait loops. Encoding: F3 90.
    /// Zero operands.
    Pause,
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
    /// xsaveopt [base + disp] (PA-R15-m4-005, #1022). Optimized save of processor extended state.
    /// Instruction: 0F AE /6 (reg field = 110). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Xsaveopt,
    /// xrstor [base + disp] (PA-R15-m4-005, #1022). Restore processor extended state.
    /// Instruction: 0F AE /5 (reg field = 101). REX.B for r8-r15 base; no REX.W.
    /// One operand (memory).
    Xrstor,
    /// xgetbv (v0.21-015, paideia-as#1294 — blocks paideia-os R21.M1 #826).
    /// Read extended control register: XCR indexed by ECX into EDX:EAX.
    /// Encoding: `0F 01 D0`. Zero explicit operands (implicit ECX / EDX:EAX).
    Xgetbv,
    /// xsetbv (v0.21-015, paideia-as#1294 — blocks paideia-os R21.M1 #826).
    /// Write extended control register: EDX:EAX into XCR indexed by ECX.
    /// Encoding: `0F 01 D1`. Zero explicit operands (implicit ECX / EDX:EAX).
    /// Privileged (ring 0 only); required to gate on x87/SSE/AVX in XCR0 before
    /// any XSAVE/XRSTOR variant is executed.
    Xsetbv,
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
    /// Note: src reg read+write in-place on the explicit operand list — no implicit-clobber slot.
    /// implicit_reads() and implicit_writes() return empty slice for this mnemonic.
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
    /// LOCK-prefixed increment: `lock inc [mem]` (PA-R16-007, #1060).
    /// Atomically increments memory by 1. One operand (mem).
    /// Encoding: `F0 [REX.W] FF /0` per Intel SDM Vol 2A INC.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    /// Supports both W32 and W64 widths.
    LockInc {
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
    /// Population count: `popcnt r32/r64, r/m32/r/m64` (PA-R15-006, #961).
    /// Counts set bits in register/memory. Two operands (dst, src).
    /// Encoding: `F3 [REX.W] 0F B8 /r` per Intel SDM Vol 2B POPCNT.
    /// Requires Nehalem+ CPUID.01H:ECX.POPCNT[bit 23].
    Popcnt {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// CRC32 checksum: `crc32 r64, r/m64` (PA-R15-006, #1005).
    /// Accumulates CRC32 checksum. Two operands (dst, src).
    /// Encoding: `F2 [REX.W] 0F 38 F1 /r` per Intel SDM Vol 2B CRC32.
    /// Requires SSE 4.2 CPUID.01H:ECX.SSE42[bit 20].
    Crc32 {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// Bit scan forward: `bsf r32/r64, r/m32/r/m64` (PA-R16-008, #974).
    /// Returns index of lowest set bit in src. ZF=1 iff src=0 (dst undefined).
    /// Encoding: `[REX.W] 0F BC /r` per Intel SDM Vol 2A BSF (RM form: dst in reg, src in rm).
    Bsf {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Bit scan reverse: `bsr r32/r64, r/m32/r/m64` (PA-R16-008, #974).
    /// Returns index of highest set bit in src. ZF=1 iff src=0 (dst undefined).
    /// Encoding: `[REX.W] 0F BD /r` per Intel SDM Vol 2A BSR (RM form: dst in reg, src in rm).
    Bsr {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Trailing-zero count: `tzcnt r32/r64, r/m32/r/m64` (PA-R16-008, #974).
    /// Returns count of trailing zeros. Defined for src=0 (returns operand width).
    /// Encoding: `F3 [REX.W] 0F BC /r` per Intel SDM Vol 2A TZCNT (RM form).
    /// Requires CPUID.07H:EBX.BMI1[bit 3]. On non-BMI1 CPUs decodes as BSF
    /// (silent semantic drift; feature-gate mechanism tracked in #1033).
    Tzcnt {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Bit test: `bt r/m32/r/m64, r32/r64` (PA-R16-001, #967).
    /// Tests a bit in the bitmap (first operand) at index (second operand).
    /// Sets CF to bit value, OF/SF/AF/PF undefined. Two operands (bitmap, index).
    /// Encoding: `[REX.W] 0F A3 /r` per Intel SDM Vol 2A BT (MR form: index in reg, bitmap in rm).
    Bt {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Bit test and set: `bts r/m32/r/m64, r32/r64` (PA-R16-001, #967).
    /// Tests a bit and sets it to 1. Sets CF to old bit value. Two operands (bitmap, index).
    /// Encoding: `[REX.W] 0F AB /r` per Intel SDM Vol 2A BTS (MR form: index in reg, bitmap in rm).
    Bts {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Bit test and reset: `btr r/m32/r/m64, r32/r64` (PA-R16-001, #967).
    /// Tests a bit and clears it to 0. Sets CF to old bit value. Two operands (bitmap, index).
    /// Encoding: `[REX.W] 0F B3 /r` per Intel SDM Vol 2A BTR (MR form: index in reg, bitmap in rm).
    Btr {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// Bit test and complement: `btc r/m32/r/m64, r32/r64` (PA-R16-001, #967).
    /// Tests a bit and toggles it. Sets CF to old bit value. Two operands (bitmap, index).
    /// Encoding: `[REX.W] 0F BB /r` per Intel SDM Vol 2A BTC (MR form: index in reg, bitmap in rm).
    Btc {
        /// Operand width selecting the encoded form (W32 or W64).
        width: IntWidth,
    },
    /// LOCK-prefixed bit test and set: `lock bts [mem], imm8/r64` (PA-R16-002, #968).
    /// Atomically tests a bit and sets it to 1. Sets CF to old bit value. Two operands (mem, index).
    /// Encoding: `F0 REX.W 0F BA /5 ib` (imm8) or `F0 REX.W 0F AB /r` (reg) per Intel SDM Vol 2A BTS.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockBts {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// LOCK-prefixed bit test and reset: `lock btr [mem], imm8/r64` (PA-R16-002, #968).
    /// Atomically tests a bit and clears it to 0. Sets CF to old bit value. Two operands (mem, index).
    /// Encoding: `F0 REX.W 0F BA /6 ib` (imm8) or `F0 REX.W 0F B3 /r` (reg) per Intel SDM Vol 2A BTR.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockBtr {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// LOCK-prefixed bit test and complement: `lock btc [mem], imm8/r64` (PA-R16-002, #968).
    /// Atomically tests a bit and toggles it. Sets CF to old bit value. Two operands (mem, index).
    /// Encoding: `F0 REX.W 0F BA /7 ib` (imm8) or `F0 REX.W 0F BB /r` (reg) per Intel SDM Vol 2A BTC.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockBtc {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// LOCK-prefixed bitwise AND: `lock and [mem], r64` (PA-R16-006, #972).
    /// Atomically ANDs register into memory. Two operands (mem, src).
    /// Encoding: `F0 REX.W 21 /r` per Intel SDM Vol 2A AND.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockAnd {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// LOCK-prefixed bitwise OR: `lock or [mem], r64` (PA-R16-006, #972).
    /// Encoding: `F0 REX.W 09 /r` per Intel SDM Vol 2A OR.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockOr {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// LOCK-prefixed bitwise XOR: `lock xor [mem], r64` (PA-R16-006, #972).
    /// Encoding: `F0 REX.W 31 /r` per Intel SDM Vol 2A XOR.
    /// Effect: !{Atomic}. LOCK (Group 1) precedes REX per Vol 2A §2.1.1.
    LockXor {
        /// Operand width selecting the encoded form (W64 only).
        width: IntWidth,
    },
    /// Bitwise XOR (256-bit vector): `vpxor ymm dst, ymm src1, ymm src2` (PA-R18-011, #1004).
    /// Encoding: VEX.256 66 0F EF /r per Intel SDM Vol 2B VPXOR.
    /// Three operands (dst, src1, src2).
    Vpxor,
    /// Byte-wise equal comparison (256-bit vector): `vpcmpeqb ymm dst, ymm src1, ymm src2` (PA-R18-011, #1004).
    /// Encoding: VEX.256 66 0F 74 /r per Intel SDM Vol 2B VPCMPEQB.
    /// Three operands (dst, src1, src2).
    Vpcmpeqb,
    /// Move mask from register (256-bit vector): `vpmovmskb r32 dst, ymm src` (PA-R18-011, #1004).
    /// Encoding: VEX.256 66 0F D7 /r per Intel SDM Vol 2B VPMOVMSKB.
    /// Two operands (dst r32, src ymm).
    Vpmovmskb,
    /// Move (unaligned) to/from vector register (256-bit): `vmovdqu ymm dst, ymm/[mem]` or `vmovdqu [mem], ymm` (PA-R18-011, #1004).
    /// Encoding: VEX.256 F3 0F 6F/7F /r per Intel SDM Vol 2B VMOVDQU.
    /// Two operands (dst {ymm|mem}, src {mem|ymm}).
    Vmovdqu {
        /// True for store form ([mem] ← ymm), false for load form (ymm ← [mem] or ymm ← ymm).
        is_store: bool,
    },
    /// Move scalar double-precision: `movsd xmm1, xmm2` (paideia-os #1333, paideia-as#1333).
    /// Encoding: `F2 0F 10 /r` per Intel SDM Vol 2A MOVSD. Register-register only
    /// (memory-operand forms deferred). Two operands (dst xmm, src xmm).
    MovSd,
    /// Move scalar single-precision: `movss xmm1, xmm2` (paideia-as#1333).
    /// Encoding: `F3 0F 10 /r` per Intel SDM Vol 2A MOVSS. Register-register only.
    MovSs,
    /// Scalar double-precision add: `addsd xmm1, xmm2` — `F2 0F 58 /r`.
    AddSd,
    /// Scalar single-precision add: `addss xmm1, xmm2` — `F3 0F 58 /r`.
    AddSs,
    /// Scalar double-precision subtract: `subsd xmm1, xmm2` — `F2 0F 5C /r`.
    SubSd,
    /// Scalar single-precision subtract: `subss xmm1, xmm2` — `F3 0F 5C /r`.
    SubSs,
    /// Scalar double-precision multiply: `mulsd xmm1, xmm2` — `F2 0F 59 /r`.
    MulSd,
    /// Scalar single-precision multiply: `mulss xmm1, xmm2` — `F3 0F 59 /r`.
    MulSs,
    /// Scalar double-precision divide: `divsd xmm1, xmm2` — `F2 0F 5E /r`.
    DivSd,
    /// Scalar single-precision divide: `divss xmm1, xmm2` — `F3 0F 5E /r`.
    DivSs,
    /// Scalar double-precision square root: `sqrtsd xmm1, xmm2` — `F2 0F 51 /r`.
    Sqrtsd,
    /// Scalar single-precision square root: `sqrtss xmm1, xmm2` — `F3 0F 51 /r`.
    Sqrtss,
    /// Unordered scalar double-precision compare: `ucomisd xmm1, xmm2` — `66 0F 2E /r`.
    Ucomisd,
    /// Unordered scalar single-precision compare: `ucomiss xmm1, xmm2` — `0F 2E /r`.
    Ucomiss,
    /// Ordered scalar double-precision compare: `comisd xmm1, xmm2` — `66 0F 2F /r`.
    Comisd,
    /// Ordered scalar single-precision compare: `comiss xmm1, xmm2` — `0F 2F /r`.
    Comiss,
    /// Convert signed int64 to scalar double: `cvtsi2sd xmm1, r64` — `F2 REX.W 0F 2A /r`.
    Cvtsi2sd,
    /// Convert signed int64 to scalar single: `cvtsi2ss xmm1, r64` — `F3 REX.W 0F 2A /r`.
    Cvtsi2ss,
    /// Convert (truncating) scalar double to signed int64: `cvttsd2si r64, xmm1` — `F2 REX.W 0F 2C /r`.
    Cvttsd2si,
    /// Convert (truncating) scalar single to signed int64: `cvttss2si r64, xmm1` — `F3 REX.W 0F 2C /r`.
    Cvttss2si,
    /// Bitcast move between a 32-bit GPR and the low dword of an XMM register
    /// (int/f32 bitcast): `movd xmm, r32` / `movd r32, xmm` — `66 0F 6E/7E /r`.
    MovdBitcast {
        /// True: load into xmm (`66 0F 6E /r`, operands `[xmm dst, r32 src]`).
        /// False: store from xmm (`66 0F 7E /r`, operands `[r32 dst, xmm src]`).
        to_xmm: bool,
    },
    /// Bitcast move between a 64-bit GPR and an XMM register (int/f64 bitcast):
    /// `movq xmm, r64` / `movq r64, xmm` — `66 REX.W 0F 6E/7E /r`.
    MovqBitcast {
        /// True: load into xmm (`66 REX.W 0F 6E /r`, operands `[xmm dst, r64 src]`).
        /// False: store from xmm (`66 REX.W 0F 7E /r`, operands `[r64 dst, xmm src]`).
        to_xmm: bool,
    },
}
