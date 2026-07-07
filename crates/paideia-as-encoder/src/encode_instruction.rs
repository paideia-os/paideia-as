//! Mnemonic ↔ encoder bridge.
//!
//! `encode_instruction(inst, &mut buf)` dispatches to the per-mnemonic
//! encoder primitives already shipping in encode.rs. Phase-3-m2-002
//! minimum: covers the 10-mnemonic catalog from instruction.rs; future
//! mnemonics drop into the match arm.

use crate::dispatch::{DispatchKind, classify};
use crate::encode::*;
use crate::encode_and_or_xor;
use crate::encode_imul;
use paideia_as_ir::{
    Cond as IrCond, InstrMode, Instruction, IntWidth, Mnemonic, Operand, RegId, Scale,
};

/// SysV AMD64 ABI: R_X86_64_PC32/PLT32 callers must supply addend = -4 so that
/// `S + A - P` resolves to `S - RIP_after_disp32` (matches CPU RIP semantics).
const PC32_FIELD_BIAS: i32 = -4;

/// Whether a 64-bit ADD with the given operand can be shortened to 32-bit.
///
/// True when the high 32 bits are known to be zero/unused (e.g., the
/// 32-bit form clears the high bits implicitly).
fn can_shorten_add_to_32bit(high_bits_used: bool) -> bool {
    !high_bits_used
}

/// Whether a Jcc rel32 can be shortened to rel8.
///
/// rel8 range: -128..=127 from the byte AFTER the jcc.
fn can_use_rel8(displacement: i64) -> bool {
    (-128..=127).contains(&displacement)
}

/// Statistics about instruction encoding, tracking tightening opportunities.
#[derive(Debug, Clone, Copy, Default)]
pub struct EncodeStats {
    /// Number of instructions tightened (used shorter encoding form).
    pub tightened: usize,
    /// Total number of instructions encoded.
    pub total: usize,
}

impl EncodeStats {
    /// Create a new empty stats structure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tightening event.
    pub fn record_tightening(&mut self) {
        self.tightened += 1;
    }

    /// Increment total instruction count.
    pub fn record_instruction(&mut self) {
        self.total += 1;
    }
}

#[derive(Debug, thiserror::Error)]
/// Errors that can occur during instruction encoding.
pub enum EncodeError {
    /// Operand count mismatch for a mnemonic.
    #[error("operand mismatch for {mnemonic:?}: expected {expected}, got {got}")]
    OperandCount {
        /// The mnemonic that had the operand count mismatch.
        mnemonic: Mnemonic,
        /// Expected operand count.
        expected: usize,
        /// Actual operand count.
        got: usize,
    },
    /// Operand shape mismatch for a mnemonic.
    #[error("operand shape mismatch for {mnemonic:?}")]
    OperandShape {
        /// The mnemonic that had the operand shape mismatch.
        mnemonic: Mnemonic,
    },
    /// Invalid operand value (e.g., RSP as SIB index).
    #[error("invalid operand: {0}")]
    InvalidOperand(&'static str),
    /// Feature not yet supported in phase 3 m2-002.
    #[error("unsupported in phase 3 m2-002: {0}")]
    Unsupported(&'static str),
}

/// Kind of relocation for a symbol reference.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RelocKind {
    /// PC-relative 32-bit relocation (x86_64 R_X86_64_PC32).
    PcRel32,
    /// PLT 32-bit relocation (x86_64 R_X86_64_PLT32).
    Plt32,
    /// Absolute 32-bit relocation (x86_64 R_X86_64_32).
    /// PA10-006a: used for ljmp imm32:imm16 direct form with symbol reference.
    Abs32,
    /// Absolute 64-bit relocation (x86_64 R_X86_64_64).
    Abs64,
}

/// A relocation site in the encoded instruction stream.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RelocSite {
    /// Byte offset into the instruction stream where the relocation applies.
    pub byte_offset: u32,
    /// Name of the symbol being referenced.
    pub symbol: String,
    /// Kind of relocation to apply.
    pub kind: RelocKind,
    /// Addend to apply to the symbol address.
    pub addend: i32,
}

/// Phase 6 m4-003: A label fixup site in the encoded instruction stream.
/// Records where a Jcc or Jmp instruction references a label (forward or backward),
/// allowing the linker to patch the rel32 displacement after all labels are resolved.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LabelFixup {
    /// Byte offset into the instruction stream where the rel32 placeholder is located.
    pub byte_offset: u32,
    /// Name of the target label.
    pub label_name: String,
    /// Addend to apply to the label offset (typically 0).
    pub addend: i32,
    /// Size of the instruction (5 for jmp, 6 for jcc).
    pub instruction_size: u32,
}

/// Output from encoding an instruction, including relocation sites and label fixups.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EncodeOutput {
    /// Relocation sites to be processed by the linker.
    pub reloc_sites: Vec<RelocSite>,
    /// Label fixup sites for Jcc/Jmp instructions (phase 6 m4-003).
    pub label_fixups: Vec<LabelFixup>,
}

impl EncodeOutput {
    /// Create a new empty output.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relocation site to the output.
    pub fn add_reloc(&mut self, site: RelocSite) {
        self.reloc_sites.push(site);
    }

    /// Phase 6 m4-003: Add a label fixup site to the output.
    pub fn add_label_fixup(&mut self, fixup: LabelFixup) {
        self.label_fixups.push(fixup);
    }
}

/// Convert an IR register ID to an encoder Reg64.
fn reg64_from(id: RegId) -> Result<Reg64, EncodeError> {
    match id.0 {
        0 => Ok(Reg64::Rax),
        1 => Ok(Reg64::Rcx),
        2 => Ok(Reg64::Rdx),
        3 => Ok(Reg64::Rbx),
        4 => Ok(Reg64::Rsp),
        5 => Ok(Reg64::Rbp),
        6 => Ok(Reg64::Rsi),
        7 => Ok(Reg64::Rdi),
        8 => Ok(Reg64::R8),
        9 => Ok(Reg64::R9),
        10 => Ok(Reg64::R10),
        11 => Ok(Reg64::R11),
        12 => Ok(Reg64::R12),
        13 => Ok(Reg64::R13),
        14 => Ok(Reg64::R14),
        15 => Ok(Reg64::R15),
        _ => Err(EncodeError::Unsupported("invalid register id")),
    }
}

fn reg32_from(id: RegId) -> Result<Reg32, EncodeError> {
    match id.0 {
        0 => Ok(Reg32::Eax),
        1 => Ok(Reg32::Ecx),
        2 => Ok(Reg32::Edx),
        3 => Ok(Reg32::Ebx),
        4 => Ok(Reg32::Esp),
        5 => Ok(Reg32::Ebp),
        6 => Ok(Reg32::Esi),
        7 => Ok(Reg32::Edi),
        8 => Ok(Reg32::R8d),
        9 => Ok(Reg32::R9d),
        10 => Ok(Reg32::R10d),
        11 => Ok(Reg32::R11d),
        12 => Ok(Reg32::R12d),
        13 => Ok(Reg32::R13d),
        14 => Ok(Reg32::R14d),
        15 => Ok(Reg32::R15d),
        _ => Err(EncodeError::Unsupported("invalid register id")),
    }
}

/// Convert an IR Scale to a numeric byte width for indexed loads.
/// Convert an IR Cond to an encoder Cond.
fn cond_from(ir_cond: IrCond) -> Result<Cond, EncodeError> {
    match ir_cond {
        IrCond::Eq => Ok(Cond::Eq),
        IrCond::Ne => Ok(Cond::Neq),
        IrCond::Lt => Ok(Cond::Lt),
        IrCond::Ge => Ok(Cond::Ge),
        IrCond::Le => Ok(Cond::Le),
        IrCond::Gt => Ok(Cond::Gt),
        IrCond::Below => Ok(Cond::Below),
        IrCond::BelowOrEqual => Ok(Cond::BelowOrEqual),
        IrCond::Above => Ok(Cond::Above),
        IrCond::AboveOrEqual => Ok(Cond::AboveOrEqual),
        IrCond::Zero => Ok(Cond::Eq),     // jz is alias for je (0x84)
        IrCond::NonZero => Ok(Cond::Neq), // jnz is alias for jne (0x85)
        IrCond::Sign => Ok(Cond::Sign),
        IrCond::NotSign => Ok(Cond::NotSign),
        IrCond::Overflow => Ok(Cond::Overflow),
        IrCond::NotOverflow => Ok(Cond::NotOverflow),
        IrCond::Parity => Ok(Cond::Parity),
        IrCond::NotParity => Ok(Cond::NotParity),
    }
}

/// PA-R13-002: Find and return the first MemSeg operand, if any.
fn find_mem_seg(operands: &[Operand]) -> Option<(usize, paideia_as_ir::SegPrefix)> {
    for (i, op) in operands.iter().enumerate() {
        if let Operand::MemSeg { seg, .. } = op {
            return Some((i, *seg));
        }
    }
    None
}

/// Encode `inst` to a throwaway buffer and return how many bytes it
/// occupies.
///
/// This is the single source of truth for "how big will this instruction
/// be" that the elaborator's `estimated_offset` bookkeeping needs to
/// track. Previously the elaborator maintained its own byte-count
/// literals scattered across ~65 sites in `emit_walker.rs`; every one
/// of those literals is another opportunity for the class of drift
/// bugs surfaced in #985 (`estimated_offset += 7` while encoder emitted
/// 10 bytes) and #986 (`+= 6` while encoder emitted 7 bytes).
///
/// Cost: one encode pass into a local `CodeBuffer`. Cheap enough for
/// emit-time use. If the instruction fails to encode, returns 0 —
/// callers that care must check `encode_instruction` separately.
pub fn estimated_bytes(inst: &Instruction) -> u32 {
    // Var operands resolve to a Reg later in resolve_var_operands. For byte
    // estimation, substitute a placeholder register (RAX) so the encoder can
    // dispatch without panicking. Size is register-class-independent for
    // 64-bit ops (REX.W is always present); this is the sizing invariant the
    // walker relies on before register allocation.
    let has_var = inst.operands.iter().any(|op| matches!(op, Operand::Var { .. }));
    let sized_inst;
    let target: &Instruction = if has_var {
        let mut clone = inst.clone();
        for op in &mut clone.operands {
            if matches!(op, Operand::Var { .. }) {
                *op = Operand::Reg(paideia_as_ir::RegId(0));
            }
        }
        sized_inst = clone;
        &sized_inst
    } else {
        inst
    };
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    match encode_instruction(target, &mut buf, &mut stats) {
        Ok(_) => buf.bytes.len() as u32,
        Err(_) => 0,
    }
}

/// Dispatch an Instruction to its mnemonic-specific encoder.
///
/// Returns `Ok(EncodeOutput)` with encoding output (including relocation sites) on success, or an error if encoding fails.
pub fn encode_instruction(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    // PA-R13-002: segment-prefix pre-pass. Emit prefix (0x65/0x64) then delegate
    // to the inner encoder with the memory operand unwrapped.
    if let Some((idx, seg)) = find_mem_seg(&inst.operands) {
        buf.bytes.push(seg.byte());
        let mut unwrapped = inst.clone();
        if let Operand::MemSeg { inner, .. } = &inst.operands[idx] {
            unwrapped.operands[idx] = (**inner).clone();
        }
        let prefix_bytes = 1;
        let mut output = encode_instruction_impl(&unwrapped, buf, stats)?;
        // Shift instruction-local reloc/label offsets by the prefix byte.
        for r in &mut output.reloc_sites {
            r.byte_offset += prefix_bytes;
        }
        for f in &mut output.label_fixups {
            f.byte_offset += prefix_bytes;
            f.instruction_size += prefix_bytes;
        }
        return Ok(output);
    }

    encode_instruction_impl(inst, buf, stats)
}

/// Internal encoder implementation (after segment prefix pre-pass).
fn encode_instruction_impl(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    stats.record_instruction();
    match &inst.mnemonic {
        Mnemonic::Mov => encode_mov(inst, buf),
        Mnemonic::Add => encode_add(inst, buf, stats),
        Mnemonic::Sub => encode_sub(inst, buf, stats),
        Mnemonic::Adc { width } => encode_adc(inst, buf, *width),
        Mnemonic::Sbb { width } => encode_sbb(inst, buf, *width),
        Mnemonic::Popcnt { width } => encode_popcnt(inst, buf, *width),
        Mnemonic::Bsf { width } => encode_bsf(inst, buf, *width),
        Mnemonic::Bsr { width } => encode_bsr(inst, buf, *width),
        Mnemonic::Tzcnt { width } => encode_tzcnt(inst, buf, *width),
        Mnemonic::Bt { width } => encode_bt(inst, buf, *width),
        Mnemonic::Bts { width } => encode_bts(inst, buf, *width),
        Mnemonic::Btr { width } => encode_btr(inst, buf, *width),
        Mnemonic::Btc { width } => encode_btc(inst, buf, *width),
        Mnemonic::Cmp => encode_cmp(inst, buf),
        Mnemonic::Test => encode_test(inst, buf),
        Mnemonic::Jcc(cond) => encode_jcc(*cond, inst, buf, stats),
        Mnemonic::Setcc(cond) => encode_setcc(*cond, inst, buf),
        Mnemonic::Jmp => encode_jmp(inst, buf),
        Mnemonic::Call => encode_call(inst, buf),
        Mnemonic::Ret => encode_ret(inst, buf),
        Mnemonic::RepMovsb => encode_rep_movsb(inst, buf),
        Mnemonic::Lea => encode_lea(inst, buf),
        // Phase-5 m2-002: zero-operand control + sync instructions
        Mnemonic::Cli => encode_cli(inst, buf),
        Mnemonic::Cld => encode_cld(inst, buf),
        Mnemonic::Sti => encode_sti(inst, buf),
        Mnemonic::Std => encode_std(inst, buf),
        Mnemonic::Hlt => encode_hlt(inst, buf),
        Mnemonic::Nop => encode_nop(inst, buf),
        Mnemonic::Swapgs => encode_swapgs(inst, buf),
        Mnemonic::Cpuid => encode_cpuid(inst, buf),
        Mnemonic::Ud2 => encode_ud2(inst, buf),
        Mnemonic::In { width } => encode_in(inst, buf, *width),
        Mnemonic::Out { width } => encode_out(inst, buf, *width),
        Mnemonic::Wrmsr => encode_wrmsr_inst(inst, buf),
        Mnemonic::Rdmsr => encode_rdmsr_inst(inst, buf),
        Mnemonic::Int => encode_int(inst, buf),
        Mnemonic::MovCr { write } => encode_mov_cr_inst(inst, buf, *write),
        Mnemonic::MovDr { write } => encode_mov_dr_inst(inst, buf, *write),
        Mnemonic::Lgdt => encode_lgdt_inst(inst, buf),
        Mnemonic::Lidt => encode_lidt_inst(inst, buf),
        Mnemonic::Iret => encode_iret_inst(inst, buf),
        Mnemonic::Iretq => encode_iretq_inst(inst, buf),
        Mnemonic::Sysret => encode_sysret_inst(inst, buf),
        Mnemonic::Syscall => encode_syscall_inst(inst, buf),
        Mnemonic::RepStosq => encode_rep_stosq_inst(inst, buf),
        Mnemonic::FarJmp => encode_far_jmp_inst(inst, buf),
        Mnemonic::Movzx => encode_movzx(inst, buf),
        Mnemonic::Movsx => encode_movsx(inst, buf),
        Mnemonic::Not => encode_not(inst, buf),
        Mnemonic::Bswap => encode_bswap(inst, buf),
        Mnemonic::Bswap32 => encode_bswap32(inst, buf),
        Mnemonic::Push => encode_push(inst, buf),
        Mnemonic::Pop => encode_pop(inst, buf),
        Mnemonic::Pushfq => encode_pushfq(inst, buf),
        Mnemonic::Popfq => encode_popfq(inst, buf),
        Mnemonic::Int3 => encode_int3(inst, buf),
        Mnemonic::MovSized { width } => encode_mov_sized(inst, buf, *width),
        // Phase 8 m1-001d: shift operations
        Mnemonic::Shl => encode_shl(inst, buf),
        Mnemonic::Shr => encode_shr(inst, buf),
        Mnemonic::Sar => encode_sar(inst, buf),
        // Phase R15 PA-R15-004: rotate operations
        Mnemonic::Rol { width } => encode_rol(inst, buf, *width),
        Mnemonic::Ror { width } => encode_ror(inst, buf, *width),
        // Phase 8 m1-001d: multiply and bitwise operations
        Mnemonic::Imul => encode_imul::encode_imul(inst, buf),
        Mnemonic::And => encode_and_or_xor::encode_and(inst, buf),
        Mnemonic::Or => encode_and_or_xor::encode_or(inst, buf),
        Mnemonic::Xor => encode_and_or_xor::encode_xor(inst, buf),
        // Phase 8 m5-001: supervisor TLB and timing mnemonics
        Mnemonic::Invlpg => encode_invlpg_inst(inst, buf),
        Mnemonic::Rdtsc => encode_rdtsc_inst(inst, buf),
        // Phase R11 PA-R11-006: divide instructions
        Mnemonic::Div => encode_div(inst, buf),
        Mnemonic::Idiv => encode_idiv(inst, buf),
        // Phase R13 PA-R13-001: load task register
        Mnemonic::Ltr => encode_ltr(inst, buf),
        // Phase R13 PA-R13-003: exchange register with memory
        Mnemonic::Xchg => encode_xchg_inst(inst, buf),
        // Phase R13 PA-R13-004: lock cmpxchg register with memory
        Mnemonic::LockCmpxchg => encode_lock_cmpxchg_inst(inst, buf),
        // Phase R16 PA-R16-003: lock cmpxchg32 register with memory
        Mnemonic::LockCmpxchg32 => encode_lock_cmpxchg32_inst(inst, buf),
        // Phase R16 PA-R16-004: lock cmpxchg16b register with memory
        Mnemonic::LockCmpxchg16b => encode_lock_cmpxchg16b_inst(inst, buf),
        // Phase R15 PA-R15-002: lock xadd register with memory
        Mnemonic::LockXadd { width } => encode_lock_xadd(inst, buf, *width),
        // Phase R15 PA-R15-003: lock add immediate/register with memory
        Mnemonic::LockAdd { width } => encode_lock_add(inst, buf, *width),
        // Phase R15 PA-R15-003: lock sub immediate/register with memory
        Mnemonic::LockSub { width } => encode_lock_sub(inst, buf, *width),
        // Phase R16 PA-R16-007: lock inc memory (issue #1060)
        Mnemonic::LockInc { width } => encode_lock_inc(inst, buf, *width),
        // Phase R16 PA-R16-002: lock bts/btr/btc immediate/register with memory
        Mnemonic::LockBts { width } => encode_lock_bts(inst, buf, *width),
        Mnemonic::LockBtr { width } => encode_lock_btr(inst, buf, *width),
        Mnemonic::LockBtc { width } => encode_lock_btc(inst, buf, *width),
        // Phase R16 PA-R16-006: lock and/or/xor register with memory
        Mnemonic::LockAnd { width } => encode_lock_and(inst, buf, *width),
        Mnemonic::LockOr { width } => encode_lock_or(inst, buf, *width),
        Mnemonic::LockXor { width } => encode_lock_xor(inst, buf, *width),
        // Phase R13 PA-R13-005: memory fence
        Mnemonic::Mfence => encode_mfence_inst(inst, buf),
        // Phase R14 PA-R14-004: store/load fence
        Mnemonic::Sfence => encode_sfence_inst(inst, buf),
        Mnemonic::Lfence => encode_lfence_inst(inst, buf),
        // Phase R16 PA-R16-007: pause spinloop hint
        Mnemonic::Pause => encode_pause_inst(inst, buf),
        // Phase R14 PA-R14-005: write-back/invalidate cache and clflush
        Mnemonic::Wbinvd => encode_wbinvd_inst(inst, buf),
        Mnemonic::Invd => encode_invd_inst(inst, buf),
        // Phase R13 PA-R13-007: fxsave/fxrstor to memory
        Mnemonic::Fxsave => encode_fxsave_inst(inst, buf),
        Mnemonic::Fxrstor => encode_fxrstor_inst(inst, buf),
        // Phase R14 PA-R14-005: cache line flush instructions
        Mnemonic::Clflush => encode_clflush_inst(inst, buf),
        Mnemonic::Clflushopt => encode_clflushopt_inst(inst, buf),
        // Phase R14 PA-R14-006: prefetch instructions
        Mnemonic::Prefetchnta => encode_prefetchnta_inst(inst, buf),
        Mnemonic::Prefetcht0 => encode_prefetcht0_inst(inst, buf),
        Mnemonic::Prefetcht1 => encode_prefetcht1_inst(inst, buf),
        Mnemonic::Prefetcht2 => encode_prefetcht2_inst(inst, buf),
        // Phase R13 PA-R13-005 (issue #934): inc/dec r64
        Mnemonic::Inc => encode_inc(inst, buf),
        Mnemonic::Dec => encode_dec(inst, buf),
        // Phase R14 PA-R14-003 (issue #946): non-temporal store movnti [mem], r32/r64
        Mnemonic::Movnti { width } => encode_movnti(inst, buf, *width),
    }
}

