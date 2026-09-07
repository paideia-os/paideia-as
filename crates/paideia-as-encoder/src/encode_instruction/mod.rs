//! Mnemonic <-> encoder bridge.
//!
//! `encode_instruction(inst, &mut buf)` dispatches to the per-mnemonic
//! encoder primitives already shipping in `encode.rs`. Historically this
//! module lived as a single 11k-line file; paideia-as#1400 split its
//! per-mnemonic-family bodies into sibling sub-modules while keeping
//! every previously-public path (`encode_instruction::EncodeError`, …,
//! `encode_instruction::encode_instruction`) reachable at the same
//! location via `pub use`. Behaviour is unchanged; the dispatcher
//! itself and the sole entry points still live here.
//!
//! Sub-module layout (each mirrors one mnemonic family from the original
//! flat file and receives its helpers via `use super::*;`):
//!
//! - [`types`]         — public `EncodeError`, `EncodeOutput`, `EncodeStats`,
//!                       `LabelFixup`, `RelocKind`, `RelocSite`
//! - [`common`]        — shared conversions and predicates
//!                       (`reg64_from`, `reg32_from`, `cond_from`,
//!                        `find_mem_seg`, `sib_scale_bits`, `resolve_reg8`,
//!                        `rex`, `can_use_rel8`, `can_shorten_add_to_32bit`)
//! - [`sse`]           — scalar SSE dispatchers
//! - [`mov`]           — `encode_mov`, `encode_mov_sized`
//! - [`mov_special`]   — CR/DR MOV dispatch + `movnti`
//! - [`mov_ext`]       — `movzx` / `movsx` and their memory helper
//! - [`simple_alu`]    — small ALU ops (not/bswap/div/idiv/mul/inc/dec/ltr)
//! - [`atomic_lock`]   — xchg + all LOCK-prefixed encoders
//! - [`system_cache`]  — fences, pause, cache-line and XSAVE helpers
//! - [`stack`]         — push/pop/pushfq/popfq/int3
//! - [`shifts`]        — shl/shr/sar/rol/ror
//! - [`arith`]         — add/sub/adc/sbb
//! - [`bit_scan`]      — popcnt/bsf/bsr/tzcnt/crc32
//! - [`bit_test`]      — bt/bts/btr/btc
//! - [`cmp_test`]      — cmp (+ per-shape helpers) and test
//! - [`branch`]        — jcc/setcc/jmp/call/ret/far_jmp
//! - [`string_ops`]    — rep movsb/movsq/stosb/stosq
//! - [`lea`]           — load-effective-address
//! - [`system_flags`]  — cli/cld/sti/std/hlt/nop/swapgs/cpuid/ud2/endbr*
//! - [`io_ports`]      — in/out
//! - [`msr`]           — wrmsr/rdmsr/xgetbv/xsetbv
//! - [`interrupt`]     — int/iret(q)/sysret/syscall
//! - [`descriptor`]    — lgdt/lidt
//! - [`tlb`]           — rdtsc/invlpg/invpcid
//! - [`avx`]           — vpxor/vpcmpeqb/vpmovmskb/vmovdqu

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

// Per-mnemonic-family sub-modules. Each is `use super::*;` internally and
// exports its encoders as `pub(super)` so the dispatcher below (and any
// sibling module that needs a helper) can call them via the glob-imports
// that follow.
mod arith;
mod atomic_lock;
mod avx;
mod bit_scan;
mod bit_test;
mod branch;
mod cmp_test;
mod common;
mod descriptor;
mod interrupt;
mod io_ports;
mod lea;
mod mov;
mod mov_ext;
mod mov_special;
mod msr;
mod shifts;
mod simple_alu;
mod sse;
mod stack;
mod string_ops;
mod system_cache;
mod system_flags;
mod tlb;
mod types;

// Glob-import every sub-module's `pub(super)` items into this file's
// namespace. The dispatcher below calls them by their bare names (unchanged
// from the pre-split file) and sibling sub-modules see them transitively via
// `use super::*;`.
use arith::*;
use atomic_lock::*;
use avx::*;
use bit_scan::*;
use bit_test::*;
use branch::*;
use cmp_test::*;
use common::*;
use descriptor::*;
use interrupt::*;
use io_ports::*;
use lea::*;
use mov::*;
use mov_ext::*;
use mov_special::*;
use msr::*;
use shifts::*;
use simple_alu::*;
use sse::*;
use stack::*;
use string_ops::*;
use system_cache::*;
use system_flags::*;
use tlb::*;

