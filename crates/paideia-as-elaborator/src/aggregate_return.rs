//! SysV AMD64 aggregate return-value instruction sequences.
//!
//! PAS-DEBT-B3-007c (paideia-as#1544). Consumes the classifier landed in
//! Slice 1 ([`paideia_as_ir::abi::classify_sysv_aggregate`],
//! [`paideia_as_ir::abi::sysv_return_placement`]) and synthesises the
//! byte-exact instruction sequences the SysV AMD64 psABI §3.2.3 requires
//! at:
//!
//!   * **Caller-side sret prelude** — before a CALL whose callee returns
//!     a Memory-classified aggregate, the caller must compute the hidden
//!     buffer address into `RDI` as the implicit first argument.
//!   * **Caller-side return-pair fetch** — after a CALL whose callee
//!     returns a ≤ 16-byte aggregate split across two return registers,
//!     the caller reads `RAX`/`RDX`/`XMM0`/`XMM1` back into a
//!     destination buffer per the [`SysvReturnPlacement`] shape.
//!   * **Callee-side return-pair epilogue** — before RET, the callee
//!     loads each eightbyte from its source buffer into the return
//!     register the shape names.
//!   * **Callee-side sret epilogue** — the callee copies the aggregate
//!     into the caller-provided buffer at `[RDI]` and returns that same
//!     pointer in `RAX`.
//!
//! # Design shape — free functions returning `Vec<Instruction>`
//!
//! Every helper takes only its geometric inputs (placement, buffer base
//! register, byte displacement, aggregate size) and returns a plain
//! `Vec<Instruction>` ready to splice into an emitter pipeline. This
//! separation keeps the byte-exact tests in this file (which do not need
//! an EmitWalker) small and pins the SysV rules independently of the
//! call-lowering machinery — which in turn keeps the sret / return-pair
//! rules debuggable in isolation from arg-marshalling, scratch-save
//! bookkeeping, MS-x64 shadow-space, and the pos-0 pair-enum path in
//! `emit_call.rs`.
//!
//! End-to-end wiring at concrete call sites remains staged behind
//! upstream work: paideia-as has no return-record-layout side-table on
//! `Symbol` today, so the classifier consumer here is called by the
//! byte-exact unit tests and reserved for the eventual return-layout
//! lookup wire-up. See `.plans/scratch/CHANGELOG-1544-b3007c-sysv-return.md`
//! for the follow-up ticket sketch.
//!
//! # SSE eightbyte handling
//!
//! `Mnemonic::MovSd` / `MovSs` are register-register only in this
//! encoder; memory-form scalar SSE moves are a deferred encoder
//! extension. For eightbytes that live in a memory buffer we round-trip
//! through a scratch GPR using `MovqBitcast`:
//!
//! ```text
//!   ; load-XMM-from-mem
//!   mov  <scratch_gpr>, [buf + disp]
//!   movq xmm<k>, <scratch_gpr>        ; MovqBitcast { to_xmm: true }
//!
//!   ; store-XMM-to-mem
//!   movq <scratch_gpr>, xmm<k>        ; MovqBitcast { to_xmm: false }
//!   mov  [buf + disp], <scratch_gpr>
//! ```
//!
//! This produces byte-identical semantics to a memory-form `movsd`
//! (both write the low eightbyte); the round-trip's only cost is the
//! extra scratch GPR use, which the callee/caller has spare at RET/CALL
//! boundaries. The scratch defaults to `R10` — caller-saved and
//! disjoint from every SysV integer-return register (`RAX`, `RDX`) and
//! from the sret-pointer register (`RDI`), so the shim never aliases a
//! live return-value register.
//!
//! # Reserved-label hygiene
//!
//! No labels are emitted by this module (it produces only register/
//! memory MOV/LEA/MOVQ instructions), so the `loop`/`if`/`let` reserved
//! word pitfall documented in the debt catalog does not apply here.

use paideia_as_ir::abi;
use paideia_as_ir::instruction::{InstrMode, Instruction, Mnemonic, Operand, RegId, Scale};
use paideia_as_ir::abi::SysvReturnPlacement;
use paideia_as_ir::SmallVec;

/// Default scratch GPR used to round-trip SSE eightbytes through a GPR
/// when loading/storing to memory (see module-level SSE note).
///
/// `R10` is chosen because:
///   * It is caller-saved (no epilogue-side save/restore needed).
///   * It is not in the SysV integer arg-register pool (`RDI, RSI, RDX,
///     RCX, R8, R9`), so a caller writing arg-marshalling code around
///     an sret-shaped call can use it without arg-slot aliasing.
///   * It does not overlap `RAX` (INTEGER return low) or `RDX` (INTEGER
///     return high) — the two GPRs a return-pair sequence might already
///     be writing.
///   * It is disjoint from `RDI`, the sret hidden-pointer register.
const SSE_SCRATCH_GPR: RegId = abi::R10;