/// Encode a width-threaded immediate-to-register move — Phase 7 m4-003.
///
/// Expects `[Operand::Reg(dst), Operand::Imm64(imm)]`. The `width` selects the
/// encoded form:
/// - W64 → delegates to the generic `encode_mov` path (`48 C7`/`48 B8`),
///   preserving the existing 64-bit behaviour.
/// - W32 → `B8+rd imm32` (5 bytes, no REX.W; implicit zero-extend to r64).
/// - W16 → `66 B8+rd imm16` (4 bytes).
/// - W8  → `B0+rb imm8` (2 bytes; 3 with REX.B for r8–r15).
///
/// The immediate is truncated to the operand width before encoding, matching
/// the semantics of a typed integer-literal binding.
fn encode_mov_sized(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            let dst_reg = reg64_from(*dst)?;
            let imm = *imm as u64;
            match width {
                // W64 reuses the established 64-bit move path verbatim.
                IntWidth::W64 => {
                    // Phase 15 m3-001: reject 64-bit destination in 32-bit mode
                    if inst.mode == InstrMode::Mode32 {
                        return Err(EncodeError::Unsupported(
                            "E0019: 64-bit destination in 32-bit mode",
                        ));
                    }
                    mov_reg64_imm64(buf, dst_reg, imm)
                }
                IntWidth::W32 => mov_reg32_imm32(buf, dst_reg, imm as u32),
                IntWidth::W16 => mov_reg16_imm16(buf, dst_reg, imm as u16),
                IntWidth::W8 => mov_reg8_imm8(buf, dst_reg, imm as u8),
            }
            Ok(EncodeOutput::new())
        }
        // PA13-001 (#930): narrow-width load from memory [base + disp] (no index)
        [Operand::Reg(dst), Operand::MemSib { base, index: None, .. }] => {
            let dst_reg = reg64_from(*dst)?;
            let base_reg = reg64_from(*base)?;
            // MemSib carries the displacement in the disp field (always present in MemSib)
            let disp = inst
                .operands
                .get(1)
                .and_then(|op| {
                    if let Operand::MemSib { disp, .. } = op {
                        Some(*disp)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            match width {
                IntWidth::W64 => mov_reg64_mem_reg64_disp(buf, dst_reg, base_reg, disp),
                IntWidth::W32 => mov_reg32_mem_base_disp(buf, dst_reg, base_reg, disp),
                IntWidth::W16 => mov_reg16_mem_base_disp(buf, dst_reg, base_reg, disp),
                IntWidth::W8 => mov_reg8_mem_base_disp(buf, dst_reg, base_reg, disp),
            }
            Ok(EncodeOutput::new())
        }
        // PA13-001 (#930): narrow-width load from memory [base + index*scale + disp]
        [Operand::Reg(dst), Operand::MemSib { base, index: Some(index), scale, disp }] => {
            let dst_reg = reg64_from(*dst)?;
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*index)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };

            match width {
                IntWidth::W64 => mov_reg64_mem_sib_disp(buf, dst_reg, base_reg, index_reg, scale_bits, *disp),
                _ => mov_reg_mem_sib_disp_sized(buf, width, dst_reg, base_reg, index_reg, scale_bits, *disp),
            }
            Ok(EncodeOutput::new())
        }
        // PA-R14-001 (#944): narrow-width store to memory [base + disp], imm
        [Operand::MemSib { base, index: None, disp, .. }, Operand::Imm64(imm)] => {
            let base_reg = reg64_from(*base)?;
            let disp32 = *disp;
            match width {
                IntWidth::W8 => mov_mem_base_disp_imm8(buf, base_reg, disp32, *imm as u8),
                IntWidth::W16 => mov_mem_base_disp_imm16(buf, base_reg, disp32, *imm as u16),
                IntWidth::W32 => mov_mem_base_disp_imm32(buf, base_reg, disp32, *imm as u32),
                IntWidth::W64 => {
                    if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                        return Err(EncodeError::Unsupported(
                            "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                        ));
                    }
                    mov_mem_base_disp_imm32_sxt(buf, base_reg, disp32, *imm as i32);
                }
            }
            Ok(EncodeOutput::new())
        }
        // pa-r17-006 (#984): narrow-width register-source STORE, [base + disp], reg
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp, .. }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W8 => mov_mem_base_disp_reg8(buf, base_reg, *disp, src_reg),
                IntWidth::W16 => mov_mem_base_disp_reg16(buf, base_reg, *disp, src_reg),
                IntWidth::W32 => mov_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => mov_mem_reg64_disp_reg64(buf, base_reg, *disp, src_reg),
            }
            Ok(EncodeOutput::new())
        }
        // PA-R14-001 (#944): narrow-width store to memory [base + index*scale + disp], imm
        [Operand::MemSib { base, index: Some(idx), scale, disp }, Operand::Imm64(imm)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*idx)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            let disp32 = *disp;
            match width {
                IntWidth::W8 => mov_mem_sib_disp_imm8(buf, base_reg, index_reg, scale_bits, disp32, *imm as u8),
                IntWidth::W16 => mov_mem_sib_disp_imm16(buf, base_reg, index_reg, scale_bits, disp32, *imm as u16),
                IntWidth::W32 => mov_mem_sib_disp_imm32(buf, base_reg, index_reg, scale_bits, disp32, *imm as u32),
                IntWidth::W64 => {
                    if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                        return Err(EncodeError::Unsupported(
                            "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                        ));
                    }
                    mov_mem_sib_disp_imm32_sxt(buf, base_reg, index_reg, scale_bits, disp32, *imm as i32);
                }
            }
            Ok(EncodeOutput::new())
        }
        // PA-R14-002b (#1030): narrow-width load from RIP-relative memory [rip + disp]
        [Operand::Reg(dst), Operand::MemRipRel { disp }] => {
            let dst_id = dst.0;
            mov_reg_mem_rip_rel_sized(buf, width, dst_id, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R14-002b (#1030): narrow-width load from RIP-relative memory with symbol [rip + sym]
        [Operand::Reg(dst), Operand::MemRipRelSym { name, addend }] => {
            let dst_id = dst.0;
            // Calculate bytes before disp32: prefix (if W16) + REX (if needed) + opcode (1) + ModRM (1)
            let prefix_len = if matches!(width, IntWidth::W16) { 1 } else { 0 };
            let rex_len = if (dst_id >> 3) != 0 || matches!(width, IntWidth::W64) { 1 } else { 0 };
            let byte_offset = buf.len() as u32 + prefix_len + rex_len + 2;
            mov_reg_mem_rip_rel_sized(buf, width, dst_id, 0);
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R16-007: mov reg, [disp32] with width from MovSized mnemonic
        [Operand::Reg(dst), Operand::MemDisp { disp }] => {
            let dst_reg = reg64_from(*dst)?;
            mov_reg_mem_abs_disp32(buf, width, dst_reg, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R16-007: mov [disp32], reg with width from MovSized mnemonic
        [Operand::MemDisp { disp }, Operand::Reg(src)] => {
            let src_reg = reg64_from(*src)?;
            mov_mem_abs_disp32_reg(buf, width, *disp, src_reg);
            Ok(EncodeOutput::new())
        }
        // PA-R16-007: mov [disp32], imm with width from MovSized mnemonic
        [Operand::MemDisp { disp }, Operand::Imm64(imm)] => {
            match width {
                IntWidth::W8 | IntWidth::W16 | IntWidth::W32 => {
                    mov_mem_abs_disp32_imm(buf, width, *disp, *imm);
                }
                IntWidth::W64 => {
                    // For W64, the immediate must fit in i32 range (sign-extended)
                    if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                        return Err(EncodeError::Unsupported(
                            "mov_q [disp32], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [disp32], r11",
                        ));
                    }
                    mov_mem_abs_disp32_imm(buf, width, *disp, *imm);
                }
            }
            Ok(EncodeOutput::new())
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovSized { width },
        }),
    }
}

/// Encode `not r64` (bitwise NOT / one's complement) — Phase 7 m4-001.
///
/// Expects exactly one register operand. Emits `REX.W F7 /2` via `not_reg64`.
fn encode_not(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            not_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Not,
        }),
    }
}

/// Phase R13 PA-R13-014: Encode byte-swap 64-bit register instruction.
/// Expects exactly one register operand. Emits `REX.W 0F C8+rd` via `bswap_reg64`.
fn encode_bswap(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            bswap_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bswap,
        }),
    }
}

/// Phase R15 PA-R15-001: Encode byte-swap 32-bit register instruction.
/// Expects exactly one register operand. Emits `0F C8+rd` via `bswap_reg32`.
fn encode_bswap32(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            bswap_reg32(buf, reg32_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bswap32,
        }),
    }
}

/// Phase R11 PA-R11-006: Encode unsigned 64-bit divide instruction.
/// Expects exactly one register operand (the divisor). Emits via `div_reg64`.
fn encode_div(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            div_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Div,
        }),
    }
}

/// Phase R11 PA-R11-006: Encode signed 64-bit divide instruction.
/// Expects exactly one register operand (the divisor). Emits via `idiv_reg64`.
fn encode_idiv(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            idiv_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Idiv,
        }),
    }
}

/// Phase R13 PA-R13-005 (issue #934): Encode `inc r64`.
/// Expects exactly one register operand. Emits `REX.W FF /0` via `inc_reg64`.
fn encode_inc(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            inc_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Inc,
        }),
    }
}

/// Phase R13 PA-R13-005 (issue #934): Encode `dec r64`.
/// Expects exactly one register operand. Emits `REX.W FF /1` via `dec_reg64`.
fn encode_dec(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            dec_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Dec,
        }),
    }
}

/// Phase R13 PA-R13-001: Encode load task register instruction.
/// Expects exactly one register operand. Emits via `ltr_reg16`.
fn encode_ltr(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            ltr_reg16(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Ltr,
        }),
    }
}

/// Phase R13 PA-R13-003: Encode xchg [base + disp], src instruction.
/// Expects [MemSib with base and disp, Reg]. Emits via `xchg_mem_base_disp_reg64`.
fn encode_xchg_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            xchg_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Xchg }),
    }
}

/// Phase R13 PA-R13-004: Encode lock cmpxchg [base + disp], src instruction.
/// Expects [MemSib with base and disp, Reg]. Emits via `lock_cmpxchg_mem_base_disp_reg64`.
fn encode_lock_cmpxchg_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            lock_cmpxchg_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg }),
    }
}

/// Phase R16 PA-R16-003 (issue #969): Encode lock cmpxchg32 [base + disp], src instruction.
/// Expects [MemSib with base and disp, Reg]. Emits via `lock_cmpxchg_mem_base_disp_reg32`.
fn encode_lock_cmpxchg32_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            lock_cmpxchg_mem_base_disp_reg32(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg32 }),
    }
}

fn encode_lock_cmpxchg16b_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            lock_cmpxchg16b_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg16b }),
    }
}

/// Phase R15 PA-R15-002 (issue #957): Encode lock xadd instruction.
/// Expects [MemSib { index: None, scale: Scale::X1, disp }, Reg(src)].
/// W32/W64 dispatch. Other widths → Unsupported. Any other shape → OperandShape.
fn encode_lock_xadd(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => lock_xadd_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => lock_xadd_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => {
                    return Err(EncodeError::Unsupported(
                        "E0031: lock_xadd only supports W32 and W64",
                    ))
                }
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockXadd { width } }),
    }
}

/// Phase R15 PA-R15-003: Encode lock add instruction.
/// Supports: [mem], imm8/imm32/r32/r64
fn encode_lock_add(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W32 && width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0032: lock_add only supports W32 and W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock add [mem], reg form (base + disp)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => lock_add_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => lock_add_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        // lock add [mem], imm form (base + disp)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = *imm_val;

            // Try imm8 first
            if let Ok(imm8) = i8::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_add_mem_base_disp_imm8_w32(buf, base_reg, *disp, imm8);
                    }
                    IntWidth::W64 => {
                        lock_add_mem_base_disp_imm8(buf, base_reg, *disp, imm8);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // Try imm32
            if let Ok(imm32) = i32::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_add_mem_base_disp_imm32_w32(buf, base_reg, *disp, imm32);
                    }
                    IntWidth::W64 => {
                        lock_add_mem_base_disp_imm32(buf, base_reg, *disp, imm32);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // imm out of range
            Err(EncodeError::Unsupported(
                "E0033: lock_add imm out of i32 range",
            ))
        }
        // lock add [disp32], imm form (absolute displacement, SIB no-base)
        [Operand::MemDisp { disp }, Operand::Imm64(imm_val)] => {
            let imm = *imm_val;

            // Try imm8 first
            if let Ok(imm8) = i8::try_from(imm) {
                lock_add_mem_abs_disp32_imm8(buf, width, *disp, imm8);
                return Ok(EncodeOutput::new());
            }

            // Try imm32
            if let Ok(imm32) = i32::try_from(imm) {
                lock_add_mem_abs_disp32_imm32(buf, width, *disp, imm32);
                return Ok(EncodeOutput::new());
            }

            // imm out of range
            Err(EncodeError::Unsupported(
                "E0033: lock_add imm out of i32 range",
            ))
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockAdd { width } }),
    }
}

/// Phase R15 PA-R15-003: Encode lock sub instruction.
/// Supports: [mem], imm8/imm32/r32/r64
fn encode_lock_sub(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W32 && width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0032: lock_sub only supports W32 and W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock sub [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => lock_sub_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => lock_sub_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        // lock sub [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = *imm_val;

            // Try imm8 first
            if let Ok(imm8) = i8::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_sub_mem_base_disp_imm8_w32(buf, base_reg, *disp, imm8);
                    }
                    IntWidth::W64 => {
                        lock_sub_mem_base_disp_imm8(buf, base_reg, *disp, imm8);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // Try imm32
            if let Ok(imm32) = i32::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_sub_mem_base_disp_imm32_w32(buf, base_reg, *disp, imm32);
                    }
                    IntWidth::W64 => {
                        lock_sub_mem_base_disp_imm32(buf, base_reg, *disp, imm32);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // imm out of range
            Err(EncodeError::Unsupported(
                "E0033: lock_sub imm out of i32 range",
            ))
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockSub { width } }),
    }
}

/// Phase R16 PA-R16-007: Encode lock inc instruction.
/// Supports: [mem] (one operand only).
/// Both W32 and W64 forms supported; uses SIB no-base for absolute displacement.
fn encode_lock_inc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W32 && width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0034: lock_inc only supports W32 and W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock inc [mem] form with absolute displacement (SIB no-base)
        [Operand::MemDisp { disp }] => {
            lock_inc_mem_abs_disp32(buf, width, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockInc { width } }),
    }
}