// Preserve the public paths that lib.rs re-exports and that downstream
// crates (paideia-as, paideia-as-elaborator, paideia-satellite-runtime,
// paideia-as-emitter-elf, etc.) address as
// `paideia_as_encoder::encode_instruction::…`.
pub use types::{EncodeError, EncodeOutput, EncodeStats, LabelFixup, RelocKind, RelocSite};

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
        Mnemonic::Crc32 { width } => encode_crc32(inst, buf, *width),
        Mnemonic::Bsf { width } => encode_bsf(inst, buf, *width),
        Mnemonic::Bsr { width } => encode_bsr(inst, buf, *width),
        Mnemonic::Tzcnt { width } => encode_tzcnt(inst, buf, *width),
        Mnemonic::Bt { width } => encode_bt(inst, buf, *width),
        Mnemonic::Bts { width } => encode_bts(inst, buf, *width),
        Mnemonic::Btr { width } => encode_btr(inst, buf, *width),
        Mnemonic::Btc { width } => encode_btc(inst, buf, *width),
        Mnemonic::Cmp => encode_cmp(inst, buf),
        Mnemonic::CmpSized { width } => encode_cmp_sized(inst, width, buf),
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
        Mnemonic::Endbr64 => encode_endbr64(inst, buf),
        Mnemonic::Endbr32 => encode_endbr32(inst, buf),
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
        Mnemonic::RepStosb => encode_rep_stosb_inst(inst, buf),
        Mnemonic::RepMovsq => encode_rep_movsq_inst(inst, buf),
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
        // v0.21-009-followup (#1297): invpcid r64, m128
        Mnemonic::Invpcid => encode_invpcid_inst(inst, buf),
        Mnemonic::Rdtsc => encode_rdtsc_inst(inst, buf),
        // Phase R11 PA-R11-006: divide instructions
        Mnemonic::Div => encode_div(inst, buf),
        Mnemonic::Idiv => encode_idiv(inst, buf),
        // paideia-as#1398: unsigned wide multiply — mul r64 (REX.W F7 /4).
        Mnemonic::Mul => encode_mul(inst, buf),
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
        // Phase R15 PA-R15-m4-005 (issue #1022): xsaveopt/xrstor to memory
        Mnemonic::Xsaveopt => encode_xsaveopt_inst(inst, buf),
        Mnemonic::Xrstor => encode_xrstor_inst(inst, buf),
        // v0.21-015 (paideia-as#1294): XCR0 access — extended control-register
        // read/write, zero explicit operands (implicit ECX index / EDX:EAX value).
        Mnemonic::Xgetbv => encode_xgetbv_inst(inst, buf),
        Mnemonic::Xsetbv => encode_xsetbv_inst(inst, buf),
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
        // Phase R18 PA-R18-011 (issue #1004): AVX2 VEX-prefixed mnemonics
        Mnemonic::Vpxor => encode_vpxor(inst, buf),
        Mnemonic::Vpcmpeqb => encode_vpcmpeqb(inst, buf),
        Mnemonic::Vpmovmskb => encode_vpmovmskb(inst, buf),
        Mnemonic::Vmovdqu { is_store } => encode_vmovdqu(inst, buf, *is_store),
        // paideia-os #1333, paideia-as#1333: scalar SSE float (register-register).
        Mnemonic::MovSd => encode_sse_xmm_xmm(inst, Mnemonic::MovSd, Some(0xF2), 0x10, buf),
        Mnemonic::MovSs => encode_sse_xmm_xmm(inst, Mnemonic::MovSs, Some(0xF3), 0x10, buf),
        Mnemonic::AddSd => encode_sse_xmm_xmm(inst, Mnemonic::AddSd, Some(0xF2), 0x58, buf),
        Mnemonic::AddSs => encode_sse_xmm_xmm(inst, Mnemonic::AddSs, Some(0xF3), 0x58, buf),
        Mnemonic::SubSd => encode_sse_xmm_xmm(inst, Mnemonic::SubSd, Some(0xF2), 0x5C, buf),
        Mnemonic::SubSs => encode_sse_xmm_xmm(inst, Mnemonic::SubSs, Some(0xF3), 0x5C, buf),
        Mnemonic::MulSd => encode_sse_xmm_xmm(inst, Mnemonic::MulSd, Some(0xF2), 0x59, buf),
        Mnemonic::MulSs => encode_sse_xmm_xmm(inst, Mnemonic::MulSs, Some(0xF3), 0x59, buf),
        Mnemonic::DivSd => encode_sse_xmm_xmm(inst, Mnemonic::DivSd, Some(0xF2), 0x5E, buf),
        Mnemonic::DivSs => encode_sse_xmm_xmm(inst, Mnemonic::DivSs, Some(0xF3), 0x5E, buf),
        Mnemonic::Sqrtsd => encode_sse_xmm_xmm(inst, Mnemonic::Sqrtsd, Some(0xF2), 0x51, buf),
        Mnemonic::Sqrtss => encode_sse_xmm_xmm(inst, Mnemonic::Sqrtss, Some(0xF3), 0x51, buf),
        Mnemonic::Ucomisd => encode_sse_xmm_xmm(inst, Mnemonic::Ucomisd, Some(0x66), 0x2E, buf),
        Mnemonic::Ucomiss => encode_sse_xmm_xmm(inst, Mnemonic::Ucomiss, None, 0x2E, buf),
        Mnemonic::Comisd => encode_sse_xmm_xmm(inst, Mnemonic::Comisd, Some(0x66), 0x2F, buf),
        Mnemonic::Comiss => encode_sse_xmm_xmm(inst, Mnemonic::Comiss, None, 0x2F, buf),
        Mnemonic::Cvtsi2sd => encode_cvtsi2s_inst(inst, Mnemonic::Cvtsi2sd, 0xF2, buf),
        Mnemonic::Cvtsi2ss => encode_cvtsi2s_inst(inst, Mnemonic::Cvtsi2ss, 0xF3, buf),
        Mnemonic::Cvttsd2si => encode_cvtts2si_inst(inst, Mnemonic::Cvttsd2si, 0xF2, buf),
        Mnemonic::Cvttss2si => encode_cvtts2si_inst(inst, Mnemonic::Cvttss2si, 0xF3, buf),
        Mnemonic::MovdBitcast { to_xmm } => encode_movd_bitcast_inst(inst, *to_xmm, buf),
        Mnemonic::MovqBitcast { to_xmm } => encode_movq_bitcast_inst(inst, *to_xmm, buf),
    }
}

#[cfg(test)]
mod tests;

// Phase 6 m4-003: Jcc encoder tests (16 condition variants + label support)
#[cfg(test)]
mod jcc_tests;