// ── low-level instruction helpers ───────────────────────────────────────

/// `mov dest_reg, [base + disp]` — 64-bit load (REX.W).
fn mov_reg_from_mem(dest: RegId, base: RegId, disp: i32) -> Instruction {
    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
    ops.push(Operand::Reg(dest));
    ops.push(Operand::MemSib { base, index: None, scale: Scale::X1, disp });
    Instruction {
        mnemonic: Mnemonic::Mov,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

/// `mov [base + disp], src_reg` — 64-bit store (REX.W).
fn mov_mem_from_reg(base: RegId, disp: i32, src: RegId) -> Instruction {
    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
    ops.push(Operand::MemSib { base, index: None, scale: Scale::X1, disp });
    ops.push(Operand::Reg(src));
    Instruction {
        mnemonic: Mnemonic::Mov,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

/// `mov dest_reg, src_reg` — 64-bit register-register move.
fn mov_reg_from_reg(dest: RegId, src: RegId) -> Instruction {
    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
    ops.push(Operand::Reg(dest));
    ops.push(Operand::Reg(src));
    Instruction {
        mnemonic: Mnemonic::Mov,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

/// `lea dest_reg, [base + disp]` — compute an effective address.
fn lea_reg_from_mem(dest: RegId, base: RegId, disp: i32) -> Instruction {
    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
    ops.push(Operand::Reg(dest));
    ops.push(Operand::MemSib { base, index: None, scale: Scale::X1, disp });
    Instruction {
        mnemonic: Mnemonic::Lea,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

/// `movq xmm_dst, gpr_src` — 64-bit bitcast GPR → XMM (`66 REX.W 0F 6E /r`).
fn movq_bitcast_to_xmm(xmm_dst: RegId, gpr_src: RegId) -> Instruction {
    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
    ops.push(Operand::Reg(xmm_dst));
    ops.push(Operand::Reg(gpr_src));
    Instruction {
        mnemonic: Mnemonic::MovqBitcast { to_xmm: true },
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

/// `movq gpr_dst, xmm_src` — 64-bit bitcast XMM → GPR (`66 REX.W 0F 7E /r`).
fn movq_bitcast_from_xmm(gpr_dst: RegId, xmm_src: RegId) -> Instruction {
    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
    ops.push(Operand::Reg(gpr_dst));
    ops.push(Operand::Reg(xmm_src));
    Instruction {
        mnemonic: Mnemonic::MovqBitcast { to_xmm: false },
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

/// `ret` — bare return (no epilogue frame teardown here; the caller
/// composes this sequence into a larger epilogue that emits frame
/// unwind separately, matching `EmitWalker::emit_ret`).
fn ret_inst() -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Ret,
        operands: SmallVec::new(),
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    }
}

// ── SSE eightbyte round-trip via GPR scratch ───────────────────────────

/// Emit the load-XMM-from-mem sequence:
/// `mov <scratch>, [base + disp]; movq xmm_dst, <scratch>`.
fn load_xmm_from_mem(xmm_dst: RegId, base: RegId, disp: i32) -> Vec<Instruction> {
    vec![
        mov_reg_from_mem(SSE_SCRATCH_GPR, base, disp),
        movq_bitcast_to_xmm(xmm_dst, SSE_SCRATCH_GPR),
    ]
}

/// Emit the store-XMM-to-mem sequence:
/// `movq <scratch>, xmm_src; mov [base + disp], <scratch>`.
fn store_xmm_to_mem(base: RegId, disp: i32, xmm_src: RegId) -> Vec<Instruction> {
    vec![
        movq_bitcast_from_xmm(SSE_SCRATCH_GPR, xmm_src),
        mov_mem_from_reg(base, disp, SSE_SCRATCH_GPR),
    ]
}

// ── public: caller-side helpers ─────────────────────────────────────────

/// Caller-side sret prelude: `lea rdi, [dest_base + dest_disp]`.
///
/// Emitted **before** the CALL to a Memory-classified aggregate-returning
/// callee. `dest_base + dest_disp` must name a caller-owned buffer at
/// least `layout.size` bytes large; SysV requires it to be 16-byte
/// aligned. The rest of the arg-marshalling then continues into
/// `RSI, RDX, RCX, R8, R9` — i.e. the real args shift right by one.
///
/// PAS-DEBT-B3-007c / paideia-as#1544.
#[must_use]
pub fn sysv_caller_sret_prelude(dest_base: RegId, dest_disp: i32) -> Vec<Instruction> {
    vec![lea_reg_from_mem(abi::RDI, dest_base, dest_disp)]
}

/// Caller-side return-pair fetch: after CALL, write the return
/// eightbytes from their placement registers into a caller-owned
/// destination buffer at `[dest_base + dest_disp]`.
///
/// Handles all six non-Memory shapes documented on
/// [`SysvReturnPlacement`]; `Memory` and `None` return the empty
/// sequence (Memory's return value already lives in the sret buffer at
/// `RAX`, and None has nothing to place).
///
/// The `.pdx` end-to-end fixtures that would exercise this at source
/// level require record-return language support that has not landed
/// yet; the byte-exact sequences here are pinned by this module's unit
/// tests and are ready to splice from `emit_call.rs` once the upstream
/// return-record-layout side-table lands.
///
/// PAS-DEBT-B3-007c / paideia-as#1544.
#[must_use]
pub fn sysv_caller_read_return_pair(
    placement: SysvReturnPlacement,
    dest_base: RegId,
    dest_disp: i32,
) -> Vec<Instruction> {
    let mut out = Vec::new();
    match placement {
        SysvReturnPlacement::None | SysvReturnPlacement::Memory => {}
        SysvReturnPlacement::IntSingle => {
            out.push(mov_mem_from_reg(dest_base, dest_disp, abi::RAX));
        }
        SysvReturnPlacement::SseSingle => {
            out.extend(store_xmm_to_mem(dest_base, dest_disp, abi::XMM0));
        }
        SysvReturnPlacement::IntPair => {
            // low → [buf+0] from RAX, high → [buf+8] from RDX.
            out.push(mov_mem_from_reg(dest_base, dest_disp, abi::RAX));
            out.push(mov_mem_from_reg(dest_base, dest_disp + 8, abi::RDX));
        }
        SysvReturnPlacement::IntSse => {
            // low INTEGER in RAX → [buf+0]; high SSE in XMM0 → [buf+8]
            // via the GPR bitcast shim.
            out.push(mov_mem_from_reg(dest_base, dest_disp, abi::RAX));
            out.extend(store_xmm_to_mem(dest_base, dest_disp + 8, abi::XMM0));
        }
        SysvReturnPlacement::SseInt => {
            // low SSE in XMM0 → [buf+0]; high INTEGER in RAX → [buf+8].
            out.extend(store_xmm_to_mem(dest_base, dest_disp, abi::XMM0));
            out.push(mov_mem_from_reg(dest_base, dest_disp + 8, abi::RAX));
        }
        SysvReturnPlacement::SsePair => {
            // low SSE in XMM0 → [buf+0]; high SSE in XMM1 → [buf+8].
            out.extend(store_xmm_to_mem(dest_base, dest_disp, abi::XMM0));
            out.extend(store_xmm_to_mem(dest_base, dest_disp + 8, abi::XMM1));
        }
    }
    out
}

// ── public: callee-side helpers ─────────────────────────────────────────

/// Callee-side return-pair epilogue prelude: load the return eightbytes
/// from a source buffer at `[buf_base + buf_disp]` into the placement's
/// return registers. Does NOT emit the trailing `RET` — the caller
/// composes this with the frame-pointer epilogue (mirrors
/// `EmitWalker::emit_ret`'s split of add-rsp / mov-rsp-rbp / pop-rbp /
/// ret across separate `emit_inst` calls).
///
/// For `Memory` and `None`, returns the empty sequence; see
/// [`sysv_callee_sret_store`] for the Memory-return callee side.
///
/// PAS-DEBT-B3-007c / paideia-as#1544.
#[must_use]
pub fn sysv_callee_load_return_pair(
    placement: SysvReturnPlacement,
    buf_base: RegId,
    buf_disp: i32,
) -> Vec<Instruction> {
    let mut out = Vec::new();
    match placement {
        SysvReturnPlacement::None | SysvReturnPlacement::Memory => {}
        SysvReturnPlacement::IntSingle => {
            out.push(mov_reg_from_mem(abi::RAX, buf_base, buf_disp));
        }
        SysvReturnPlacement::SseSingle => {
            out.extend(load_xmm_from_mem(abi::XMM0, buf_base, buf_disp));
        }
        SysvReturnPlacement::IntPair => {
            out.push(mov_reg_from_mem(abi::RAX, buf_base, buf_disp));
            out.push(mov_reg_from_mem(abi::RDX, buf_base, buf_disp + 8));
        }
        SysvReturnPlacement::IntSse => {
            out.push(mov_reg_from_mem(abi::RAX, buf_base, buf_disp));
            out.extend(load_xmm_from_mem(abi::XMM0, buf_base, buf_disp + 8));
        }
        SysvReturnPlacement::SseInt => {
            out.extend(load_xmm_from_mem(abi::XMM0, buf_base, buf_disp));
            out.push(mov_reg_from_mem(abi::RAX, buf_base, buf_disp + 8));
        }
        SysvReturnPlacement::SsePair => {
            out.extend(load_xmm_from_mem(abi::XMM0, buf_base, buf_disp));
            out.extend(load_xmm_from_mem(abi::XMM1, buf_base, buf_disp + 8));
        }
    }
    out
}

/// Callee-side sret store: copy a Memory-classified aggregate from
/// `[src_base + src_disp]` into the caller-provided buffer at `[RDI]`,
/// then move `RDI` into `RAX` (SysV requires the callee to return the
/// hidden-pointer buffer address in `RAX` per psABI §3.2.3).
///
/// Precondition: `RDI` must still hold the entry-time hidden-pointer
/// argument. The callee body must therefore either preserve `RDI`
/// across its local computation or restore it before calling this
/// helper. Both restrictions match the sret ABI contract (the caller
/// passes RDI once at CALL time and expects the callee to give the same
/// pointer back in RAX).
///
/// Bulk copy strategy: for aggregates whose size is a multiple of 8
/// bytes, emit `size / 8` qword MOVs via the SSE scratch GPR. Sizes
/// that are not a multiple of 8 (17–23, 25–31, …) are deferred: today
/// the classifier only produces Memory for size > 16, and every
/// aggregate that reaches this helper in a real callee will have been
/// laid out with 8-byte-aligned tail padding by
/// [`paideia_as_ir::RecordLayout`]. The `size_bytes` argument is
/// asserted to be a multiple of 8; non-multiples are a caller bug and
/// panic with a diagnostic message.
///
/// PAS-DEBT-B3-007c / paideia-as#1544. Tail-byte handling for oddly-
/// sized aggregates is a follow-up (would emit a REP MOVSB variant or
/// a byte-granular MOV chain — deferred until a real fixture exercises
/// a non-multiple-of-8 aggregate).
///
/// # Panics
///
/// Panics if `size_bytes` is zero (nothing to copy — Memory placement
/// should never apply to a zero-sized aggregate; the classifier maps
/// zero-size to `[]`, not `[Memory]`), or if `size_bytes % 8 != 0`
/// (see the note above).
#[must_use]
pub fn sysv_callee_sret_store(
    size_bytes: u32,
    src_base: RegId,
    src_disp: i32,
) -> Vec<Instruction> {
    assert!(
        size_bytes > 0,
        "sysv_callee_sret_store: zero-size aggregate cannot be Memory-classified"
    );
    assert!(
        size_bytes % 8 == 0,
        "sysv_callee_sret_store: non-multiple-of-8 aggregate size {} not yet supported \
         (follow-up: tail-byte MOV chain / REP MOVSB variant)",
        size_bytes
    );

    let qwords = (size_bytes / 8) as i32;
    let mut out = Vec::with_capacity((qwords as usize) * 2 + 1);
    for i in 0..qwords {
        let off = i * 8;
        // load [src_base + src_disp + off] → SSE_SCRATCH_GPR
        out.push(mov_reg_from_mem(SSE_SCRATCH_GPR, src_base, src_disp + off));
        // store SSE_SCRATCH_GPR → [RDI + off]
        out.push(mov_mem_from_reg(abi::RDI, off, SSE_SCRATCH_GPR));
    }
    // Return the sret buffer pointer in RAX per psABI §3.2.3.
    out.push(mov_reg_from_reg(abi::RAX, abi::RDI));
    out
}

/// Compose [`sysv_callee_load_return_pair`] with a trailing `RET`.
///
/// Convenience shim for tests and future emitter wire-ups that emit the
/// full pre-RET tail as a single sequence. Downstream emitters that
/// need to interleave frame-pointer teardown between the register
/// loads and the RET should use [`sysv_callee_load_return_pair`]
/// directly and emit their own RET.
#[must_use]
pub fn sysv_callee_return_pair_epilogue_with_ret(
    placement: SysvReturnPlacement,
    buf_base: RegId,
    buf_disp: i32,
) -> Vec<Instruction> {
    let mut out = sysv_callee_load_return_pair(placement, buf_base, buf_disp);
    out.push(ret_inst());
    out
}

/// Compose [`sysv_callee_sret_store`] with a trailing `RET`.
///
/// Convenience shim mirroring
/// [`sysv_callee_return_pair_epilogue_with_ret`].
#[must_use]
pub fn sysv_callee_sret_epilogue_with_ret(
    size_bytes: u32,
    src_base: RegId,
    src_disp: i32,
) -> Vec<Instruction> {
    let mut out = sysv_callee_sret_store(size_bytes, src_base, src_disp);
    out.push(ret_inst());
    out
}

#[cfg(test)]
mod tests {
    //! Byte-exact tests for the SysV aggregate-return sequences.
    //!
    //! Each test constructs the emitted sequence via the public helper,
    //! encodes every instruction through
    //! [`paideia_as_encoder::encode_instruction`], concatenates the
    //! resulting bytes, and asserts the exact byte string. This pins
    //! both the mnemonic/operand choices (via the helper) and the
    //! encoder's output (via the byte-string assertion) — a divergence
    //! in either surface fails a specific, named test.
    //!
    //! The expected bytes are derived from the Intel SDM opcode tables:
    //!
    //! - `mov r64, [base + disp8]` — `REX.W 8B /r [SIB] disp8`
    //!   e.g. `mov rax, [rbp+0]` = `48 8B 45 00`
    //! - `mov [base + disp8], r64` — `REX.W 89 /r [SIB] disp8`
    //!   e.g. `mov [rbp+0], rax` = `48 89 45 00`
    //! - `lea r64, [base + disp8]` — `REX.W 8D /r [SIB] disp8`
    //!   e.g. `lea rdi, [rbp-32]` = `48 8D 7D E0`
    //! - `movq xmm, r64` — `66 REX.W 0F 6E /r` (to_xmm=true)
    //! - `movq r64, xmm` — `66 REX.W 0F 7E /r` (to_xmm=false)
    //! - `mov rN, rM` — `REX.W 8B /r ModRM` or `89 /r` (either encoding
    //!   is valid; the encoder picks one form which the tests pin).
    //! - `ret` — `C3`.

    use super::*;
    use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};
    use paideia_as_ir::abi;

    /// Encode every instruction in `seq` and return the concatenated
    /// byte string.
    fn encode_seq(seq: &[Instruction]) -> Vec<u8> {
        let mut buf = CodeBuffer::new();
        let mut stats = EncodeStats::new();
        for inst in seq {
            encode_instruction(inst, &mut buf, &mut stats)
                .expect("aggregate-return helper produced an un-encodable instruction");
        }
        buf.bytes
    }

    // ── caller-side sret prelude ────────────────────────────────────

    /// `lea rdi, [rbp - 32]` — the canonical sret prelude for a caller
    /// with a 32-byte-below-RBP stack buffer. Bytes: `48 8D 7D E0`
    /// (REX.W 8D /r [ModRM=7Dh: mod=01, reg=RDI(7), rm=RBP(5)] disp8=E0).
    #[test]
    fn caller_sret_prelude_lea_rdi_rbp_minus_32_bytes_exact() {
        let seq = sysv_caller_sret_prelude(abi::RBP, -32);
        assert_eq!(seq.len(), 1, "sret prelude must be a single LEA");
        assert_eq!(encode_seq(&seq), vec![0x48, 0x8D, 0x7D, 0xE0]);
    }

    // ── callee-side [Integer, Integer] epilogue ─────────────────────

    /// `[Integer, Integer]` return from `{ u64 lo; u64 hi }`. Sequence:
    /// `mov rax, [rbp+0]; mov rdx, [rbp+8]; ret`.
    /// Bytes: `48 8B 45 00` + `48 8B 55 08` + `C3` = 9 bytes.
    #[test]
    fn callee_int_pair_epilogue_bytes_exact() {
        let seq = sysv_callee_return_pair_epilogue_with_ret(
            SysvReturnPlacement::IntPair,
            abi::RBP,
            0,
        );
        assert_eq!(seq.len(), 3, "IntPair epilogue must be 2 loads + ret");
        assert_eq!(
            encode_seq(&seq),
            vec![0x48, 0x8B, 0x45, 0x00, 0x48, 0x8B, 0x55, 0x08, 0xC3]
        );
    }

    /// Companion caller-side: after CALL, write RAX + RDX into a
    /// `[rbp - 16]` buffer. Bytes: `48 89 45 F0` + `48 89 55 F8` = 8 bytes.
    #[test]
    fn caller_int_pair_readback_bytes_exact() {
        let seq = sysv_caller_read_return_pair(
            SysvReturnPlacement::IntPair,
            abi::RBP,
            -16,
        );
        assert_eq!(seq.len(), 2, "IntPair readback must be 2 stores");
        assert_eq!(
            encode_seq(&seq),
            vec![0x48, 0x89, 0x45, 0xF0, 0x48, 0x89, 0x55, 0xF8]
        );
    }

    // ── callee-side [SSE, SSE] epilogue ─────────────────────────────

    /// `[SSE, SSE]` return from `{ f64 x; f64 y }`. Sequence:
    /// `mov r10, [rbp+0]; movq xmm0, r10; mov r10, [rbp+8]; movq xmm1, r10; ret`.
    ///
    /// Bytes:
    /// - `mov r10, [rbp+0]` = REX(W+R) (4C) 8B /r [ModRM=55 (mod=01 disp8, reg=r10&7=2, rm=rbp=5)] disp8=00
    ///   → `4C 8B 55 00` (RBP-based, so mod=01 disp8=0 per the RBP escape, not mod=00)
    /// - `movq xmm0, r10`   = 66 REX(W+B) (49) 0F 6E /r [ModRM=C2 (mod=11, reg=xmm0=0, rm=r10&7=2)]
    ///   → `66 49 0F 6E C2`
    ///   (REX.W is always set for movq per the encoder; REX.B is set because r10 has bit 3.)
    /// - `mov r10, [rbp+8]` = `4C 8B 55 08`
    /// - `movq xmm1, r10`   = `66 49 0F 6E CA`
    ///   (ModRM=C0 | (xmm1=1 << 3) | (r10&7=2) = 0xCA.)
    /// - `ret`              = `C3`
    /// Total: 4 + 5 + 4 + 5 + 1 = 19 bytes.
    #[test]
    fn callee_sse_pair_epilogue_bytes_exact() {
        let seq = sysv_callee_return_pair_epilogue_with_ret(
            SysvReturnPlacement::SsePair,
            abi::RBP,
            0,
        );
        assert_eq!(seq.len(), 5, "SsePair epilogue = 2×(load+movq) + ret");
        assert_eq!(
            encode_seq(&seq),
            vec![
                0x4C, 0x8B, 0x55, 0x00, // mov r10, [rbp+0]
                0x66, 0x49, 0x0F, 0x6E, 0xC2, // movq xmm0, r10
                0x4C, 0x8B, 0x55, 0x08, // mov r10, [rbp+8]
                0x66, 0x49, 0x0F, 0x6E, 0xCA, // movq xmm1, r10
                0xC3, // ret
            ]
        );
    }

    /// Companion caller-side: after CALL, write XMM0 + XMM1 into a
    /// `[rbp - 16]` buffer via the SSE_SCRATCH_GPR round-trip.
    /// Bytes:
    /// - `movq r10, xmm0` = 66 REX(W+B) (49) 0F 7E /r [ModRM=C2 (mod=11, reg=xmm0=0, rm=r10&7=2)]
    ///   → `66 49 0F 7E C2` (REX.W always set for movq; REX.B for r10.)
    /// - `mov [rbp-16], r10` = REX(W+R) (4C) 89 /r [ModRM=55 (mod=01 disp8, reg=r10&7=2, rm=rbp=5)] disp8=F0
    ///   → `4C 89 55 F0`
    /// - `movq r10, xmm1` = `66 49 0F 7E CA` (ModRM=C0 | (xmm1=1<<3) | 2 = 0xCA.)
    /// - `mov [rbp-8], r10` = `4C 89 55 F8`
    /// Total: 5 + 4 + 5 + 4 = 18 bytes.
    #[test]
    fn caller_sse_pair_readback_bytes_exact() {
        let seq = sysv_caller_read_return_pair(
            SysvReturnPlacement::SsePair,
            abi::RBP,
            -16,
        );
        assert_eq!(seq.len(), 4, "SsePair readback = 2×(movq+store)");
        assert_eq!(
            encode_seq(&seq),
            vec![
                0x66, 0x49, 0x0F, 0x7E, 0xC2, // movq r10, xmm0
                0x4C, 0x89, 0x55, 0xF0, // mov [rbp-16], r10
                0x66, 0x49, 0x0F, 0x7E, 0xCA, // movq r10, xmm1
                0x4C, 0x89, 0x55, 0xF8, // mov [rbp-8], r10
            ]
        );
    }

    // ── callee-side Memory-sret epilogue ────────────────────────────

    /// A 24-byte Memory-classified aggregate (three u64) copied from
    /// `[rbp - 24]` into `[rdi]`, then `mov rax, rdi; ret`. Sequence:
    /// `mov r10, [rbp-24]; mov [rdi+0], r10;`
    /// `mov r10, [rbp-16]; mov [rdi+8], r10;`
    /// `mov r10, [rbp-8];  mov [rdi+16], r10;`
    /// `mov rax, rdi; ret`.
    ///
    /// Bytes:
    /// - `mov r10, [rbp-24]` = REX(W+R)=4C, 8B, mod=01 disp8=E8, ModRM=55
    ///   → `4C 8B 55 E8`
    /// - `mov [rdi+0], r10`  = REX(W+R)=4C, 89, mod=00 no-disp (base=RDI is not
    ///   the RBP/R13 escape), ModRM=17: reg=r10&7=2, rm=RDI(7)
    ///   → `4C 89 17` (3 bytes — no displacement byte because disp=0 and
    ///     base is not RBP/R13; see `emit_mem_base_disp` in the encoder).
    /// - `mov r10, [rbp-16]` = `4C 8B 55 F0`
    /// - `mov [rdi+8], r10`  = REX(W+R)=4C, 89, mod=01 disp8=08, ModRM=57
    ///   → `4C 89 57 08`
    /// - `mov r10, [rbp-8]`  = `4C 8B 55 F8`
    /// - `mov [rdi+16], r10` = `4C 89 57 10`
    /// - `mov rax, rdi`      = REX.W=48, 89, ModRM=F8 (mod=11, reg=RDI(7), rm=RAX(0))
    ///   → `48 89 F8`
    /// - `ret`               = `C3`
    /// Total: 4 + 3 + 4 + 4 + 4 + 4 + 3 + 1 = 27 bytes.
    #[test]
    fn callee_sret_24byte_epilogue_bytes_exact() {
        let seq = sysv_callee_sret_epilogue_with_ret(24, abi::RBP, -24);
        // 3 (loads + stores) pairs + mov rax,rdi + ret = 3*2 + 1 + 1 = 8 insts
        assert_eq!(seq.len(), 8, "24-byte sret = 3 qword copies + rax-load + ret");
        assert_eq!(
            encode_seq(&seq),
            vec![
                0x4C, 0x8B, 0x55, 0xE8, // mov r10, [rbp-24]
                0x4C, 0x89, 0x17,       // mov [rdi+0], r10   (disp elides — base is RDI, not RBP/R13)
                0x4C, 0x8B, 0x55, 0xF0, // mov r10, [rbp-16]
                0x4C, 0x89, 0x57, 0x08, // mov [rdi+8], r10
                0x4C, 0x8B, 0x55, 0xF8, // mov r10, [rbp-8]
                0x4C, 0x89, 0x57, 0x10, // mov [rdi+16], r10
                0x48, 0x89, 0xF8,       // mov rax, rdi
                0xC3,                   // ret
            ]
        );
    }

    // ── panic guards ────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "zero-size aggregate cannot be Memory-classified")]
    fn sret_store_zero_size_panics() {
        let _ = sysv_callee_sret_store(0, abi::RBP, 0);
    }

    #[test]
    #[should_panic(expected = "non-multiple-of-8 aggregate size 17")]
    fn sret_store_non_multiple_of_8_panics() {
        let _ = sysv_callee_sret_store(17, abi::RBP, 0);
    }

    // ── placement inertness for None / Memory ───────────────────────

    /// `None` placement → both callee-load and caller-read produce
    /// empty sequences. Pins that a void-return callee/caller path is
    /// a no-op at the aggregate-return layer (the frame-pointer
    /// epilogue and CALL emission remain the responsibility of their
    /// existing owners).
    #[test]
    fn none_placement_produces_empty_sequences() {
        assert!(sysv_callee_load_return_pair(SysvReturnPlacement::None, abi::RBP, 0).is_empty());
        assert!(sysv_caller_read_return_pair(SysvReturnPlacement::None, abi::RBP, 0).is_empty());
    }

    /// `Memory` placement at the register-pair helpers → empty
    /// sequences. The Memory return-value is handled by the sret
    /// helpers ([`sysv_caller_sret_prelude`] on the caller side and
    /// [`sysv_callee_sret_store`] on the callee side), not by the
    /// return-pair helpers.
    #[test]
    fn memory_placement_bypasses_register_pair_helpers() {
        assert!(sysv_callee_load_return_pair(SysvReturnPlacement::Memory, abi::RBP, 0).is_empty());
        assert!(sysv_caller_read_return_pair(SysvReturnPlacement::Memory, abi::RBP, 0).is_empty());
    }

    // ── coverage for the four remaining placements ──────────────────

    /// `IntSingle` — canonical scalar-integer return (existing single-
    /// register RAX path). Pins that we don't accidentally break
    /// byte-identity for the trivial `u64` return shape.
    #[test]
    fn callee_int_single_epilogue_bytes_exact() {
        let seq = sysv_callee_return_pair_epilogue_with_ret(
            SysvReturnPlacement::IntSingle,
            abi::RBP,
            -8,
        );
        // mov rax, [rbp-8] + ret
        assert_eq!(seq.len(), 2);
        assert_eq!(encode_seq(&seq), vec![0x48, 0x8B, 0x45, 0xF8, 0xC3]);
    }

    /// `SseSingle` — canonical scalar-float return (existing XMM0 path,
    /// re-derived via the aggregate placement selector for `[SSE]`).
    #[test]
    fn callee_sse_single_epilogue_bytes_exact() {
        let seq = sysv_callee_return_pair_epilogue_with_ret(
            SysvReturnPlacement::SseSingle,
            abi::RBP,
            -8,
        );
        // mov r10, [rbp-8]; movq xmm0, r10; ret
        assert_eq!(seq.len(), 3);
        assert_eq!(
            encode_seq(&seq),
            vec![0x4C, 0x8B, 0x55, 0xF8, 0x66, 0x49, 0x0F, 0x6E, 0xC2, 0xC3]
        );
    }

    /// `IntSse` — mixed low-INT/high-SSE. Load RAX from low, then
    /// XMM0-from-mem shim for the high eightbyte. Ret.
    #[test]
    fn callee_int_sse_epilogue_shape() {
        let seq = sysv_callee_return_pair_epilogue_with_ret(
            SysvReturnPlacement::IntSse,
            abi::RBP,
            -16,
        );
        // mov rax, [rbp-16]; mov r10, [rbp-8]; movq xmm0, r10; ret
        assert_eq!(seq.len(), 4);
        // Sanity check: last instruction is RET.
        assert!(matches!(seq.last().unwrap().mnemonic, Mnemonic::Ret));
        // First instruction targets RAX (low INTEGER eightbyte).
        match seq[0].operands.as_slice() {
            [Operand::Reg(r), Operand::MemSib { base, disp, .. }] => {
                assert_eq!(*r, abi::RAX);
                assert_eq!(*base, abi::RBP);
                assert_eq!(*disp, -16);
            }
            other => panic!("IntSse first instruction has unexpected operands: {:?}", other),
        }
    }

    /// `SseInt` — mixed low-SSE/high-INT. XMM0-from-mem shim for the
    /// low eightbyte, then load RAX from the high eightbyte. Ret.
    #[test]
    fn callee_sse_int_epilogue_shape() {
        let seq = sysv_callee_return_pair_epilogue_with_ret(
            SysvReturnPlacement::SseInt,
            abi::RBP,
            -16,
        );
        // mov r10, [rbp-16]; movq xmm0, r10; mov rax, [rbp-8]; ret
        assert_eq!(seq.len(), 4);
        assert!(matches!(seq.last().unwrap().mnemonic, Mnemonic::Ret));
        // The 3rd instruction (index 2) is the RAX load from the high
        // eightbyte.
        match seq[2].operands.as_slice() {
            [Operand::Reg(r), Operand::MemSib { base, disp, .. }] => {
                assert_eq!(*r, abi::RAX);
                assert_eq!(*base, abi::RBP);
                assert_eq!(*disp, -8);
            }
            other => panic!("SseInt high-eightbyte load has unexpected operands: {:?}", other),
        }
    }

    // ── end-to-end classifier → placement → sequence chain ─────────

    /// End-to-end: classify a `{ u64, u64 }` RecordLayout, resolve its
    /// placement, and materialise the callee epilogue. Pins that the
    /// three-layer pipeline (classifier + placement + emitter) composes
    /// without an intermediate hand-massage.
    #[test]
    fn classifier_to_placement_to_epilogue_pair_of_u64() {
        use paideia_as_ir::record_layout::{FieldLayout, RecordLayout};

        let layout = RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
            ],
        );
        let classes = abi::classify_sysv_aggregate(&layout);
        let placement = abi::sysv_return_placement(&classes);
        assert_eq!(placement, SysvReturnPlacement::IntPair);

        let seq = sysv_callee_return_pair_epilogue_with_ret(placement, abi::RBP, 0);
        // Full byte-string identical to the direct-placement test above.
        assert_eq!(
            encode_seq(&seq),
            vec![0x48, 0x8B, 0x45, 0x00, 0x48, 0x8B, 0x55, 0x08, 0xC3]
        );
    }
}