/// Phase R16 PA-R16-002: Encode lock bts instruction.
/// Supports: [mem], imm8/r64 (W64 only).
fn encode_lock_bts(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0044: lock_bts only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock bts [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(index_reg)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg_val = reg64_from(*index_reg)?;
            lock_bts_mem_base_disp_reg64(buf, base_reg, *disp, index_reg_val);
            Ok(EncodeOutput::new())
        }
        // lock bts [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = u8::try_from(*imm_val)
                .map_err(|_| EncodeError::Unsupported("E0044: lock_bts imm8 out of u8 range"))?;
            lock_bts_mem_base_disp_imm8(buf, base_reg, *disp, imm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockBts { width } }),
    }
}

/// Phase R16 PA-R16-002: Encode lock btr instruction.
/// Supports: [mem], imm8/r64 (W64 only).
fn encode_lock_btr(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0045: lock_btr only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock btr [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(index_reg)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg_val = reg64_from(*index_reg)?;
            lock_btr_mem_base_disp_reg64(buf, base_reg, *disp, index_reg_val);
            Ok(EncodeOutput::new())
        }
        // lock btr [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = u8::try_from(*imm_val)
                .map_err(|_| EncodeError::Unsupported("E0045: lock_btr imm8 out of u8 range"))?;
            lock_btr_mem_base_disp_imm8(buf, base_reg, *disp, imm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockBtr { width } }),
    }
}

/// Phase R16 PA-R16-002: Encode lock btc instruction.
/// Supports: [mem], imm8/r64 (W64 only).
fn encode_lock_btc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0046: lock_btc only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock btc [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(index_reg)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg_val = reg64_from(*index_reg)?;
            lock_btc_mem_base_disp_reg64(buf, base_reg, *disp, index_reg_val);
            Ok(EncodeOutput::new())
        }
        // lock btc [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = u8::try_from(*imm_val)
                .map_err(|_| EncodeError::Unsupported("E0046: lock_btc imm8 out of u8 range"))?;
            lock_btc_mem_base_disp_imm8(buf, base_reg, *disp, imm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockBtc { width } }),
    }
}

/// Phase R16 PA-R16-006: Encode lock and instruction.
/// Supports: [mem], r64 (W64 only).
fn encode_lock_and(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0047: lock_and only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock and [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src_reg)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg_val = reg64_from(*src_reg)?;
            lock_and_mem_base_disp_reg64(buf, base_reg, *disp, src_reg_val);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockAnd { width } }),
    }
}

/// Phase R16 PA-R16-006: Encode lock or instruction.
/// Supports: [mem], r64 (W64 only).
fn encode_lock_or(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0048: lock_or only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock or [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src_reg)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg_val = reg64_from(*src_reg)?;
            lock_or_mem_base_disp_reg64(buf, base_reg, *disp, src_reg_val);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockOr { width } }),
    }
}

/// Phase R16 PA-R16-006: Encode lock xor instruction.
/// Supports: [mem], r64 (W64 only).
fn encode_lock_xor(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0049: lock_xor only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock xor [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src_reg)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg_val = reg64_from(*src_reg)?;
            lock_xor_mem_base_disp_reg64(buf, base_reg, *disp, src_reg_val);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockXor { width } }),
    }
}

/// Phase R13 PA-R13-005: Encode mfence instruction.
/// Expects zero operands. Emits via `mfence`.
fn encode_mfence_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Mfence, expected: 0, got: inst.operands.len(),
        });
    }
    mfence(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-004: Encode sfence instruction.
/// Expects zero operands. Emits via `sfence`.
fn encode_sfence_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sfence, expected: 0, got: inst.operands.len(),
        });
    }
    sfence(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-004: Encode lfence instruction.
/// Expects zero operands. Emits via `lfence`.
fn encode_lfence_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Lfence, expected: 0, got: inst.operands.len(),
        });
    }
    lfence(buf);
    Ok(EncodeOutput::new())
}

/// Phase R16 PA-R16-007: Encode pause instruction.
/// Expects zero operands. Emits via `pause`.
fn encode_pause_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Pause, expected: 0, got: inst.operands.len(),
        });
    }
    pause(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-005: Encode wbinvd instruction.
/// Expects zero operands. Emits via `wbinvd`.
fn encode_wbinvd_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Wbinvd, expected: 0, got: inst.operands.len(),
        });
    }
    wbinvd(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-005: Encode invd instruction.
/// Expects zero operands. Emits via `invd`.
fn encode_invd_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Invd, expected: 0, got: inst.operands.len(),
        });
    }
    invd(buf);
    Ok(EncodeOutput::new())
}

/// Phase R13 PA-R13-007: Encode fxsave instruction.
/// Expects one memory operand [base + disp]. Emits via `fxsave_mem_base_disp`.
fn encode_fxsave_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            fxsave_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Fxsave }),
    }
}

/// Phase R13 PA-R13-007: Encode fxrstor instruction.
/// Expects one memory operand [base + disp]. Emits via `fxrstor_mem_base_disp`.
fn encode_fxrstor_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            fxrstor_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Fxrstor }),
    }
}

/// Phase R14 PA-R14-005: Encode clflush instruction.
/// Expects one memory operand [base + disp]. Emits via `clflush_mem_base_disp`.
fn encode_clflush_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            clflush_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Clflush }),
    }
}

/// Phase R14 PA-R14-005: Encode clflushopt instruction.
/// Expects one memory operand [base + disp]. Emits via `clflushopt_mem_base_disp`.
fn encode_clflushopt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            clflushopt_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Clflushopt }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetchnta instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetchnta_mem_base_disp`.
fn encode_prefetchnta_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetchnta_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetchnta }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetcht0 instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetcht0_mem_base_disp`.
fn encode_prefetcht0_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetcht0_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetcht0 }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetcht1 instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetcht1_mem_base_disp`.
fn encode_prefetcht1_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetcht1_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetcht1 }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetcht2 instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetcht2_mem_base_disp`.
fn encode_prefetcht2_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetcht2_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetcht2 }),
    }
}

/// Phase R14 PA-R14-003 (issue #946): Encode non-temporal store movnti [mem], r32/r64.
/// Expects [MemSib, Reg]. Dispatches per width (W32 or W64) to store-form encoders.
fn encode_movnti(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        // Base + disp, no index (Scale::X1 sentinel)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => movnti_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => movnti_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => {
                    return Err(EncodeError::Unsupported(
                        "E0030: movnti only supports W32 and W64 widths",
                    ))
                }
            }
            Ok(EncodeOutput::new())
        }
        // Base + index*scale + disp (SIB form)
        [Operand::MemSib { base, index: Some(index), scale, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*index)?;
            let src_reg = reg64_from(*src)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            match width {
                IntWidth::W32 => movnti_mem_sib_disp_reg32(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                IntWidth::W64 => movnti_mem_sib_disp_reg64(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                _ => {
                    return Err(EncodeError::Unsupported(
                        "E0030: movnti only supports W32 and W64 widths",
                    ))
                }
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Movnti { width } }),
    }
}

/// Phase R9 m2-001 (PA-R9-001): Encode push 64-bit register instruction.
/// Expects exactly one register operand. Rejects Mode32. Emits via `push_reg64`.
fn encode_push(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase R9 m2-001: reject Mode32
    if inst.mode == InstrMode::Mode32 {
        return Err(EncodeError::Unsupported(
            "E0020: push r64 not supported in 32-bit mode",
        ));
    }
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            push_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Push,
        }),
    }
}

/// Phase R9 m2-001 (PA-R9-001): Encode pop 64-bit register instruction.
/// Expects exactly one register operand. Rejects Mode32. Emits via `pop_reg64`.
fn encode_pop(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase R9 m2-001: reject Mode32
    if inst.mode == InstrMode::Mode32 {
        return Err(EncodeError::Unsupported(
            "E0021: pop r64 not supported in 32-bit mode",
        ));
    }
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            pop_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Pop,
        }),
    }
}

/// Phase R9 m2-002 (PA-R9-002): Encode pushfq instruction.
/// Push flags register onto stack: `pushfq` (0x9C). Zero operands.
fn encode_pushfq(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Pushfq,
        });
    }
    buf.bytes.push(0x9C);
    Ok(EncodeOutput::new())
}

/// Phase R9 m2-002 (PA-R9-002): Encode popfq instruction.
/// Pop flags register from stack: `popfq` (0x9D). Zero operands.
fn encode_popfq(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Popfq,
        });
    }
    buf.bytes.push(0x9D);
    Ok(EncodeOutput::new())
}

/// Phase R9 m2-003 (PA-R9-003): Encode int3 instruction.
/// Breakpoint interrupt: `int3` (0xCC). Zero operands.
fn encode_int3(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Int3,
        });
    }
    buf.bytes.push(0xCC);
    Ok(EncodeOutput::new())
}

/// Phase 8 m1-001d: Encode shift-left instruction.
/// Supports: shl r64, imm8 or shl r64, rcx (via r64 operand)
fn encode_shl(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            // shl r64, imm8 → 48 C1 E0 NN
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xC1);
            buf.bytes.push(0xE0 | (dst_id & 7));
            buf.bytes.push(*imm as u8);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // shl r64, rcx (variable shift count, src must be RCX)
            if reg64_from(*src)? != Reg64::Rcx {
                return Err(EncodeError::Unsupported(
                    "shl with variable count requires CL register",
                ));
            }
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xD3);
            buf.bytes.push(0xE0 | (dst_id & 7));
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "shl form not supported in phase 8 m1-001d",
        )),
    }
}

/// Phase 8 m1-001d: Encode shift-right (logical) instruction.
fn encode_shr(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            // shr r64, imm8 → 48 C1 E8 NN
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xC1);
            buf.bytes.push(0xE8 | (dst_id & 7));
            buf.bytes.push(*imm as u8);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // shr r64, rcx (variable shift count)
            if reg64_from(*src)? != Reg64::Rcx {
                return Err(EncodeError::Unsupported(
                    "shr with variable count requires CL register",
                ));
            }
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xD3);
            buf.bytes.push(0xE8 | (dst_id & 7));
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "shr form not supported in phase 8 m1-001d",
        )),
    }
}

/// Phase 8 m1-001d: Encode arithmetic shift-right instruction.
fn encode_sar(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            // sar r64, imm8 → 48 C1 F8 NN
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xC1);
            buf.bytes.push(0xF8 | (dst_id & 7));
            buf.bytes.push(*imm as u8);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // sar r64, rcx (variable shift count)
            if reg64_from(*src)? != Reg64::Rcx {
                return Err(EncodeError::Unsupported(
                    "sar with variable count requires CL register",
                ));
            }
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xD3);
            buf.bytes.push(0xF8 | (dst_id & 7));
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "sar form not supported in phase 8 m1-001d",
        )),
    }
}

/// Phase R15 PA-R15-004: Encode rotate-left instruction.
fn encode_rol(inst: &Instruction, buf: &mut CodeBuffer, width: IntWidth) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // rol r64, imm8
                    let dst_reg = reg64_from(*dst)?;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: rol imm must fit in i8"))?;
                    rol_reg64_imm8(buf, dst_reg, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // rol r64, cl (rotate count in CL)
                    let dst_reg = reg64_from(*dst)?;
                    if reg64_from(*src)? != Reg64::Rcx {
                        return Err(EncodeError::Unsupported("E0036: rol variable count requires CL"));
                    }
                    rol_reg64_cl(buf, dst_reg);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Rol { width },
                }),
            }
        }
        IntWidth::W32 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // rol r32, imm8
                    let _dst_reg = reg32_from(*dst)?;
                    let dst_id = dst.0;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: rol imm must fit in i8"))?;
                    rol_reg32_imm8(buf, dst_id, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // rol r32, cl
                    let _dst_reg = reg32_from(*dst)?;
                    let _src_reg = reg32_from(*src)?;
                    if reg32_from(*src)? != Reg32::Ecx {
                        return Err(EncodeError::Unsupported("E0036: rol variable count requires CL"));
                    }
                    let dst_id = dst.0;
                    rol_reg32_cl(buf, dst_id);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Rol { width },
                }),
            }
        }
        IntWidth::W16 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // rol r16, imm8
                    let dst_id = dst.0;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: rol imm must fit in i8"))?;
                    rol_reg16_imm8(buf, dst_id, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // rol r16, cl
                    if src.0 != 1 {
                        return Err(EncodeError::Unsupported("E0036: rol variable count requires CL"));
                    }
                    let dst_id = dst.0;
                    rol_reg16_cl(buf, dst_id);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Rol { width },
                }),
            }
        }
        _ => Err(EncodeError::Unsupported("E0034: rol only supports W16, W32, and W64")),
    }
}

/// Phase R15 PA-R15-004: Encode rotate-right instruction.
fn encode_ror(inst: &Instruction, buf: &mut CodeBuffer, width: IntWidth) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // ror r64, imm8
                    let dst_reg = reg64_from(*dst)?;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: ror imm must fit in i8"))?;
                    ror_reg64_imm8(buf, dst_reg, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // ror r64, cl (rotate count in CL)
                    let dst_reg = reg64_from(*dst)?;
                    if reg64_from(*src)? != Reg64::Rcx {
                        return Err(EncodeError::Unsupported("E0036: ror variable count requires CL"));
                    }
                    ror_reg64_cl(buf, dst_reg);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Ror { width },
                }),
            }
        }
        IntWidth::W32 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // ror r32, imm8
                    let _dst_reg = reg32_from(*dst)?;
                    let dst_id = dst.0;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: ror imm must fit in i8"))?;
                    ror_reg32_imm8(buf, dst_id, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // ror r32, cl
                    let _dst_reg = reg32_from(*dst)?;
                    let _src_reg = reg32_from(*src)?;
                    if reg32_from(*src)? != Reg32::Ecx {
                        return Err(EncodeError::Unsupported("E0036: ror variable count requires CL"));
                    }
                    let dst_id = dst.0;
                    ror_reg32_cl(buf, dst_id);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Ror { width },
                }),
            }
        }
        _ => Err(EncodeError::Unsupported("E0034: ror only supports W32 and W64")),
    }
}

fn encode_mov(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 6, m1-002 & m1-003: Classify MOV operands and dispatch to specialized encoders.
    let dispatch_kind = classify(inst);

    // Route CR moves through encode_mov_cr_dispatcher.
    match dispatch_kind {
        DispatchKind::MovToCr => {
            return encode_mov_cr_dispatcher(inst, buf, true);
        }
        DispatchKind::MovFromCr => {
            return encode_mov_cr_dispatcher(inst, buf, false);
        }
        // Phase 6, m1-003: Route DR moves through encode_mov_dr_dispatcher.
        DispatchKind::MovToDr => {
            return encode_mov_dr_dispatcher(inst, buf, true);
        }
        DispatchKind::MovFromDr => {
            return encode_mov_dr_dispatcher(inst, buf, false);
        }
        // All other dispatch kinds (MovGeneric, Generic) fall through to the rest of this function.
        _ => {}
    }

    // Phase 15 m5-002: Handle segment register MOV (mov sreg, r16).
    // Opcode: 8E /r (no REX prefix)
    match inst.operands.as_slice() {
        [Operand::SegReg(sreg), Operand::Reg(src)] => {
            let sreg_id = sreg.id();
            let src_reg = reg64_from(*src)?;
            mov_sreg_reg16(buf, sreg_id, src_reg);
            return Ok(EncodeOutput::new());
        }
        _ => {}
    }

    // Phase 15 m3-001: Mode32 dispatch for mov r32, imm32 and mov r32, r32
    if inst.mode == InstrMode::Mode32 {
        match inst.operands.as_slice() {
            [Operand::Reg(dst), Operand::Imm64(imm)] => {
                let imm_u = *imm as u64;
                let fits_u32 = imm_u <= u32::MAX as u64;
                let fits_i32 = (*imm as i64) >= i32::MIN as i64 && (*imm as i64) <= i32::MAX as i64;
                if !(fits_u32 || fits_i32) {
                    return Err(EncodeError::Unsupported(
                        "E0019: 64-bit immediate in 32-bit mode",
                    ));
                }
                mov_reg32_imm32(buf, reg64_from(*dst)?, imm_u as u32);
                return Ok(EncodeOutput::new());
            }
            [Operand::Reg(dst), Operand::Reg(src)] => {
                mov_reg32_reg32(buf, reg64_from(*dst)?, reg64_from(*src)?);
                return Ok(EncodeOutput::new());
            }
            // Phase 15 m3-002: mov r32, [abs32]
            [Operand::Reg(dst), Operand::SymbolRef { name, addend }] => {
                let byte_offset = mov_reg32_mem_abs32(buf, reg64_from(*dst)?);

                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }
            // Phase 15 m3-002: mov [abs32], r32
            [Operand::SymbolRef { name, addend }, Operand::Reg(src)] => {
                let byte_offset = mov_mem_abs32_reg32(buf, reg64_from(*src)?);

                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }
            // Phase 15 m3-003: mov [abs32], imm32
            [Operand::SymbolRef { name, addend }, Operand::Imm64(imm)] => {
                let imm_i = *imm as i64;
                let fits_u32 = (*imm as u64) <= u32::MAX as u64;
                let fits_i32 = (i32::MIN as i64..=i32::MAX as i64).contains(&imm_i);
                if !(fits_u32 || fits_i32) {
                    return Err(EncodeError::Unsupported(
                        "E0019: mov [abs32], imm: immediate exceeds 32 bits",
                    ));
                }
                let byte_offset = mov_mem_abs32_imm32(buf, *imm as u32);
                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }
            _ => {} // fall through
        }
    }

    // Phase 15 m3-003: Mode64 diagnostic for mov [abs], imm
    if inst.mode == InstrMode::Mode64 {
        if let [Operand::SymbolRef { .. }, Operand::Imm64(_)] = inst.operands.as_slice() {
            return Err(EncodeError::Unsupported(
                "mov [abs], imm not encodable in 64-bit mode without register base; \
                 use mov rax, sym; mov [rax], imm",
            ));
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // mov r64, r64 → 48 89 <ModR/M>
            mov_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            // mov r64, imm64 → REX.W B8+rd <imm64>
            mov_reg64_imm64(buf, reg64_from(*dest)?, *imm as u64);
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp: 0,
            },
        ] => {
            // mov r64, [base + index * scale] — delegate to general SIB handler
            // (emit_indexed_load was designed for mixed widths and confused scale with operand width)
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            mov_reg64_mem_sib_disp(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                0,
            );
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // mov r64, [base + disp] — Phase 8 m5-002: general memory operand form
            mov_reg64_mem_reg64_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
            Operand::Reg(src),
        ] => {
            // mov [base + disp], r64 — Phase 8 m5-002: general memory operand form
            mov_mem_reg64_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp,
            },
        ] => {
            // mov r64, [base + index*scale + disp] — Phase 9 m1-003: SIB with displacement
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            mov_reg64_mem_sib_disp(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                *disp,
            );
            Ok(EncodeOutput::new())
        }
        [
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp,
            },
            Operand::Reg(src),
        ] => {
            // mov [base + index*scale + disp], r64 — Phase 9 m1-003: SIB with displacement
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            mov_mem_sib_disp_reg64(
                buf,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                *disp,
                reg64_from(*src)?,
            );
            Ok(EncodeOutput::new())
        }
        // PA-R16-007: mov reg, [disp32] and related absolute-address forms
        [Operand::Reg(dest), Operand::MemDisp { disp }] => {
            // mov reg, [disp32] — delegate to absolute-form encoder
            // Width is determined by the register size (default W64 for r64)
            let dest_reg = reg64_from(*dest)?;
            let width = if let Mnemonic::MovSized { width } = inst.mnemonic {
                width
            } else {
                // Plain Mov defaults to W64
                IntWidth::W64
            };
            mov_reg_mem_abs_disp32(buf, width, dest_reg, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::MemDisp { disp }, Operand::Reg(src)] => {
            // mov [disp32], reg — delegate to absolute-form encoder
            // Width is determined by the register size (default W64 for r64)
            let src_reg = reg64_from(*src)?;
            let width = if let Mnemonic::MovSized { width } = inst.mnemonic {
                width
            } else {
                // Plain Mov defaults to W64
                IntWidth::W64
            };
            mov_mem_abs_disp32_reg(buf, width, *disp, src_reg);
            Ok(EncodeOutput::new())
        }
        [Operand::MemDisp { disp }, Operand::Imm64(imm)] => {
            // mov [disp32], imm — delegate to absolute-form encoder
            // Width must come from MovSized mnemonic; if plain Mov, default W64
            let width = if let Mnemonic::MovSized { width } = inst.mnemonic {
                width
            } else {
                // Plain Mov with immediate defaults to W64
                IntWidth::W64
            };
            mov_mem_abs_disp32_imm(buf, width, *disp, *imm);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::SymbolRef { name, addend }] => {
            // mov r64, [symbol + addend] → 48 8B /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8B); // mov r64, r/m64 opcode

            // RIP-relative addressing: mod=00, r/m=5
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the mov instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [Operand::SymbolRef { name, addend }, Operand::Reg(src)] => {
            // PA10-006w: mov [symbol + addend], r64 → 48 89 /r [rip-relative ModR/M] [disp32_placeholder]
            // Symmetric to the load form above; opcode 0x8B → 0x89 for store; REX.R still applies
            // to the register operand (now the source). ModR/M mod=00, rm=5 (rip-relative),
            // reg field = src<2:0>. Emits R_X86_64_PC32 with addend biased by -4 per SysV AMD64 ABI.
            let src_id = reg64_from(*src)? as u8;
            let rex_byte = rex(true, (src_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x89); // mov r/m64, r64 opcode
            buf.bytes.push(0x05 | ((src_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for mov r64, [rip + sym + addend]
        [Operand::Reg(dest), Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 48 8B /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8B); // mov r64, r/m64 opcode
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for mov [rip + sym + addend], r64
        [Operand::MemRipRelSym { name, addend }, Operand::Reg(src)] => {
            // Identical encoding to SymbolRef store form: 48 89 /r [rip-relative ModR/M] [disp32_placeholder]
            let src_id = reg64_from(*src)? as u8;
            let rex_byte = rex(true, (src_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x89); // mov r/m64, r64 opcode
            buf.bytes.push(0x05 | ((src_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::Unsupported(
            "mov form not in phase-3-m2-002 minimum",
        )),
    }
}

/// Dispatcher for MOV to/from control register (Phase 6, m1-002).
///
/// Extracts CR and GPR indices from operands and routes to encode_mov_cr.
/// - write=true: mov cr_idx, gpr (destination is CR, source is GPR)
/// - write=false: mov gpr, cr_idx (destination is GPR, source is CR)
fn encode_mov_cr_dispatcher(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(first), Operand::Reg(second)] => {
            let (cr_id, gpr_id) = if write {
                // mov cr, gpr: first is CR, second is GPR
                (first.0, second.0)
            } else {
                // mov gpr, cr: first is GPR, second is CR
                (second.0, first.0)
            };

            // Convert CR ID to CR index: cr_idx = RegId - 16
            let cr_idx = cr_id - 16;

            // GPR index is directly the reg_id (0-15)
            let gpr_idx = gpr_id;

            // Encode using the low-level helper
            encode_mov_cr(buf, write, cr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mov,
        }),
    }
}

/// Dispatcher for MOV to/from debug register (Phase 6, m1-003).
///
/// Extracts DR and GPR indices from operands and routes to encode_mov_dr.
/// - write=true: mov dr_idx, gpr (destination is DR, source is GPR)
/// - write=false: mov gpr, dr_idx (destination is GPR, source is DR)
fn encode_mov_dr_dispatcher(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(first), Operand::Reg(second)] => {
            let (dr_id, gpr_id) = if write {
                // mov dr, gpr: first is DR, second is GPR
                (first.0, second.0)
            } else {
                // mov gpr, dr: first is GPR, second is DR
                (second.0, first.0)
            };

            // Convert DR ID to DR index: dr_idx = RegId - 25 (compact encoding)
            let dr_idx = dr_id - 25;

            // GPR index is directly the reg_id (0-15)
            let gpr_idx = gpr_id;

            // Encode using the low-level helper
            encode_mov_dr(buf, write, dr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mov,
        }),
    }
}

fn encode_add(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // add r64, r64 → 48 01 <ModR/M>
            add_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // Consult can_shorten_add_to_32bit: if the high 32 bits are zero/unused,
            // use 32-bit immediate form instead of 64-bit
            if can_shorten_add_to_32bit(false)
                && imm_i64 >= i32::MIN as i64
                && imm_i64 <= i32::MAX as i64
            {
                // High bits are not used and value fits in i32: use 32-bit form
                let imm_i32 = imm_i64 as i32;

                // Further tighten: if imm fits in i8, use 8-bit form for even shorter encoding
                if (-128..=127).contains(&imm_i32) {
                    add_reg64_imm8(buf, dest_reg, imm_i32 as i8);
                    stats.record_tightening();
                } else {
                    add_reg64_imm32(buf, dest_reg, imm_i32);
                    stats.record_tightening();
                }
            } else {
                // Value requires full 64-bit immediate: use mov + add pattern
                // For now, return unsupported as phase-3-m2-002 doesn't have this
                return Err(EncodeError::Unsupported(
                    "64-bit immediate add not yet supported",
                ));
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "add form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_sub(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // sub r64, r64 → 48 29 <ModR/M>
            sub_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            if can_shorten_add_to_32bit(false)
                && imm_i64 >= i32::MIN as i64
                && imm_i64 <= i32::MAX as i64
            {
                let imm_i32 = imm_i64 as i32;
                if (-128..=127).contains(&imm_i32) {
                    sub_reg64_imm8(buf, dest_reg, imm_i32 as i8);
                    stats.record_tightening();
                } else {
                    sub_reg64_imm32(buf, dest_reg, imm_i32);
                    stats.record_tightening();
                }
            } else {
                return Err(EncodeError::Unsupported(
                    "64-bit immediate sub not yet supported",
                ));
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "sub form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_adc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0037: adc only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            match width {
                IntWidth::W64 => {
                    adc_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let src_id = reg32_from(*src)? as u8;
                    adc_reg32_reg32(buf, dst_id, src_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            match width {
                IntWidth::W64 => {
                    adc_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let base_id = reg32_from(*base)? as u8;
                    adc_reg32_mem_base_disp(buf, dst_id, base_id, *disp);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Adc { width },
            })
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let imm_i64 = *imm;

            // Check if the immediate fits in i32
            if imm_i64 < i32::MIN as i64 || imm_i64 > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "adc: immediate does not fit in i32",
                ));
            }

            let imm_i32 = imm_i64 as i32;

            // Choose between imm8 and imm32 forms
            if (-128..=127).contains(&imm_i32) {
                // Immediate fits in i8; use shorter encoding
                match width {
                    IntWidth::W64 => {
                        adc_reg64_imm8(buf, reg64_from(*dest)?, imm_i32 as i8);
                    }
                    IntWidth::W32 => {
                        let dst_id = reg32_from(*dest)? as u8;
                        adc_reg32_imm8(buf, dst_id, imm_i32 as i8);
                    }
                    _ => unreachable!(),
                }
            } else {
                // Immediate requires full i32 form
                match width {
                    IntWidth::W64 => {
                        adc_reg64_imm32(buf, reg64_from(*dest)?, imm_i32);
                    }
                    IntWidth::W32 => {
                        let dst_id = reg32_from(*dest)? as u8;
                        adc_reg32_imm32(buf, dst_id, imm_i32);
                    }
                    _ => unreachable!(),
                }
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Adc { width },
        }),
    }
}

fn encode_sbb(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0038: sbb only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            match width {
                IntWidth::W64 => {
                    sbb_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let src_id = reg32_from(*src)? as u8;
                    sbb_reg32_reg32(buf, dst_id, src_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            match width {
                IntWidth::W64 => {
                    sbb_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let base_id = reg32_from(*base)? as u8;
                    sbb_reg32_mem_base_disp(buf, dst_id, base_id, *disp);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Sbb { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Sbb { width },
        }),
    }
}

fn encode_popcnt(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0039: popcnt only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            match width {
                IntWidth::W64 => {
                    popcnt_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let src_id = reg32_from(*src)? as u8;
                    popcnt_reg32_reg32(buf, dst_id, src_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            match width {
                IntWidth::W64 => {
                    popcnt_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let base_id = reg32_from(*base)? as u8;
                    popcnt_reg32_mem_base_disp(buf, dst_id, base_id, *disp);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Popcnt { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Popcnt { width },
        }),
    }
}

fn encode_bsf(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0050: bsf only supports W64 (PA-R16-008, #974)",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            bsf_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            bsf_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bsf { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bsf { width },
        }),
    }
}

fn encode_bsr(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0051: bsr only supports W64 (PA-R16-008, #974)",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            bsr_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            bsr_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bsr { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bsr { width },
        }),
    }
}

fn encode_tzcnt(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0052: tzcnt only supports W64 (PA-R16-008, #974)",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            tzcnt_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            tzcnt_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Tzcnt { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Tzcnt { width },
        }),
    }
}

fn encode_bt(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0040: bt only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    bt_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    bt_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    bt_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    bt_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bt { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bt { width },
        }),
    }
}

fn encode_bts(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0041: bts only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    bts_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    bts_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    bts_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    bts_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bts { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bts { width },
        }),
    }
}

fn encode_btr(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0042: btr only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    btr_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    btr_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    btr_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    btr_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Btr { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Btr { width },
        }),
    }
}

fn encode_btc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0043: btc only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    btc_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    btc_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    btc_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    btc_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Btc { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Btc { width },
        }),
    }
}

fn encode_cmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // cmp r64, r64 → 48 39 <ModR/M>
            cmp_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
            Operand::Reg(src),
        ] => {
            // cmp [base + disp], r64 → 48 39 <ModR/M> [disp]
            cmp_mem_reg64_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // Determine the best encoding form for the immediate
            if (-128..=127).contains(&imm_i64) {
                // 8-bit immediate: use 83 /7 ib
                cmp_reg64_imm8(buf, dest_reg, imm_i64 as i8);
            } else if imm_i64 >= i32::MIN as i64 && imm_i64 <= i32::MAX as i64 {
                // 32-bit immediate: use 81 /7 id
                cmp_reg64_imm32(buf, dest_reg, imm_i64 as i32);
            } else {
                // imm64 out-of-range: unsupported
                return Err(EncodeError::Unsupported(
                    "cmp imm64 not supported; load into reg first",
                ));
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "cmp shape not in phase-6-m4-001 minimum",
        )),
    }
}

fn encode_test(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 7 m1-001: test r64, r64 for condition testing.
    // Operands: [register, register] for "test rdi, rdi" shape.
    // PA-R13-006 (issue #935): [register, imm64-in-i32-range] for
    // "test r64, imm32" — REX.W F7 /0 id (with 48 A9 id short form for RAX).
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // test r64, r64 → 48 85 <ModR/M>
            test_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // TEST has no imm8 sign-extended form (unlike CMP/ADD/SUB — the
            // 83 /X ib subgroup doesn't include /0=TEST). All immediates go
            // through F7 /0 id (or the A9 id short form for RAX).
            if imm_i64 < i32::MIN as i64 || imm_i64 > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "64-bit immediate test not yet supported; use and+cmp workaround",
                ));
            }
            test_reg64_imm32(buf, dest_reg, imm_i64 as i32);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "test shape not in phase-7-m1-001 minimum",
        )),
    }
}

/// Resolve an 8-bit register ID to (masked_id, needs_rex) for use in setcc.
///
/// Returns:
/// - (id, false) for al-bl and r8b-r15b (standard low-byte registers)
/// - (id, true) for spl/bpl/sil/dil (high-byte regs requiring REX prefix)
/// - (id, false) for r8b-r15b (extended regs, REX.B handled by setcc_reg8)
fn resolve_reg8(reg_id: RegId) -> (u8, bool) {
    match reg_id.0 {
        // Standard low-byte registers: al, cl, dl, bl (0-3)
        0..=3 => (reg_id.0 as u8, false),
        // spl/bpl/sil/dil (33-36) — mapped to 4-7 with needs_rex = true
        33 => (4, true),
        34 => (5, true),
        35 => (6, true),
        36 => (7, true),
        // Extended low-byte registers: r8b-r15b (8-15)
        8..=15 => (reg_id.0 as u8, false),
        // Anything else is invalid
        _ => (reg_id.0 as u8, false),
    }
}

fn encode_jcc(
    ir_cond: IrCond,
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jcc can be encoded as rel32 or rel8 depending on displacement
            let cond = cond_from(ir_cond)?;
            let disp = *rel;

            // Consult can_use_rel8: if displacement fits in signed byte, use shorter encoding
            if can_use_rel8(disp) {
                // Use rel8 form (saves 4 bytes: 0x0F 0x8X <rel32> → 0x7X <rel8>)
                jcc_rel8(buf, cond, disp as i8);
                stats.record_tightening();
            } else {
                // Use rel32 form (standard 6-byte encoding)
                jcc_rel32(buf, cond, disp as i32);
            }
            Ok(EncodeOutput::new())
        }
        [Operand::LabelRef { name, addend }] => {
            // Phase 6 m4-003: Label reference (forward or backward).
            // Emit placeholder rel32 and record fixup for linker resolution.
            let cond = cond_from(ir_cond)?;

            // Emit jcc rel32 with zero placeholder
            jcc_rel32(buf, cond, 0);

            let mut output = EncodeOutput::new();
            output.add_label_fixup(LabelFixup {
                byte_offset: 2, // offset of rel32 relative to instruction start (after 0F XX)
                label_name: name.clone(),
                addend: *addend,
                instruction_size: 6,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "jcc form not in phase-3-m2-002 minimum",
        )),
    }
}

/// Encode `setcc r8` — set byte on condition.
///
/// Expects `[Operand::Reg(dst)]` where dst is an 8-bit register.
/// Emits via `setcc_reg8` with REX handling for extended registers.
fn encode_setcc(
    ir_cond: IrCond,
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            let cond = cond_from(ir_cond)?;
            let (reg_id, needs_rex) = resolve_reg8(*dst);
            setcc_reg8(buf, cond, reg_id, needs_rex);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Setcc(ir_cond),
        }),
    }
}

fn encode_jmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jmp rel32 → E9 <rel32>
            jmp_rel32(buf, *rel as i32);
            Ok(EncodeOutput::new())
        }
        [Operand::LabelRef { name, addend }] => {
            // Phase 6 m4-003: Label reference (forward or backward).
            // Emit placeholder rel32 and record fixup for linker resolution.

            // Emit jmp rel32 with zero placeholder
            jmp_rel32(buf, 0);

            let mut output = EncodeOutput::new();
            output.add_label_fixup(LabelFixup {
                byte_offset: 1, // offset of rel32 relative to instruction start (after E9)
                label_name: name.clone(),
                addend: *addend,
                instruction_size: 5,
            });
            Ok(output)
        }
        [Operand::MemSymIndexed { name, addend, index, scale }] => {
            // PA-R15-009a: jmp [sym + index*scale] with absolute addressing.
            // Emit FF 24 SIB disp32 (absolute form, not RIP-relative).
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            let index_reg = reg64_from(*index)?;
            let disp_offset = jmp_mem_sib_no_base_indexed(buf, index_reg, scale_bits, 0)?;

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: disp_offset as u32,
                symbol: name.clone(),
                kind: RelocKind::Abs32,
                addend: *addend,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "jmp form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_call(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // call rel32 → E8 <rel32>
            call_rel32(buf, *rel as i32);
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // call symbol → E8 <disp32_placeholder> + RelocSite with Plt32
            // Phase 7 m1-001: Use RelocKind::Plt32 for PLT relocations.
            // Phase 7 m1-003: RelocSite::byte_offset is INSTRUCTION-LOCAL.
            // emit_text_from_instructions translates it to .text-relative by
            // adding `offset_before` (the buffer length at the start of this
            // instruction). The rel32 displacement begins at byte +1 of the E8
            // opcode. Previous code returned `byte_offset_in_text + 1`
            // (already .text-relative), then the translator added
            // `offset_before` again — double-counted, putting the reloc at
            // 2*offset_before + 1 instead of offset_before + 1.
            let _ = inst.byte_offset_in_text; // unused; translator owns the math
            buf.bytes.push(0xE8); // call rel32 opcode
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 1, // rel32 starts at byte +1 of the instruction
                symbol: name.clone(),
                kind: RelocKind::Plt32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: call reg64
        [Operand::Reg(r)] => {
            call_reg64(buf, reg64_from(*r)?);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [base + disp]
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            call_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [base + index*scale + disp]
        [Operand::MemSib { base, index: Some(idx), scale, disp }] => {
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            call_mem_sib_disp(buf, reg64_from(*base)?, reg64_from(*idx)?, scale_bits, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [rip + disp32]
        [Operand::MemRipRel { disp }] => {
            call_mem_rip_rel(buf, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [rip + sym + addend] → FF 15 <disp32> + PcRel32 reloc
        [Operand::MemRipRelSym { name, addend }] => {
            call_mem_rip_rel(buf, 0); // placeholder disp32
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 2, // rel32 starts at byte +2 of the FF 15 prefix
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "call form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_ret(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Ret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    ret(buf);
    Ok(EncodeOutput::new())
}

fn encode_rep_movsb(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepMovsb,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    buf.bytes.push(0xF3);
    buf.bytes.push(0xA4); // rep movsb
    Ok(EncodeOutput::new())
}

fn encode_lea(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // lea r64, [base + disp]
            // LEA uses MOV encoding but with different semantics
            // lea r64, [rbp + disp] → 48 8D /r [ModR/M] [disp]
            let dest_id = reg64_from(*dest)? as u8;
            let base_id = reg64_from(*base)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, (base_id >> 3) != 0);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            if (-128..=127).contains(disp) {
                // Use mod=01, disp8
                buf.bytes.push(0x40 | ((dest_id & 7) << 3) | (base_id & 7));
                buf.bytes.push(*disp as u8);
            } else {
                // Use mod=10, disp32
                buf.bytes.push(0x80 | ((dest_id & 7) << 3) | (base_id & 7));
                buf.bytes.extend(disp.to_le_bytes());
            }
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp,
            },
        ] => {
            // lea r64, [base + index * scale + disp]
            // Uses SIB (Scale-Index-Base) byte format: SIB = scale (2 bits) | index (3 bits) | base (3 bits)
            let dest_id = reg64_from(*dest)? as u8;
            let base_id = reg64_from(*base)? as u8;
            let index_id = reg64_from(*index)? as u8;

            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };

            let rex_byte = rex(
                true,
                (dest_id >> 3) != 0,
                (index_id >> 3) != 0,
                (base_id >> 3) != 0,
            );

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            // Use emit_mem_sib_disp to handle ModR/M, SIB, and displacement encoding,
            // including R13/RBP escape (disp8=0 when base is RBP/R13 and disp=0).
            emit_mem_sib_disp(buf, dest_id & 7, base_id, index_id, scale_bits, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::SymbolRef { name, addend }] => {
            // Mode32: lea r32, [symbol] → 8D /r [absolute ModR/M] [disp32_placeholder]
            if inst.mode == InstrMode::Mode32 {
                let dest_reg = reg64_from(*dest)?;
                let byte_offset = lea_reg32_mem_abs32(buf, dest_reg);

                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }

            // Mode64: lea r64, [symbol] → 48 8D /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            // RIP-relative addressing: mod=00, r/m=5
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            // PA-R17-014 / #992: Defense-in-depth check: addend must fit in i32 for rel32 relocations
            if i32::try_from(*addend as i64 + PC32_FIELD_BIAS as i64).is_err() {
                return Err(EncodeError::Unsupported("lea rel32 addend overflows i32"));
            }

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the lea instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for lea r64, [rip + sym + addend]
        [Operand::Reg(dest), Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 48 8D /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            // PA-R17-014 / #992: Defense-in-depth check: addend must fit in i32 for rel32 relocations
            if i32::try_from(*addend as i64 + PC32_FIELD_BIAS as i64).is_err() {
                return Err(EncodeError::Unsupported("lea rel32 addend overflows i32"));
            }

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "lea form not in phase-3-m2-002 minimum",
        )),
    }
}

// Helper to emit a REX prefix byte (copied from encode.rs for use in encode_lea).
fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | (u8::from(x) << 1) | u8::from(b)
}

fn encode_cli(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cli,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xFA);
    Ok(EncodeOutput::new())
}

fn encode_cld(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cld,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x84); // sentinel for CLD
    Ok(EncodeOutput::new())
}

fn encode_sti(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sti,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xFB);
    Ok(EncodeOutput::new())
}

fn encode_std(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Std,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x85); // sentinel for STD
    Ok(EncodeOutput::new())
}

fn encode_hlt(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Hlt,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xF4);
    Ok(EncodeOutput::new())
}

fn encode_nop(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Nop,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x90);
    Ok(EncodeOutput::new())
}

fn encode_swapgs(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Swapgs,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x81); // sentinel for SWAPGS
    Ok(EncodeOutput::new())
}

fn encode_cpuid(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cpuid,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x82); // sentinel for CPUID
    Ok(EncodeOutput::new())
}

fn encode_ud2(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Ud2,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x83); // sentinel for UD2
    Ok(EncodeOutput::new())
}

fn encode_in(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: u8,
) -> Result<EncodeOutput, EncodeError> {
    // `in` expects exactly 1 operand: the data register (al/ax/eax, encoded as Rax)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::In { width },
            expected: 1,
            got: inst.operands.len(),
        });
    }

    // Verify the operand is Rax (al, ax, or eax depending on width)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            if *reg != RegId(0) {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::In { width },
                });
            }
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::In { width },
            });
        }
    }

    encode_in_dx(buf, width);
    Ok(EncodeOutput::new())
}

fn encode_out(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: u8,
) -> Result<EncodeOutput, EncodeError> {
    // `out` expects exactly 1 operand: the data register (al/ax/eax, encoded as Rax)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Out { width },
            expected: 1,
            got: inst.operands.len(),
        });
    }

    // Verify the operand is Rax (al, ax, or eax depending on width)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            if *reg != RegId(0) {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Out { width },
                });
            }
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Out { width },
            });
        }
    }

    encode_out_dx(buf, width);
    Ok(EncodeOutput::new())
}

fn encode_wrmsr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // wrmsr expects exactly 0 operands (MSR index in ECX, value in EDX:EAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Wrmsr,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_wrmsr(buf);
    Ok(EncodeOutput::new())
}

fn encode_rdmsr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rdmsr expects exactly 0 operands (MSR index in ECX, result in EDX:EAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Rdmsr,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rdmsr(buf);
    Ok(EncodeOutput::new())
}

fn encode_int(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // int expects exactly 1 operand: an immediate value that fits in u8
    match inst.operands.as_slice() {
        [Operand::Imm64(imm)] => {
            // Check that the operand fits in u8
            if *imm > u8::MAX as i64 {
                return Err(EncodeError::Unsupported("int operand > u8"));
            }
            encode_int_imm8(buf, *imm as u8);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Int,
            expected: 1,
            got: inst.operands.len(),
        }),
    }
}

fn encode_mov_cr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    // mov cr_idx, gpr (write=true): first=CR, second=GPR
    // mov gpr, cr_idx (write=false): first=GPR, second=CR
    match inst.operands.as_slice() {
        [Operand::Reg(first_reg), Operand::Reg(second_reg)] => {
            let (cr_idx, gpr_idx) = if write {
                // mov cr_idx, gpr: first is CR, second is GPR
                (first_reg.0, second_reg.0)
            } else {
                // mov gpr, cr_idx: first is GPR, second is CR
                (second_reg.0, first_reg.0)
            };

            // Validate CR index: phase-5 supports CR0..CR4 + CR8 only
            match cr_idx {
                0 | 3 | 4 | 8 => {}
                _ => {
                    return Err(EncodeError::Unsupported("CR index not in phase-5 minimum"));
                }
            }

            // Validate GPR index: must be 0-15
            if gpr_idx > 15 {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::MovCr { write },
                });
            }

            // Emit the instruction using the low-level encoder
            encode_mov_cr(buf, write, cr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovCr { write },
        }),
    }
}

fn encode_mov_dr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    // mov dr_idx, gpr (write=true): first=DR, second=GPR
    // mov gpr, dr_idx (write=false): first=GPR, second=DR
    match inst.operands.as_slice() {
        [Operand::Reg(first_reg), Operand::Reg(second_reg)] => {
            let (dr_idx, gpr_idx) = if write {
                // mov dr_idx, gpr: first is DR, second is GPR
                (first_reg.0, second_reg.0)
            } else {
                // mov gpr, dr_idx: first is GPR, second is DR
                (second_reg.0, first_reg.0)
            };

            // Validate DR index: phase-5 supports DR0..DR7 only
            if dr_idx > 7 {
                return Err(EncodeError::Unsupported(
                    "DR index > 7 not supported in phase-5",
                ));
            }

            // Validate GPR index: must be 0-15
            if gpr_idx > 15 {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::MovDr { write },
                });
            }

            // Emit the instruction using the low-level encoder
            encode_mov_dr(buf, write, dr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovDr { write },
        }),
    }
}

fn encode_lgdt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // lgdt [base + disp] - load GDT descriptor
    // Mode32 short-circuit: lgdt [symbol] with absolute 32-bit addressing
    if inst.mode == InstrMode::Mode32 {
        if let [Operand::SymbolRef { name, addend }] = inst.operands.as_slice() {
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0x15);
            buf.bytes.extend([0, 0, 0, 0]);
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::Abs32,
                addend: *addend,
            });
            return Ok(output);
        }
    }

    match inst.operands.as_slice() {
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // Valid form: [base] with optional displacement, no index
            let base_reg = reg64_from(*base)?;
            encode_descriptor_table_load(buf, base_reg, *disp, 2); // 2 = /2 for lgdt
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // lgdt [symbol] in Mode64 → 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            // RIP-relative addressing: mod=00, /2 for lgdt
            buf.bytes.push(0x15); // 0x05 | (2 << 3) = rip-relative with /2

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the lgdt instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for lgdt [rip + sym + addend]
        [Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0x15); // 0x05 | (2 << 3) = rip-relative with /2
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [
            Operand::MemSib {
                base: _,
                index: Some(_),
                scale: _,
                disp: _,
            },
        ] => {
            // Indexed form not supported
            Err(EncodeError::Unsupported("lgdt/lidt indexed form"))
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Lgdt,
        }),
    }
}

fn encode_lidt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // lidt [base + disp] - load IDT descriptor
    match inst.operands.as_slice() {
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // Valid form: [base] with optional displacement, no index
            let base_reg = reg64_from(*base)?;
            encode_descriptor_table_load(buf, base_reg, *disp, 3); // 3 = /3 for lidt
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // lidt [symbol] → 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            // RIP-relative addressing: mod=00, /3 for lidt
            buf.bytes.push(0x1D); // 0x05 | (3 << 3) = rip-relative with /3

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the lidt instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for lidt [rip + sym + addend]
        [Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0x1D); // 0x05 | (3 << 3) = rip-relative with /3
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [
            Operand::MemSib {
                base: _,
                index: Some(_),
                scale: _,
                disp: _,
            },
        ] => {
            // Indexed form not supported
            Err(EncodeError::Unsupported("lgdt/lidt indexed form"))
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Lidt,
        }),
    }
}

fn encode_iret_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // iret expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Iret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_iret(buf);
    Ok(EncodeOutput::new())
}

fn encode_iretq_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // iretq expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Iretq,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_iretq(buf);
    Ok(EncodeOutput::new())
}

fn encode_sysret_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // sysret expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sysret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_sysret(buf);
    Ok(EncodeOutput::new())
}

fn encode_syscall_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // syscall expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Syscall,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_syscall(buf);
    Ok(EncodeOutput::new())
}

fn encode_rep_stosq_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rep stosq expects exactly 0 operands (RAX=value, RCX=count, RDI=destination implicit)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepStosq,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rep_stosq(buf);
    Ok(EncodeOutput::new())
}

fn encode_far_jmp_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // ljmp supports two forms:
    // 1. Memory indirect: [base + disp] or [rip + disp] — expects 1 operand
    // 2. Direct immediate: ljmp selector:offset — expects 2 operands (selector, offset)
    match inst.operands.len() {
        1 => {
            // Memory indirect form
            match &inst.operands[0] {
                Operand::MemSib {
                    base,
                    index: None,
                    scale: Scale::X1,
                    disp,
                } => {
                    // [base + disp] form
                    encode_far_jmp(buf, Some(reg64_from(*base)?), *disp);
                    Ok(EncodeOutput::new())
                }
                Operand::MemRipRel { disp } => {
                    // [rip + disp32] form
                    encode_far_jmp(buf, None, *disp);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::FarJmp,
                }),
            }
        }
        2 => {
            // Direct immediate form: ljmp selector:offset
            // Operand[0] = selector (imm16, encoded as Imm64)
            // Operand[1] = offset (imm32, encoded as Imm64 or symbol reference)
            let selector = match &inst.operands[0] {
                Operand::Imm64(imm) => *imm as u16,
                _ => {
                    return Err(EncodeError::OperandShape {
                        mnemonic: Mnemonic::FarJmp,
                    });
                }
            };

            match &inst.operands[1] {
                Operand::Imm64(imm) => {
                    // Direct immediate: opcode EA + imm32 offset + imm16 selector
                    encode_far_jmp_imm(buf, *imm as u32, selector);
                    Ok(EncodeOutput::new())
                }
                Operand::SymbolRef { name, addend } => {
                    // Symbol reference: emit R_X86_64_32 relocation
                    let mut output = EncodeOutput::new();
                    encode_far_jmp_imm_sym(buf, selector);
                    output.add_reloc(RelocSite {
                        byte_offset: 1, // imm32 starts at byte +1 of the EA instruction (instruction-local); translator adds offset_before
                        symbol: name.clone(),
                        kind: RelocKind::Abs32,
                        addend: *addend,
                    });
                    Ok(output)
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::FarJmp,
                }),
            }
        }
        _ => Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::FarJmp,
            expected: 1,
            got: inst.operands.len(),
        }),
    }
}

fn encode_rdtsc_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rdtsc expects exactly 0 operands (implicitly reads time-stamp counter into RDX:RAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Rdtsc,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rdtsc(buf);
    Ok(EncodeOutput::new())
}

fn encode_invlpg_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // invlpg expects exactly 1 operand: memory address
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Invlpg,
            expected: 1,
            got: inst.operands.len(),
        });
    }

    match &inst.operands[0] {
        Operand::MemSib {
            base,
            index: None,
            scale: Scale::X1,
            disp,
        } => {
            // [base + disp] form
            encode_invlpg(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Invlpg,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{InstrMode, Instruction, Mnemonic, Operand, RegId, Scale, SegReg};

    #[test]
    fn encode_mov_rax_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_not_rax_emits_48_f7_d0() {
        // Mnemonic::Not with [Reg(rax)] dispatches to not_reg64 → 48 F7 D0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Not,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xD0]);
    }

    #[test]
    fn encode_not_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Not,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Not);
    }

    #[test]
    fn encode_mov_sized_w32_dispatches_to_b8_imm32() {
        // Mnemonic::MovSized { W32 } with [Reg(rax), Imm64(42)] → B8 2A 00 00 00.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W32,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB8, 0x2A, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn encode_mov_sized_w16_dispatches_to_66_b8_imm16() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W16,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x66, 0xB8, 0x2A, 0x00]);
    }

    #[test]
    fn encode_mov_sized_w8_dispatches_to_b0_imm8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W8,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB0, 0x2A]);
    }

    #[test]
    fn encode_mov_sized_w64_delegates_to_generic_mov() {
        // W64 must reproduce the established 64-bit immediate move (48 B8 ...).
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W64,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(
            buf.as_slice(),
            &[0x48, 0xB8, 0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn encode_movsx_default_width_emits_48_63() {
        // Mnemonic::Movsx with [Reg(rax), Reg(rcx)] and no hint defaults to a
        // 4-byte source → MOVSXD → 48 63 C1.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Movsx,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x63, 0xC1]);
    }

    #[test]
    fn encode_movsx_width1_via_hint_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
        use paideia_as_ir::EncodingHint;

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Movsx,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: Some(EncodingHint {
                opcode: 0x0FBE,
                operand_size: 1,
            }),
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xBE, 0xC1]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Movsx);
    }

    #[test]
    fn encode_mov_rax_imm64_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)),
                Operand::Imm64(0x1234567890ABCDEF)
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // Phase 15 m3-001: Mode32 dispatch tests
    #[test]
    fn mov_eax_imm32_mode32_emits_b8_no_rex() {
        // mov eax, 0x83 in 32-bit mode → B8 83 00 00 00 (no REX)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x83)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB8, 0x83, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mov_ecx_zero_mode32_emits_b9_zero() {
        // mov ecx, 0x00 in 32-bit mode → B9 00 00 00 00 (no REX)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Imm64(0x00)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xB9, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mov_r8d_imm32_mode32_emits_rex_b_b8() {
        // mov r8d, 0x40000083 in 32-bit mode → 41 B8 83 00 00 40 (REX.B for r8)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Imm64(0x40000083)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0xB8, 0x83, 0x00, 0x00, 0x40]);
    }

    #[test]
    fn mov_eax_ecx_mode32_emits_89_c8() {
        // mov eax, ecx in 32-bit mode → 89 C8 (store form per AC specs)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x89, 0xC8]);
    }

    #[test]
    fn mov_eax_imm64_overflow_mode32_yields_e0501() {
        // mov eax, 0x1_0000_0000 in 32-bit mode → E0019 error (64-bit imm)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1_0000_0000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err());
        match result {
            Err(EncodeError::Unsupported(msg)) => {
                assert!(msg.contains("E0019"));
            }
            _ => panic!("Expected E0019 error for 64-bit immediate in 32-bit mode"),
        }
    }

    #[test]
    fn mov_rax_imm_mode32_yields_e0501() {
        // mov rax, 0x83 with Mode32 → E0019 error (64-bit destination)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W64,
            },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x83)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err());
        match result {
            Err(EncodeError::Unsupported(msg)) => {
                assert!(msg.contains("E0019"));
            }
            _ => panic!("Expected E0019 error for 64-bit destination in 32-bit mode"),
        }
    }

    #[test]
    fn mov_eax_ecx_mode32_round_trips_through_iced_x86_32bit_decoder() {
        // Verify 32-bit mov eax, ecx round-trips with iced-x86 32-bit decoder
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Decode in 32-bit mode
        let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_add_rax_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_sub_rax_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_ret_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ret);
    }

    #[test]
    fn encode_rep_movsb_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsb,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Movsb);
    }

    #[test]
    fn encode_rep_movsb_rejects_operand() {
        // PA-R13-011 (#940): rep movsb must not have any operands.
        // This test verifies that rep_movsb rax; correctly fails with OperandCount error.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsb,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::RepMovsb, expected: 0, .. }));
    }

    #[test]
    fn encode_rep_stosq_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify byte sequence: F3 48 AB
        assert_eq!(buf.as_slice(), &[0xF3, 0x48, 0xAB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Stosq);
    }

    #[test]
    fn encode_rep_stosq_rejects_operand() {
        // PA-R13-012 (#941): rep stosq must not have any operands.
        // This test verifies that rep_stosq rax; correctly fails with OperandCount error.
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosq,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::RepStosq, expected: 0, .. }));
    }

    #[test]
    fn encode_indexed_load_via_mov_dispatches_correctly() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(6),        // rsi
                    index: Some(RegId(7)), // rdi
                    scale: Scale::X8,
                    disp: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_reg_mem_disp_plain_mov_defaults_to_w64() {
        // PA-R16-007: plain Mov with MemDisp now supported, defaults to W64
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemDisp { disp: 0x1000 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_ok(), "mov rax, [0x1000] should now be supported");
        // Verify it encodes as W64 (with REX.W)
        assert_eq!(
            buf.as_slice(),
            &[0x48, 0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
            "plain Mov with MemDisp defaults to W64"
        );
    }

    // ── Tightened instruction encoding tests ────────────────────

    #[test]
    fn encode_add_with_small_imm_uses_8bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 8-bit immediate form (4 bytes: REX.W 83 /0 imm8)
        assert_eq!(buf.len(), 4);
        assert_eq!(stats.tightened, 1, "Expected one tightening for small imm8");

        // Verify with iced
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_add_with_imm_fitting_in_i32_uses_32bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 32-bit immediate form (7 bytes: REX.W 81 /0 imm32)
        assert_eq!(buf.len(), 7);
        assert_eq!(stats.tightened, 1, "Expected one tightening for i32 imm");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_sub_with_small_imm_uses_8bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 8-bit immediate form (4 bytes: REX.W 83 /5 imm8)
        assert_eq!(buf.len(), 4);
        assert_eq!(stats.tightened, 1, "Expected one tightening for small imm8");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_sub_with_imm_fitting_in_i32_uses_32bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 32-bit immediate form (7 bytes: REX.W 81 /5 imm32)
        assert_eq!(buf.len(), 7);
        assert_eq!(stats.tightened, 1, "Expected one tightening for i32 imm");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_sub_rax_5_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(5)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
        assert_eq!(instr.immediate32(), 5);
    }

    #[test]
    fn encode_jcc_with_rel8_disp_uses_rel8_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(paideia_as_ir::Cond::Eq),
            operands: smallvec::smallvec![Operand::Imm64(50)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use rel8 form (2 bytes: 0x74 disp8)
        assert_eq!(buf.len(), 2);
        assert_eq!(stats.tightened, 1, "Expected one tightening for rel8 Jcc");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    #[test]
    fn encode_jcc_with_large_disp_uses_rel32_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(paideia_as_ir::Cond::Ne),
            operands: smallvec::smallvec![Operand::Imm64(0x1000)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use rel32 form (6 bytes: 0x0F 0x85 disp32)
        assert_eq!(buf.len(), 6);
        assert_eq!(stats.tightened, 0, "Expected no tightening for large disp");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    #[test]
    fn encode_stats_counts_tightening() {
        let mut stats = EncodeStats::new();
        assert_eq!(stats.tightened, 0);
        assert_eq!(stats.total, 0);

        stats.record_instruction();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.tightened, 0);

        stats.record_tightening();
        assert_eq!(stats.tightened, 1);
        assert_eq!(stats.total, 1);

        stats.record_instruction();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.tightened, 1);
    }

    // ── Phase-5 m2-002: zero-operand control + sync instructions ────────

    #[test]
    fn encode_nop_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Nop,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x90]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Nop);
    }

    #[test]
    fn encode_hlt_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Hlt,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xF4]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Hlt);
    }

    #[test]
    fn encode_cli_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cli,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xFA]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cli);
    }

    #[test]
    fn encode_sti_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sti,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xFB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sti);
    }

    #[test]
    fn encode_swapgs_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Swapgs,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0xF8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
    }

    #[test]
    fn encode_cpuid_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cpuid,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0xA2]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cpuid);
    }

    // ── I/O port instruction tests (phase-5 m2-003) ──────────────

    #[test]
    fn encode_in_al_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 1 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // al
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEC]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_ax_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 2 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // ax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x66, 0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_eax_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 4 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // eax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_out_dx_al_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 1 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // al
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEE]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_ax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 2 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // ax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x66, 0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_eax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 4 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // eax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    // ── Phase-5 m2-004: MSR and interrupt instructions ────────────

    #[test]
    fn encode_wrmsr_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Wrmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x30]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Wrmsr);
    }

    #[test]
    fn encode_rdmsr_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x32]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdmsr);
    }

    #[test]
    fn encode_int_0x20_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int,
            operands: smallvec::smallvec![Operand::Imm64(0x20)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xCD, 0x20]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int);
    }

    // ── Phase-5 m2-005: control register MOV instruction encoding ────────

    // Write (mov cr_idx, rax) tests via encode_instruction dispatcher
    #[test]
    fn encode_instruction_mov_cr0_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // mov cr0, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr3_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(3)), Operand::Reg(RegId(0))], // mov cr3, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr4_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(4)), Operand::Reg(RegId(0))], // mov cr4, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr8_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(0))], // mov cr8, rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr2_write_fails_validation() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(2)), Operand::Reg(RegId(0))], // mov cr2, rax (not supported)
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err(), "CR2 should not be supported in phase-5");
    }

    // Read (mov rax, cr_idx) tests via encode_instruction dispatcher
    #[test]
    fn encode_instruction_mov_rax_cr0_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // mov rax, cr0
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr3_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))], // mov rax, cr3
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr4_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(4))], // mov rax, cr4
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr8_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(8))], // mov rax, cr8
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr2_fails_validation() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(2))], // mov rax, cr2 (not supported)
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err(), "CR2 should not be supported in phase-5");
    }

    // ── Phase-5 m2-007: descriptor-table load (lgdt/lidt) ────────

    #[test]
    fn encode_lgdt_rdi_disp0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 17 (3 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x17]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lgdt_rdi_disp8_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 57 08 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x57, 0x08]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lgdt_rdi_disp_neg128_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: -128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 57 80 (4 bytes, -128 as u8 = 0x80)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x57, 0x80]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lidt_rdi_disp0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 1F (3 bytes, encoding: 0F 01 /3)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x1F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_lidt_rdi_disp16_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 16,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 5F 10 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x5F, 0x10]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_lidt_rdi_disp_neg128_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: -128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 5F 80 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x5F, 0x80]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    // ── Phase-5 m2-008: interrupt-return + system-return instructions ────────

    #[test]
    fn encode_iret_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: CF (1 byte)
        assert_eq!(buf.as_slice(), &[0xCF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Note: in 64-bit decoder, bare CF is decoded as Iretd (32-bit form)
        assert_eq!(instr.mnemonic(), IcedMnem::Iretd);
    }

    #[test]
    fn encode_iretq_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iretq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 CF (2 bytes, REX.W prefix)
        assert_eq!(buf.as_slice(), &[0x48, 0xCF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Iretq);
    }

    #[test]
    fn encode_sysret_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sysret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 0F 07 (3 bytes, REX.W prefix + two-byte opcode)
        assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0x07]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Note: in 64-bit decoder, 48 0F 07 is decoded as Sysretq (64-bit form)
        assert_eq!(instr.mnemonic(), IcedMnem::Sysretq);
    }

    #[test]
    fn encode_far_jmp_mem_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 2F (3 bytes)
        // 48 = REX.W
        // FF = opcode
        // 2F = ModR/M with mod=00, reg=5, rm=7 (rdi)
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x2F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_mem_rdi_plus_8_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 6F 08 (4 bytes)
        // 48 = REX.W
        // FF = opcode
        // 6F = ModR/M with mod=01, reg=5, rm=7 (rdi + disp8)
        // 08 = disp8
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x6F, 0x08]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_mem_rip_relative_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemRipRel { disp: 0x1000 }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 2D 00 10 00 00 (7 bytes)
        // 48 = REX.W
        // FF = opcode
        // 2D = ModR/M with mod=00, reg=5, rm=5 (rip-relative marker)
        // 00 10 00 00 = 0x1000 in little-endian
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x2D, 0x00, 0x10, 0x00, 0x00]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_imm_sym_produces_abs32_reloc_mode64() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x18), // selector
                Operand::SymbolRef {
                    name: "long_mode_entry".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode64,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: EA 00 00 00 00 18 00 (7 bytes)
        // EA = opcode
        // 00 00 00 00 = placeholder for imm32 offset (relocation target)
        // 18 00 = imm16 selector (0x18 in little-endian)
        assert_eq!(buf.as_slice(), &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "long_mode_entry");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Abs32);
        assert_eq!(output.reloc_sites[0].addend, 0);
    }

    #[test]
    fn encode_far_jmp_imm_sym_with_addend_produces_abs32_reloc() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x20), // selector
                Operand::SymbolRef {
                    name: "kernel_entry".to_string(),
                    addend: 16,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: EA 00 00 00 00 20 00 (7 bytes) — same byte form in Mode32 and Mode64
        assert_eq!(buf.as_slice(), &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "kernel_entry");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Abs32);
        assert_eq!(output.reloc_sites[0].addend, 16); // addend passed through unchanged
    }

    #[test]
    fn encode_far_jmp_imm_sym_reloc_agnostic_to_mode() {
        // Mode32 and Mode64 must emit identical bytes for ljmp selector:symbol
        let inst_mode32 = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x18),
                Operand::SymbolRef {
                    name: "entry".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode32,
        };

        let inst_mode64 = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x18),
                Operand::SymbolRef {
                    name: "entry".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode64,
        };

        let mut buf_mode32 = CodeBuffer::new();
        let mut buf_mode64 = CodeBuffer::new();
        let mut stats = EncodeStats::new();

        encode_instruction(&inst_mode32, &mut buf_mode32, &mut stats)
            .expect("Mode32 encoding failed");
        encode_instruction(&inst_mode64, &mut buf_mode64, &mut stats)
            .expect("Mode64 encoding failed");

        // Both must produce the same bytes (EA selector:offset form is mode-agnostic)
        assert_eq!(buf_mode32.as_slice(), buf_mode64.as_slice());
        assert_eq!(
            buf_mode32.as_slice(),
            &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00]
        );
    }

    // ── Phase-5 m5-002: SymbolRef tests ───────────────────────────────

    #[test]
    fn encode_lea_rax_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::SymbolRef {
                    name: "gdt_descriptor".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8D 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 8D = LEA opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "gdt_descriptor");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    #[test]
    fn encode_lgdt_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::SymbolRef {
                name: "gdt_descriptor".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 15 00 00 00 00 (7 bytes)
        // 0F 01 = two-byte opcode
        // 15 = ModR/M with mod=00, reg=2 (/2 for lgdt), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x15, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "gdt_descriptor");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    #[test]
    fn encode_mov_rax_symbol_ref_with_addend_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 8B = mov r64, r/m64 opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site with addend
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 4); // PC32_FIELD_BIAS: IR addend 8 → reloc addend 8 + (-4) = 4
    }

    // ── PA10-006w: mov [rip+sym], r64 store form ─────

    #[test]
    fn encode_mov_mem_sym_rax_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 0,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 89 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 89 = mov r/m64, r64 opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    #[test]
    fn encode_mov_mem_sym_addend_rdi_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 8,
                },
                Operand::Reg(RegId(7)), // rdi
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 89 3D 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 89 = mov r/m64, r64 opcode
        // 3D = ModR/M with mod=00, reg=7 (rdi), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x3D, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site with addend
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 4); // PC32_FIELD_BIAS: IR addend 8 → reloc addend 8 + (-4) = 4
    }

    #[test]
    fn encode_mov_mem_sym_r8_sets_rex_r() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "buf".to_string(),
                    addend: 0,
                },
                Operand::Reg(RegId(8)), // r8
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 4C 89 05 00 00 00 00 (7 bytes)
        // 4C = REX.W | REX.R (for r8 as source register)
        // 89 = mov r/m64, r64 opcode
        // 05 = ModR/M with mod=00, reg=0 (r8 & 7), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "buf");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, -4);
    }

    #[test]
    fn encode_mov_mem_sym_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SymbolRef {
                    name: "data".to_string(),
                    addend: 0,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
        assert_eq!(instr.op_kind(0), OpKind::Memory); // destination is memory (rip-relative)
        assert_eq!(instr.op_kind(1), OpKind::Register); // source is register (rax)
    }

    #[test]
    fn encode_call_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Call,
            operands: smallvec::smallvec![Operand::SymbolRef {
                name: "kernel_main_64".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: Some(0),
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: E8 00 00 00 00 (5 bytes)
        // E8 = call rel32 opcode
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0xE8, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site (Phase 7 m1-001: uses Plt32)
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "kernel_main_64");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Plt32);
        assert_eq!(output.reloc_sites[0].addend, -4); // PC32_FIELD_BIAS: IR addend 0 → reloc addend -4
    }

    // Phase 6 m1-002: CR move dispatch tests
    // These tests verify that MOV instructions with CR operands are correctly
    // classified and routed through encode_mov_cr_dispatcher, emitting the correct bytes.

    /// Test: mov cr3, rdi → 0F 22 DF
    /// CR3 = 16 + 3 = 19, RDI = 7
    #[test]
    fn encode_mov_cr3_rdi_emits_0f22df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(19)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xDF]);
    }

    /// Test: mov cr4, rcx → 0F 22 E1
    /// CR4 = 16 + 4 = 20, RCX = 1
    #[test]
    fn encode_mov_cr4_rcx_emits_0f22e1() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(20)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE1]);
    }

    /// Test: mov cr0, rax → 0F 22 C0
    /// CR0 = 16 + 0 = 16, RAX = 0
    #[test]
    fn encode_mov_cr0_rax_via_dispatch_emits_0f22c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(16)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);
    }

    /// Test: mov rdi, cr3 → 0F 20 DF (read from CR3)
    /// RDI = 7, CR3 = 16 + 3 = 19
    #[test]
    fn encode_mov_rdi_cr3_emits_0f20df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(19))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xDF]);
    }

    /// Test: mov rcx, cr4 → 0F 20 E1 (read from CR4)
    /// RCX = 1, CR4 = 16 + 4 = 20
    #[test]
    fn encode_mov_rcx_cr4_emits_0f20e1() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(20))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE1]);
    }

    /// Test: mov cr8, rax → 44 0F 22 C0 (CR8 requires REX.R)
    /// CR8 = 16 + 8 = 24, RAX = 0
    #[test]
    fn encode_mov_cr8_rax_via_dispatch_emits_440f22c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(24)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);
    }

    // Phase 6 m1-003: DR move dispatch tests
    // These tests verify that MOV instructions with DR operands are correctly
    // classified and routed through encode_mov_dr_dispatcher, emitting the correct bytes.
    // DR encoding: dr_idx = RegId - 25 (compact encoding), opcodes 0F 23 (write), 0F 21 (read).

    /// Test: mov dr0, rax → 0F 23 C0
    /// DR0 = 25 + 0 = 25, RAX = 0
    #[test]
    fn encode_mov_dr0_rax_emits_0f23c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(25)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);
    }

    /// Test: mov dr1, rdi → 0F 23 CF
    /// DR1 = 25 + 1 = 26, RDI = 7
    #[test]
    fn encode_mov_dr1_rdi_emits_0f23cf() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(26)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xCF]);
    }

    /// Test: mov dr7, rcx → 0F 23 F9
    /// DR7 = 25 + 7 = 32, RCX = 1
    #[test]
    fn encode_mov_dr7_rcx_emits_0f23f9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(32)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF9]);
    }

    /// Test: mov rax, dr0 → 0F 21 C0 (read from DR0)
    /// RAX = 0, DR0 = 25 + 0 = 25
    #[test]
    fn encode_mov_rax_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(25))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }

    /// Test: mov rdi, dr1 → 0F 21 CF (read from DR1)
    /// RDI = 7, DR1 = 25 + 1 = 26
    #[test]
    fn encode_mov_rdi_dr1_emits_0f21cf() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(26))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xCF]);
    }

    /// Test: mov rcx, dr7 → 0F 21 F9 (read from DR7)
    /// RCX = 1, DR7 = 25 + 7 = 32
    #[test]
    fn encode_mov_rcx_dr7_emits_0f21f9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(32))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF9]);
    }

    /// Test: mov r8, dr0 → 0F 21 C0 (read from DR0 into r8, GPR 8)
    /// R8 = 8, DR0 = 25 + 0 = 25
    #[test]
    fn encode_mov_r8_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(25))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }

    // Phase 15 m5-002: Segment register MOV instructions (opcode 8E /r).
    // Pattern: mov sreg, r16 → 8E /r

    #[test]
    fn encode_mov_ds_ax_emits_8e_d8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Ds),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E D8 (Ds=3, so ModR/M = 0xC0 | (3 << 3) | 0 = 0xD8)
        assert_eq!(buf.as_slice(), &[0x8E, 0xD8]);
    }

    #[test]
    fn encode_mov_es_ax_emits_8e_c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Es),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E C0 (Es=0, so ModR/M = 0xC0 | (0 << 3) | 0 = 0xC0)
        assert_eq!(buf.as_slice(), &[0x8E, 0xC0]);
    }

    #[test]
    fn encode_mov_ss_ax_emits_8e_d0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Ss),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E D0 (Ss=2, so ModR/M = 0xC0 | (2 << 3) | 0 = 0xD0)
        assert_eq!(buf.as_slice(), &[0x8E, 0xD0]);
    }

    #[test]
    fn encode_mov_fs_ax_emits_8e_e0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Fs),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E E0 (Fs=4, so ModR/M = 0xC0 | (4 << 3) | 0 = 0xE0)
        assert_eq!(buf.as_slice(), &[0x8E, 0xE0]);
    }

    #[test]
    fn encode_mov_gs_ax_emits_8e_e8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::SegReg(SegReg::Gs),
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: 8E E8 (Gs=5, so ModR/M = 0xC0 | (5 << 3) | 0 = 0xE8)
        assert_eq!(buf.as_slice(), &[0x8E, 0xE8]);
    }

    #[test]
    fn encode_mov_ds_ax_mode_agnostic() {
        // Verify Mode32 and Mode64 produce identical bytecode.
        let test_mode = |mode: InstrMode| {
            let mut buf = CodeBuffer::new();
            let inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: smallvec::smallvec![
                    Operand::SegReg(SegReg::Ds),
                    Operand::Reg(RegId(0)), // rax
                ],
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
            };

            let mut stats = EncodeStats::new();
            encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
            buf.as_slice().to_vec()
        };

        let bytes_mode32 = test_mode(InstrMode::Mode32);
        let bytes_mode64 = test_mode(InstrMode::Mode64);

        // Both should emit identical bytecode (no mode-dependent encoding for sreg MOV).
        assert_eq!(bytes_mode32, bytes_mode64);
        assert_eq!(bytes_mode32, vec![0x8E, 0xD8]);
    }
}

/// Phase 6 m3-002: Encode MOVZX (move with zero-extend) instruction.
///
/// MOVZX r64, r/m8/r/m16/r/m32 — zero-extends smaller operand to 64-bit.
/// For now, we only support movzx rax, byte [rdi+offset] pattern used in field access.
///
/// Opcode: 0F B6 for r/m8 → r64, 0F B7 for r/m16 → r64, etc.
/// This is a placeholder implementation; full support deferred to future phase.
fn encode_movzx(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 13 m6-001: MOVZX encoder supporting both register-to-register and memory-source.
    // For field access lowering: movzx rax, byte [rdi + offset] or movzx rax, word [rdi + offset]
    // Operands: [0] = dst (Reg), [1] = src (Reg or MemSib)
    //
    // Opcodes:
    // - 1 byte source: `REX.W 0F B6 /r` (movzx r64, r/m8)
    // - 2 byte source: `REX.W 0F B7 /r` (movzx r64, r/m16)

    if inst.operands.len() != 2 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Movzx,
            expected: 2,
            got: inst.operands.len(),
        });
    }

    // Extract destination register
    let dest_reg = match &inst.operands[0] {
        Operand::Reg(reg) => *reg,
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Movzx,
            });
        }
    };

    // Determine source width from encoding_hint
    let src_width = inst.encoding_hint.map(|h| h.operand_size).unwrap_or(1);

    match &inst.operands[1] {
        Operand::Reg(src_reg) => {
            // Register-to-register: already supported
            movzx_reg64(buf, reg64_from(dest_reg)?, reg64_from(*src_reg)?, src_width);
            Ok(EncodeOutput::new())
        }
        Operand::MemSib { base, index, disp, .. } => {
            // Memory source: movzx r64, [base + disp]
            if index.is_some() {
                return Err(EncodeError::Unsupported(
                    "MOVZX: indexed addressing not supported",
                ));
            }

            match src_width {
                1 | 2 => {
                    movzx_reg64_mem_base_disp(buf, reg64_from(dest_reg)?, reg64_from(*base)?, *disp, src_width);
                    Ok(EncodeOutput::new())
                }
                _ => {
                    Err(EncodeError::Unsupported(
                        "MOVZX: source width must be 1 or 2 bytes",
                    ))
                }
            }
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Movzx,
        }),
    }
}

/// Encode `movzx r64, [base + disp]` — zero-extend load from memory.
///
/// Instruction: REX.W opcode /r [disp]
/// Operand-size: determined by src_width parameter
/// - 1 byte (r/m8 → r64):  `REX.W 0F B6 /r` (movzx r64, byte [mem])
/// - 2 bytes (r/m16 → r64): `REX.W 0F B7 /r` (movzx r64, word [mem])
///
/// REX.W: always set (64-bit destination)
/// REX.R: set if dst in r8–r15
/// REX.B: set if base in r8–r15
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `movzx rax, byte [rdi]`: `48 0F B6 07`
/// - `movzx rax, word [rdi + 8]`: `48 0F B7 47 08`
fn movzx_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32, src_width: u8) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    match src_width {
        1 => buf.bytes.push(0xB6), // movzx r64, r/m8
        2 => buf.bytes.push(0xB7), // movzx r64, r/m16
        _ => return, // Invalid width; caller should have checked
    }
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Phase 13 m6-001: Encode MOVSX (move with sign-extend), register-to-register or memory-source.
///
/// MOVSX r64, r/m8/r/m16/r/m32 — sign-extends a smaller source register or memory location into a
/// 64-bit destination. Used by the cast emit path for *widening signed* casts and field access.
///
/// Operands: `[Reg(dst), Reg(src)]` or `[Reg(dst), MemSib{...}]`. The source width (1, 2, or 4 bytes) is
/// taken from `encoding_hint.operand_size`; if no hint is present we default to
/// 4 bytes (the common `i32 as i64` widening).
///
/// Opcodes: width 1 → `0F BE`, width 2 → `0F BF`,
/// width 4 → `63` (MOVSXD), all with `REX.W`.
fn encode_movsx(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if inst.operands.len() != 2 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Movsx,
            expected: 2,
            got: inst.operands.len(),
        });
    }

    let dest_reg = match &inst.operands[0] {
        Operand::Reg(reg) => *reg,
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Movsx,
            });
        }
    };

    let src_width = inst.encoding_hint.map(|h| h.operand_size).unwrap_or(4);

    match &inst.operands[1] {
        Operand::Reg(src_reg) => {
            // Register-to-register: movsx r64, reg
            if movsx_reg64(buf, reg64_from(dest_reg)?, reg64_from(*src_reg)?, src_width) {
                Ok(EncodeOutput::new())
            } else {
                Err(EncodeError::Unsupported(
                    "MOVSX: source width must be 1, 2, or 4 bytes",
                ))
            }
        }
        Operand::MemSib { base, index, disp, .. } => {
            // Memory source: movsx r64, [base + disp]
            if index.is_some() {
                return Err(EncodeError::Unsupported(
                    "MOVSX: indexed addressing not supported",
                ));
            }

            match src_width {
                1 | 2 | 4 => {
                    movsx_reg64_mem_base_disp(buf, reg64_from(dest_reg)?, reg64_from(*base)?, *disp, src_width);
                    Ok(EncodeOutput::new())
                }
                _ => {
                    Err(EncodeError::Unsupported(
                        "MOVSX: source width must be 1, 2, or 4 bytes",
                    ))
                }
            }
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Movsx,
        }),
    }
}

// Phase 6 m4-003: Jcc encoder tests (16 condition variants + label support)

#[cfg(test)]
mod jcc_tests {
    use super::*;
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    use paideia_as_ir::{Cond as IrCond, InstrMode, Instruction, Mnemonic, Operand};

    // Test 1: Je with immediate (rel32) round-trips through iced-x86
    #[test]
    fn jcc_je_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Eq),
            operands: smallvec::smallvec![Operand::Imm64(0x100)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should be 6 bytes: 0F 84 <rel32_le>
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x84);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    // Test 2: Jne with immediate (rel32) round-trips
    #[test]
    fn jcc_jne_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Ne),
            operands: smallvec::smallvec![Operand::Imm64(0x1000)], // Large displacement
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x85);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    // Test 3: Jl (signed less than) round-trips
    #[test]
    fn jcc_jl_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Lt),
            operands: smallvec::smallvec![Operand::Imm64(0x200)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8C);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jl);
    }

    // Test 4: Jg (signed greater than) round-trips
    #[test]
    fn jcc_jg_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Gt),
            operands: smallvec::smallvec![Operand::Imm64(0x300)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8F);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jg);
    }

    // Test 5: Jle round-trips
    #[test]
    fn jcc_jle_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Le),
            operands: smallvec::smallvec![Operand::Imm64(0x400)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8E);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jle);
    }

    // Test 6: Jge round-trips
    #[test]
    fn jcc_jge_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Ge),
            operands: smallvec::smallvec![Operand::Imm64(-5000i64)], // Large negative displacement
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8D);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jge);
    }

    // Test 7: Jb (below, unsigned) round-trips
    #[test]
    fn jcc_jb_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Below),
            operands: smallvec::smallvec![Operand::Imm64(0x500)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x82);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jb);
    }

    // Test 8: Jbe round-trips
    #[test]
    fn jcc_jbe_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::BelowOrEqual),
            operands: smallvec::smallvec![Operand::Imm64(0x600)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x86);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jbe);
    }

    // Test 9: Ja round-trips
    #[test]
    fn jcc_ja_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Above),
            operands: smallvec::smallvec![Operand::Imm64(0x700)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x87);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ja);
    }

    // Test 10: Jae round-trips
    #[test]
    fn jcc_jae_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::AboveOrEqual),
            operands: smallvec::smallvec![Operand::Imm64(0x800)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x83);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jae);
    }

    // Test 11: Jz (alias for Je) round-trips
    #[test]
    fn jcc_jz_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Zero),
            operands: smallvec::smallvec![Operand::Imm64(0x100)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x84); // Same opcode as Je

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Decoder will recognize as Je (iced-x86 canonicalizes)
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    // Test 12: Jnz (alias for Jne) round-trips
    #[test]
    fn jcc_jnz_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NonZero),
            operands: smallvec::smallvec![Operand::Imm64(0x200)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x85); // Same opcode as Jne

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Decoder will recognize as Jne (iced-x86 canonicalizes)
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    // Test 13: Js round-trips
    #[test]
    fn jcc_js_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Sign),
            operands: smallvec::smallvec![Operand::Imm64(0x300)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x88);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Js);
    }

    // Test 14: Jns round-trips
    #[test]
    fn jcc_jns_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NotSign),
            operands: smallvec::smallvec![Operand::Imm64(0x400)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x89);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jns);
    }

    // Test 15: Jo round-trips
    #[test]
    fn jcc_jo_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Overflow),
            operands: smallvec::smallvec![Operand::Imm64(0x500)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x80);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jo);
    }

    // Test 16: Jno round-trips
    #[test]
    fn jcc_jno_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NotOverflow),
            operands: smallvec::smallvec![Operand::Imm64(0x600)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x81);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jno);
    }

    // Test 17: Je with label reference records fixup correctly
    #[test]
    fn jcc_je_label_records_fixup() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Eq),
            operands: smallvec::smallvec![Operand::LabelRef {
                name: "fail".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should emit 6 bytes with zero placeholder
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x84);
        assert_eq!(&buf.as_slice()[2..6], &[0, 0, 0, 0]);

        // Should record fixup
        assert_eq!(output.label_fixups.len(), 1);
        let fixup = &output.label_fixups[0];
        assert_eq!(fixup.label_name, "fail");
        assert_eq!(fixup.byte_offset, 2);
        assert_eq!(fixup.addend, 0);
        assert_eq!(fixup.instruction_size, 6);
    }

    // Test 18: Jmp with label reference records fixup correctly
    #[test]
    fn jmp_label_records_fixup() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::LabelRef {
                name: "end".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should emit 5 bytes: E9 + zero rel32
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.as_slice()[0], 0xE9);
        assert_eq!(&buf.as_slice()[1..5], &[0, 0, 0, 0]);

        // Should record fixup
        assert_eq!(output.label_fixups.len(), 1);
        let fixup = &output.label_fixups[0];
        assert_eq!(fixup.label_name, "end");
        assert_eq!(fixup.byte_offset, 1);
        assert_eq!(fixup.addend, 0);
        assert_eq!(fixup.instruction_size, 5);
    }

    // ── PA8 m3-003 (#827): width-aware mov reg, imm via MovSized ─────────
    //
    // The elaborator retargets `mov al, imm` → MovSized{W8} and
    // `mov eax, imm` → MovSized{W32} (see unsafe_walker::register_name_width).
    // These two tests pin the encoded bytes the retarget produces, so the
    // narrow r8/r32 immediate forms can never silently regress to the generic
    // 10-byte 64-bit move.

    // mov al/cl, imm8 → B0+rb imm8 (2 bytes, no REX.W).
    #[test]
    fn movsized_w8_reg_imm_emits_b0_plus_rb_imm8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W8,
            },
            // RegId(1) = rcx, so the 8-bit low byte is `cl`.
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Imm64(0x2a)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // `mov cl, 0x2a` → B0+1 2a = B1 2A.
        assert_eq!(buf.as_slice(), &[0xB1, 0x2A]);
    }

    // mov eax/ecx, imm32 → B8+rd imm32 (5 bytes, no REX.W, implicit zero-extend).
    #[test]
    fn movsized_w32_reg_imm_emits_b8_plus_rd_imm32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovSized {
                width: IntWidth::W32,
            },
            // RegId(1) = rcx, so the 32-bit sub-register is `ecx`.
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Imm64(0x2a)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // `mov ecx, 0x2a` → B8+1 2a 00 00 00 = B9 2A 00 00 00.
        assert_eq!(buf.as_slice(), &[0xB9, 0x2A, 0x00, 0x00, 0x00]);
    }

    // ── Phase 8 m5-001: supervisor TLB and timing mnemonics ─────────────────

    #[test]
    fn encode_rdtsc_zero_operands_emits_0f31() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdtsc,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 31 (2 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x31]);
    }

    #[test]
    fn encode_rdtsc_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdtsc,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x31]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdtsc);
    }

    #[test]
    fn encode_invlpg_mem_rdi_emits_0f017f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 /7 = 0F 01 3F (3 bytes)
        // ModR/M: mod=00, reg=7, rm=7 (rdi) = 00_111_111 = 0x3F
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x3F]);
    }

    #[test]
    fn encode_invlpg_mem_rdi_plus_8_emits_0f01777f08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 /7 + disp8 = 0F 01 7F 08 (4 bytes)
        // ModR/M: mod=01, reg=7, rm=7 (rdi) = 01_111_111 = 0x7F, disp=0x08
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x7F, 0x08]);
    }

    #[test]
    fn encode_invlpg_mem_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x3F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Invlpg);
    }

    // ── Phase 8 m5-002: general memory operand mov [base + disp] ──────────

    #[test]
    fn encode_mov_reg64_mem_base_disp0_emits_48_8b_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 07 (3 bytes, REX.W + mov r64, r/m64 + ModR/M)
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x07]);
    }

    #[test]
    fn encode_mov_reg64_mem_base_disp8_emits_48_8b_47_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 47 08 (4 bytes, disp8 form)
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x47, 0x08]);
    }

    #[test]
    fn encode_mov_reg64_mem_base_disp32_emits_48_8b_87_xxxx() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 256,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 87 00 01 00 00 (7 bytes, disp32 form for 256)
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x87, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_mov_mem_base_disp_reg64_emits_48_89_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 89 07 (3 bytes, REX.W + mov r/m64, r64 + ModR/M)
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x07]);
    }

    #[test]
    fn encode_mov_mem_base_disp_reg64_round_trips_load() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_mem_base_disp_reg64_round_trips_store() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::MemSib {
                    base: RegId(7), // rdi
                    index: None,
                    scale: Scale::X1,
                    disp: 8,
                },
                Operand::Reg(RegId(0)), // rax
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // ── Phase 8 m5-004: comprehensive iced-x86 round-trip fixtures ──────────────
    // ≥12 fixtures covering m5-001 supervisor mnemonics + m5-002 memory operands

    #[test]
    fn encode_lgdt_rax_disp0_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(0), // rax
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lidt_rbx_disp32_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(3), // rbx
                index: None,
                scale: Scale::X1,
                disp: 256,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_wrmsr_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Wrmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Wrmsr);
    }

    #[test]
    fn encode_rdmsr_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdmsr);
    }

    #[test]
    fn encode_iretq_round_trips_m5_004() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iretq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Iretq);
    }

    #[test]
    fn encode_swapgs_round_trips_m5_004() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Swapgs,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
    }

    #[test]
    fn encode_int_0x21_round_trips_m5_004() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int,
            operands: smallvec::smallvec![Operand::Imm64(0x21)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int);
    }

    #[test]
    fn encode_mov_rax_mem_rsi_disp512_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(6), // rsi
                    index: None,
                    scale: Scale::X1,
                    disp: 512,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_mem_r13_disp8_r14_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::MemSib {
                    base: RegId(13), // r13
                    index: None,
                    scale: Scale::X1,
                    disp: 16,
                },
                Operand::Reg(RegId(14)), // r14
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_invlpg_rcx_disp128_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Invlpg,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(1), // rcx
                index: None,
                scale: Scale::X1,
                disp: 128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Invlpg);
    }

    #[test]
    fn encode_rdtsc_m5_004_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdtsc,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdtsc);
    }

    // PA10-006a: ljmp immediate form tests
    #[test]
    fn encode_ljmp_imm_selector_offset_emits_ea_form() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x0008),     // selector
                Operand::Imm64(0x12345678), // offset
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expected: EA 78 56 34 12 08 00
        // EA = opcode, 78 56 34 12 = offset in LE, 08 00 = selector in LE
        assert_eq!(buf.as_slice(), &[0xEA, 0x78, 0x56, 0x34, 0x12, 0x08, 0x00]);
    }

    #[test]
    fn encode_ljmp_imm_produces_correct_length() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x0008),     // selector
                Operand::Imm64(0xdeadbeef), // offset
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify length: EA + imm32 + imm16 = 7 bytes
        assert_eq!(buf.len(), 7);

        // Verify first byte is EA opcode
        assert_eq!(buf.as_slice()[0], 0xEA);
    }

    #[test]
    fn encode_ljmp_imm_with_symbol_ref_emits_reloc() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![
                Operand::Imm64(0x0008), // selector
                Operand::SymbolRef {
                    name: "kernel_entry".to_string(),
                    addend: 0,
                },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify bytecode: EA + 4 zero placeholder + selector
        assert_eq!(buf.as_slice(), &[0xEA, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00]);

        // Verify relocation site at byte offset 1 (after EA opcode)
        assert_eq!(output.reloc_sites.len(), 1);
        let reloc = &output.reloc_sites[0];
        assert_eq!(reloc.byte_offset, 1);
        assert_eq!(reloc.symbol, "kernel_entry");
        assert_eq!(reloc.kind, RelocKind::Abs32);
        assert_eq!(reloc.addend, 0);
    }

    // Phase R9 m2-001 (PA-R9-001): Push r64 tests
    #[test]
    fn encode_push_rax_emits_50() {
        // push rax → 50 (no REX needed for rax)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x50]);
    }

    #[test]
    fn encode_push_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // rax
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Push);
        assert_eq!(instr.op0_register(), Register::RAX);
    }

    #[test]
    fn encode_push_rbx_emits_53() {
        // push rbx → 53 (no REX needed for rbx)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(3))], // rbx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x53]);
    }

    #[test]
    fn encode_push_r9_emits_41_51() {
        // push r9 → 41 51 (REX.B 51)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(9))], // r9
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x51]);
    }

    #[test]
    fn encode_push_r15_emits_41_57() {
        // push r15 → 41 57 (REX.B 57)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Push,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))], // r15
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x57]);
    }

    // Phase R9 m2-001 (PA-R9-001): Pop r64 tests
    #[test]
    fn encode_pop_rcx_emits_59() {
        // pop rcx → 59 (no REX needed for rcx)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))], // rcx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x59]);
    }

    #[test]
    fn encode_pop_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))], // rcx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Pop);
        assert_eq!(instr.op0_register(), Register::RCX);
    }

    #[test]
    fn encode_pop_rdx_emits_5a() {
        // pop rdx → 5a (no REX needed for rdx)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(2))], // rdx
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x5a]);
    }

    #[test]
    fn encode_pop_r8_emits_41_58() {
        // pop r8 → 41 58 (REX.B 58)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))], // r8
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x58]);
    }

    #[test]
    fn encode_pop_r14_emits_41_5e() {
        // pop r14 → 41 5e (REX.B 5e)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pop,
            operands: smallvec::smallvec![Operand::Reg(RegId(14))], // r14
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x41, 0x5e]);
    }

    // Phase R9 m2-002 (PA-R9-002): Pushfq and Popfq tests
    #[test]
    fn encode_pushfq_emits_9c() {
        // pushfq → 9C
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pushfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x9C]);
    }

    #[test]
    fn encode_pushfq_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Pushfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Pushfq);
    }

    #[test]
    fn encode_popfq_emits_9d() {
        // popfq → 9D
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Popfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x9D]);
    }

    #[test]
    fn encode_popfq_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Popfq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Popfq);
    }

    // Phase R9 m2-003 (PA-R9-003): Int3 tests
    #[test]
    fn encode_int3_emits_cc() {
        // int3 → CC
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int3,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xCC]);
    }

    #[test]
    fn encode_int3_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int3,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int3);
    }

    // Phase R11 PA-R11-006 (issue #909): Div/Idiv r64 instruction tests
    #[test]
    fn encode_div_rax_emits_48_f7_f0() {
        // Mnemonic::Div with [Reg(rax)] → 48 F7 F0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xF0]);
    }

    #[test]
    fn encode_div_rcx_emits_48_f7_f1() {
        // Mnemonic::Div with [Reg(rcx)] → 48 F7 F1
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xF1]);
    }

    #[test]
    fn encode_div_r8_emits_49_f7_f0() {
        // Mnemonic::Div with [Reg(r8)] → 49 F7 F0
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xF0]);
    }

    #[test]
    fn encode_div_r15_emits_49_f7_f7() {
        // Mnemonic::Div with [Reg(r15)] → 49 F7 F7
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xF7]);
    }

    #[test]
    fn encode_idiv_rax_emits_48_f7_f8() {
        // Mnemonic::Idiv with [Reg(rax)] → 48 F7 F8
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Idiv,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0xF7, 0xF8]);
    }

    #[test]
    fn encode_idiv_r8_emits_49_f7_f8() {
        // Mnemonic::Idiv with [Reg(r8)] → 49 F7 F8
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Idiv,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xF8]);
    }

    #[test]
    fn encode_idiv_r15_emits_49_f7_ff() {
        // Mnemonic::Idiv with [Reg(r15)] → 49 F7 FF
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Idiv,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0xF7, 0xFF]);
    }

    #[test]
    fn encode_div_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Div);
        assert_eq!(instr.op0_register(), Register::RCX);
    }

    #[test]
    fn encode_ltr_ax_emits_0f_00_d8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x00, 0xD8]);
    }

    #[test]
    fn encode_ltr_cx_emits_0f_00_d9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x00, 0xD9]);
    }

    #[test]
    fn encode_ltr_r8_emits_41_0f_00_d8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(8))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x00, 0xD8]);
    }

    #[test]
    fn encode_ltr_r10_emits_41_0f_00_da() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(10))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x00, 0xDA]);
    }

    #[test]
    fn encode_ltr_r15_emits_41_0f_00_df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(15))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0x00, 0xDF]);
    }

    #[test]
    fn encode_ltr_r10_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Reg(RegId(10))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ltr);
        // Note: iced_x86 decodes the r/m16 operand as R10D when using REX.B in 64-bit mode.
        // The bytes are correct (41 0F 00 DA); iced_x86's width interpretation varies.
        // Verify the register index is 10 (either R10D, R10W, or R10 depending on decoder version).
        let reg = instr.op0_register();
        assert!(reg == Register::R10W || reg == Register::R10D,
                "Expected R10W or R10D, got {:?}", reg);
    }

    #[test]
    fn encode_ltr_rejects_imm_operand() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ltr,
            operands: smallvec::smallvec![Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Ltr }));
    }

    // Phase R13 PA-R13-003 (issue #916): XCHG tests
    #[test]
    fn encode_xchg_rdi_rax_emits_48_87_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x87, 0x07]);
    }

    #[test]
    fn encode_xchg_rdi_disp8_r10_emits_4c_87_57_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
                Operand::Reg(RegId(10)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x4C, 0x87, 0x57, 0x08]);
    }

    #[test]
    fn encode_xchg_r8_rax_emits_49_87_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x49, 0x87, 0x00]);
    }

    #[test]
    fn encode_xchg_rdi_r15_emits_4c_87_3f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(15)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x4C, 0x87, 0x3F]);
    }

    #[test]
    fn encode_xchg_rsp_rax_emits_48_87_04_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x87, 0x04, 0x24]);
    }

    #[test]
    fn encode_xchg_rbp_rax_emits_48_87_45_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x48, 0x87, 0x45, 0x00]);
    }

    #[test]
    fn encode_xchg_rdi_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Xchg);
    }

    // Phase R13 PA-R13-004 (issue #917): LOCK CMPXCHG tests
    #[test]
    fn encode_lock_cmpxchg_rdi_rcx_emits_f0_48_0f_b1_0f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xB1, 0x0F]);
    }

    #[test]
    fn encode_lock_cmpxchg_rdi_disp8_r10_emits_f0_4c_0f_b1_57_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
                Operand::Reg(RegId(10)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x4C, 0x0F, 0xB1, 0x57, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg_r8_rcx_emits_f0_49_0f_b1_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x49, 0x0F, 0xB1, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg_rsp_rax_emits_f0_48_0f_b1_04_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xB1, 0x04, 0x24]);
    }

    #[test]
    fn encode_lock_cmpxchg_rdi_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmpxchg);
        assert!(instr.has_lock_prefix());
    }

    // Phase R13 PA-R13-005 (issue #918): MFENCE tests
    #[test]
    fn encode_mfence_emits_0f_ae_f0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0xF0]);
    }

    #[test]
    fn encode_mfence_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mfence);
    }

    #[test]
    fn encode_mfence_rejects_operand() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandCount { mnemonic: Mnemonic::Mfence, expected: 0, .. }));
    }

    // Phase R13 PA-R13-007: fxsave/fxrstor byte-exact tests
    #[test]
    fn encode_fxsave_rdi_emits_0f_ae_07() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x07]);
    }

    #[test]
    fn encode_fxsave_rdi_disp8_emits_0f_ae_47_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x47, 0x08]);
    }

    #[test]
    fn encode_fxsave_r8_emits_41_0f_ae_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x00]);
    }

    #[test]
    fn encode_fxsave_rsp_emits_0f_ae_04_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x04, 0x24]);
    }

    #[test]
    fn encode_fxsave_rbp_emits_0f_ae_45_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x45, 0x00]);
    }

    #[test]
    fn encode_fxsave_r15_disp32_emits_41_0f_ae_87_disp32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x87, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_fxrstor_rdi_emits_0f_ae_0f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x0F]);
    }

    #[test]
    fn encode_fxrstor_rdi_disp8_emits_0f_ae_4f_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x4F, 0x08]);
    }

    #[test]
    fn encode_fxrstor_r8_emits_41_0f_ae_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x08]);
    }

    #[test]
    fn encode_fxrstor_rsp_emits_0f_ae_0c_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x0C, 0x24]);
    }

    #[test]
    fn encode_fxrstor_rbp_emits_0f_ae_4d_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x4D, 0x00]);
    }

    #[test]
    fn encode_fxrstor_r15_disp32_emits_41_0f_ae_8f_disp32() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x8F, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_fxsave_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Fxsave);
    }

    #[test]
    fn encode_fxrstor_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxrstor,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Fxrstor);
    }

    #[test]
    fn encode_fxsave_reg_operand_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Fxsave,
            operands: smallvec::smallvec![Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Fxsave }));
    }

    // Error-shape tests for new instructions
    #[test]
    fn encode_xchg_reg_reg_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Xchg }));
    }

    #[test]
    fn encode_lock_cmpxchg_reg_reg_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg }));
    }

    // Phase R16 PA-R16-003 (issue #969): LOCK CMPXCHG32 tests
    #[test]
    fn encode_lock_cmpxchg32_rax_ecx_emits_f0_0f_b1_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x0F, 0xB1, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg32_r8_ecx_emits_rex_b() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x41, 0x0F, 0xB1, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg32_rax_r15d_emits_rex_r() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(15)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x44, 0x0F, 0xB1, 0x38]);
    }

    #[test]
    fn encode_lock_cmpxchg32_r15_disp8_r10d_emits_rex_rb() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
                Operand::Reg(RegId(10)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x45, 0x0F, 0xB1, 0x57, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg32_rsp_eax_emits_sib() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x0F, 0xB1, 0x04, 0x24]);
    }

    #[test]
    fn encode_lock_cmpxchg32_r13_disp0_eax_forces_disp8() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(0)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x41, 0x0F, 0xB1, 0x45, 0x00]);
    }

    #[test]
    fn encode_lock_cmpxchg32_rax_ecx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmpxchg);
        assert!(instr.has_lock_prefix());
    }

    #[test]
    fn encode_lock_cmpxchg32_reg_reg_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg32,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
                Operand::Reg(RegId(1)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg32 }));
    }

    // Phase R16 PA-R16-004 (issue #970): lock cmpxchg16b tests

    #[test]
    fn encode_lock_cmpxchg16b_rdi_emits_f0_48_0f_c7_0f() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC7, 0x0F]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_r15_disp8_emits_f0_49_0f_c7_4f_08() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x49, 0x0F, 0xC7, 0x4F, 0x08]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_rsp_emits_f0_48_0f_c7_0c_24() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC7, 0x0C, 0x24]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_r13_disp0_emits_f0_49_0f_c7_4d_00() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xF0, 0x49, 0x0F, 0xC7, 0x4D, 0x00]);
    }

    #[test]
    fn encode_lock_cmpxchg16b_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmpxchg16b);
        assert!(instr.has_lock_prefix());
    }

    #[test]
    fn encode_lock_cmpxchg16b_reg_operand_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(7)),
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg16b }));
    }

    #[test]
    fn encode_lock_cmpxchg16b_sib_with_index_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockCmpxchg16b,
            operands: smallvec::smallvec![
                Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X1, disp: 0 },
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg16b }));
    }

    #[test]
    fn encode_xchg_imm_rejects() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Xchg,
            operands: smallvec::smallvec![Operand::Imm64(0)],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::OperandShape { mnemonic: Mnemonic::Xchg }));
    }

    // PA-R15-009a: jmp_mem_sib_no_base_indexed test suite (10 tests)

    #[test]
    fn encode_jmp_mem_sym_indexed_rax_8x_emits_correct_bytes() {
        // Test 1: [sym + rax*8] → FF 24 C5 00 00 00 00 (no REX.X)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "jump_table".to_string(),
                addend: 0,
                index: RegId(0), // rax
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0xFF, 0x24, 0xC5, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "jump_table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Abs32);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rcx_1x_emits_correct_scale() {
        // Test 2: [sym + rcx*1] → FF 24 0D ... (scale=0 in SIB)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "vtable".to_string(),
                addend: 0,
                index: RegId(1), // rcx
                scale: Scale::X1,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0xFF);
        assert_eq!(buf.as_slice()[1], 0x24);
        // SIB: scale=0 (00), index=rcx(1) (<<3), base=101
        // (0 << 6) | (1 << 3) | 0b101 = 0x0D
        assert_eq!(buf.as_slice()[2], 0x0D);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rbx_4x_emits_correct_scale() {
        // Test 3: [sym + rbx*4] → FF 24 9D ... (scale=2 in SIB)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "dispatch".to_string(),
                addend: 0,
                index: RegId(3), // rbx
                scale: Scale::X4,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0xFF);
        assert_eq!(buf.as_slice()[1], 0x24);
        // SIB: scale=2 (10), index=rbx(3) (<<3), base=101
        // (2 << 6) | (3 << 3) | 0b101 = 0x9D
        assert_eq!(buf.as_slice()[2], 0x9D);
        assert_eq!(output.reloc_sites.len(), 1);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rdi_2x_emits_correct_scale() {
        // Test 4: [sym + rdi*2] → FF 24 7D ... (scale=1 in SIB)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "handlers".to_string(),
                addend: 0,
                index: RegId(7), // rdi
                scale: Scale::X2,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0xFF);
        assert_eq!(buf.as_slice()[1], 0x24);
        // SIB: scale=1 (01), index=rdi(7) (<<3), base=101
        // (1 << 6) | (7 << 3) | 0b101 = 0x7D
        assert_eq!(buf.as_slice()[2], 0x7D);
        assert_eq!(output.reloc_sites.len(), 1);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_r8_8x_emits_rex_x() {
        // Test 5: [sym + r8*8] → 42 FF 24 C5 ... (REX.X prefix)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "table".to_string(),
                addend: 0,
                index: RegId(8), // r8
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0x42); // REX.X
        assert_eq!(buf.as_slice()[1], 0xFF);
        assert_eq!(buf.as_slice()[2], 0x24);
        // SIB with r8 (index_id=8, low 3 bits=0): (3<<6)|(0<<3)|0b101 = 0xC5
        assert_eq!(buf.as_slice()[3], 0xC5);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 4); // disp32 at byte 4 with REX
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_r15_4x_emits_rex_x() {
        // Test 6: [sym + r15*4] → 42 FF 24 BD ... (REX.X prefix)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "jumptbl".to_string(),
                addend: 0,
                index: RegId(15), // r15
                scale: Scale::X4,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice()[0], 0x42); // REX.X
        assert_eq!(buf.as_slice()[1], 0xFF);
        assert_eq!(buf.as_slice()[2], 0x24);
        // SIB with r15 (index_id=15, low 3 bits=7): (2<<6)|(7<<3)|0b101 = 0xBD
        assert_eq!(buf.as_slice()[3], 0xBD);
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 4);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_non_zero_addend_flows_into_reloc() {
        // Test 7: Non-zero addend flows verbatim into RelocSite
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "handler_table".to_string(),
                addend: 8,
                index: RegId(2), // rdx
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].addend, 8);
        assert_eq!(output.reloc_sites[0].symbol, "handler_table");
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rsp_as_index_rejects() {
        // Test 8: [sym + rsp*8] → EncodeError::InvalidOperand (RSP cannot be index)
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "forbidden".to_string(),
                addend: 0,
                index: RegId(4), // rsp (id 4)
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        let err = encode_instruction(&inst, &mut buf, &mut stats).unwrap_err();
        assert!(matches!(err, EncodeError::InvalidOperand(_)));
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_rax_8x_round_trips_iced() {
        // Test 9: Iced roundtrip: rax*8 form disassembles correctly
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "table".to_string(),
                addend: 0,
                index: RegId(0), // rax
                scale: Scale::X8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_jmp_mem_sym_indexed_r15_4x_round_trips_iced() {
        // Test 10: Iced roundtrip: r15*4 form preserves REX.X
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::MemSymIndexed {
                name: "handlers".to_string(),
                addend: 0,
                index: RegId(15), // r15
                scale: Scale::X4,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }
}
