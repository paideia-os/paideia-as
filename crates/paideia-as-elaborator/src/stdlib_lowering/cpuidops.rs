//! `CpuidOps` stdlib-lowering recipes.
//!
//! v0.21-007 (issue #1283): typed leaf-record intrinsics over the
//! zero-arity CPUID mnemonic.
//!
//! # Background
//!
//! CPUID (SDM Vol 2A §3.2 CPUID) takes an implicit leaf in EAX and
//! subleaf in ECX and clobbers all four of EAX/EBX/ECX/EDX with the
//! result. A full leaf record is 16 bytes (4 × u32), which the SysV
//! ABI would return via RAX:RDX split. The current recipe framework
//! (see `stdlib_lowering/mod.rs`) supports scalar u64 return in RAX
//! only — full record-return marshalling is tracked separately in
//! issue #1298 and needs its own softarch pass.
//!
//! To land a useful CPUID intrinsic today without waiting on the
//! record-return work, this module exposes two SysVRegs recipes that
//! together cover every register a caller might want:
//!
//!   cpuid_leaf_ad(leaf, subleaf) -> u64   // (EDX << 32) | EAX
//!   cpuid_leaf_bc(leaf, subleaf) -> u64   // (ECX << 32) | EBX
//!
//! The two-call idiom pays for a second CPUID execution when the
//! caller wants all four registers. The alternative (a single
//! stack-descriptor recipe where the caller passes a pointer to a
//! 16-byte record slot) would prejudge the record-return convention
//! that #1298 must design; keeping the primitives as pure scalar
//! returns lets #1298 add a `cpuid_leaf(leaf, subleaf) -> LeafRecord`
//! wrapper on top without breaking anything landed here.
//!
//! # PAS-DEBT-B4-002 retirement status (v0.36.64, paideia-as#1524)
//!
//! **SUPPLY side landed.** Byte-exact aggregate-return sequences for
//! both x86-64 ABIs are available at:
//!   * `crates/paideia-as-elaborator/src/aggregate_return.rs`
//!     — `sysv_caller_sret_prelude` / `sysv_caller_read_return_pair`
//!       (caller-side; SysV, paideia-as#1544, v0.36.62)
//!     — `sysv_callee_load_return_pair` / `sysv_callee_sret_store`
//!       (callee-side; SysV, paideia-as#1544, v0.36.62)
//!     — `ms_caller_sret_prelude` / `ms_caller_read_return_reg`
//!       (caller-side; MS x64, paideia-as#1543, v0.36.63)
//!     — `ms_callee_load_return_reg` / `ms_callee_sret_store`
//!       (callee-side; MS x64, paideia-as#1543, v0.36.63)
//! For `CpuidRegs { eax, ebx, ecx, edx }` (4 × u32 = 16 B, all INTEGER)
//! SysV classifies as `IntPair` (RAX + RDX) and MS as `Memory` (>8 B → sret via RCX).
//!
//! **DEMAND side still blocked.** Wiring these helpers at the
//! `cpuid_leaf` call site requires machinery that has not landed:
//!   1. `CpuidRegs {eax,ebx,ecx,edx}` record type not declared in
//!      `crates/paideia-as-stdlib/pdx/cpuid.pdx`. Also unparseable at
//!      `fn` return position (no AST syntax for record return types).
//!   2. `Symbol` has no return-record-layout side-table — the elaborator
//!      cannot look up whether a symbol's return type is aggregate.
//!   3. `emit_call.rs` has no branch that inspects the return-type
//!      layout, allocates a caller-owned buffer slot, splices
//!      `sysv_caller_sret_prelude` (or MS mirror) before CALL, and
//!      shifts SysV/MS argument registers right by one on `Memory`
//!      placement (so leaf → RSI/RDX and subleaf → RDX/R8 instead
//!      of RDI/RCX).
//!   4. `emit_ret` in `emit_walker/emit_core.rs` has no branch that
//!      reads the return-type side-table and splices
//!      `sysv_callee_sret_store` / `ms_callee_sret_store` (or the
//!      pair-load variants for the SysV register-return path) before
//!      the frame-pointer epilogue + RET.
//!   5. `ArgConvention` has no `SysVSret` / `MsSret` variant to signal
//!      "arg[0] is an sret hidden pointer; real args shift right by one".
//!      The current `SysVRegs` convention hard-wires arg[0] to RDI,
//!      which conflicts with using RDI as the sret pointer.
//!   6. For the SysV register-return alternative (Option B of
//!      `design/stdlib/cpuid-record-return.md` §2 — return in RAX+RDX
//!      per §2.2 tuple-in-registers rule), the record subsystem has no
//!      lowering that consumes a register-pair return and materialises
//!      it as a field-addressable temporary. Without that path, `r.ebx`
//!      / `r.ecx` / `r.edx` accesses have nowhere to lower from.
//!
//! Follow-up ticket sketch: **PAS-DEBT-B3-007c-followup — record-return
//! type plumbing + call-site wiring for SysV/MS aggregate returns.**
//! Blocks B4-002 and B4-003 (mldsa65_sign sret, paideia-as debt-catalog
//! §5.2). Details in
//! `.plans/scratch/CHANGELOG-1544-b3007c-sysv-return.md` §"End-to-end
//! fixtures — deferred" and this ticket's
//! `.plans/scratch/CHANGELOG-1524-b4002-cpuid-sret.md`.
//!
//! Typed per-leaf decoders (0x01 basic feature bits, 0x0B / 0x1F
//! topology, 0x0D XSAVE, 0x1A hybrid) live in
//! `crates/paideia-as-stdlib/pdx/cpuid.pdx` as pdx-level functions
//! composed on top of these two primitives — the elaborator sees
//! them as ordinary calls into a stdlib module.
//!
//! # Register discipline
//!
//! SysV places leaf in RDI, subleaf in RSI. CPUID clobbers EAX, EBX,
//! ECX and EDX and (in 64-bit mode) zero-extends each of RAX/RBX/RCX/
//! RDX (SDM Vol 1 §3.4.1.1 "General-Purpose Registers in 64-Bit
//! Mode"), so no explicit masking is required before the shift-and-or
//! pack.
//!
//! RBX is *callee-saved* in SysV, and CPUID's writing of EBX is what
//! makes this recipe non-obvious: the recipe splices in place of the
//! CALL+RET in the caller's function body (see the SysVRegs branch in
//! `emit_call.rs`), so a live-across-recipe RBX binding in the caller
//! would be silently trashed. Both recipes therefore bracket the
//! CPUID (and, in cpuid_leaf_bc, the read of EBX) with a push/pop of
//! RBX. RSP alignment is not disturbed by the balanced push+pop pair,
//! and CPUID has no alignment requirement of its own.
//!
//! RCX and RDX are caller-saved, so their post-CPUID content in RCX/
//! RDX is free for the recipe to consume without further preservation
//! — the caller has no expectation on them across the call boundary.
//!
//! # Effect + capability discipline
//!
//! Both intrinsics are `!{sysreg}` (CPUID reads architectural state,
//! not memory) and gated behind `@{paideia.sysreg}` in
//! `stdlib/pdx/cpuid.pdx` — matching the effect row on RDMSR/WRMSR
//! and the other privileged system-register primitives. CPUID itself
//! is not ring-restricted (any CPL may execute it), but the typed
//! wrapper is scoped to kernel-context callers because the primary
//! consumers (topology walk, XSAVE sizing, hybrid tagging) all live
//! in early boot / arch/x86_64.

#![allow(unused_imports)]

use paideia_as_ir::{
    IrArena, IrNodeId, SmallVec, abi,
    instruction::{InstrMode, Instruction, IntWidth, Mnemonic, Operand, SegPrefix},
};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

/// Build a `push rbx` instruction — save the callee-saved RBX before
/// CPUID clobbers it.
fn push_rbx(mode: InstrMode) -> Instruction {
    let mut ops = SmallVec::new();
    ops.push(Operand::Reg(abi::RBX));
    Instruction {
        mnemonic: Mnemonic::Push,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode,
        emission_order: 0,
    }
}

/// Build a `pop rbx` instruction — restore the callee-saved RBX
/// after any use of the CPUID-written EBX has completed.
fn pop_rbx(mode: InstrMode) -> Instruction {
    let mut ops = SmallVec::new();
    ops.push(Operand::Reg(abi::RBX));
    Instruction {
        mnemonic: Mnemonic::Pop,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode,
        emission_order: 0,
    }
}

/// Dispatch a `CpuidOps::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission and, ultimately, a T0553 unresolved-identifier
/// diagnostic if the callee is not otherwise a real symbol).
pub(super) fn try_lower(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    let _ = (arg_ids, arena);
    match method_name {
        // cpuid_leaf_ad(leaf: u32, subleaf: u32) -> u64
        //   leaf arrives in RDI (upper 32 zero-extended per SysV for u32).
        //   subleaf arrives in RSI.
        //
        //   push rbx         ; preserve callee-saved RBX (CPUID clobbers EBX).
        //   mov rax, rdi     ; leaf → RAX (EAX)
        //   mov rcx, rsi     ; subleaf → RCX (ECX)
        //   cpuid            ; EAX/EBX/ECX/EDX ← CPUID(leaf, subleaf).
        //                     ; hardware zero-extends the R- halves in 64-bit mode.
        //   pop rbx          ; restore callee-saved RBX before any downstream
        //                     ; caller code observes it clobbered.
        //   shl rdx, 32      ; RDX = EDX_result << 32
        //   or  rax, rdx     ; RAX = (EDX << 32) | EAX  → SysV return.
        "cpuid_leaf_ad" => {
            let mut mov_rax_rdi = SmallVec::new();
            mov_rax_rdi.push(Operand::Reg(abi::RAX));
            mov_rax_rdi.push(Operand::Reg(abi::RDI));

            let mut mov_rcx_rsi = SmallVec::new();
            mov_rcx_rsi.push(Operand::Reg(abi::RCX));
            mov_rcx_rsi.push(Operand::Reg(abi::RSI));

            let mut shl_rdx = SmallVec::new();
            shl_rdx.push(Operand::Reg(abi::RDX));
            shl_rdx.push(Operand::Imm64(32));

            let mut or_rax_rdx = SmallVec::new();
            or_rax_rdx.push(Operand::Reg(abi::RAX));
            or_rax_rdx.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    push_rbx(mode),
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rax_rdi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rcx_rsi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Cpuid,
                        operands: SmallVec::new(),
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    pop_rbx(mode),
                    Instruction {
                        mnemonic: Mnemonic::Shl,
                        operands: shl_rdx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Or,
                        operands: or_rax_rdx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
            }))
        }
        // cpuid_leaf_bc(leaf: u32, subleaf: u32) -> u64
        //   leaf arrives in RDI, subleaf in RSI.
        //
        //   push rbx         ; preserve callee-saved RBX.
        //   mov rax, rdi     ; leaf → EAX
        //   mov rcx, rsi     ; subleaf → ECX
        //   cpuid            ; clobbers EAX/EBX/ECX/EDX.
        //   mov rax, rbx     ; RAX = EBX_result (upper zeroed by CPUID).
        //                     ; MUST happen before the pop restores RBX.
        //   pop rbx          ; restore callee-saved RBX.
        //   shl rcx, 32      ; RCX = ECX_result << 32
        //   or  rax, rcx     ; RAX = (ECX << 32) | EBX  → SysV return.
        //
        // Note: this recipe reissues CPUID identically to cpuid_leaf_ad
        // when a caller wants all four registers. That doubles the
        // instruction cost but keeps the intrinsic surface a pair of
        // pure scalar-return functions — the record-return recipe that
        // would consolidate them is tracked in #1298.
        "cpuid_leaf_bc" => {
            let mut mov_rax_rdi = SmallVec::new();
            mov_rax_rdi.push(Operand::Reg(abi::RAX));
            mov_rax_rdi.push(Operand::Reg(abi::RDI));

            let mut mov_rcx_rsi = SmallVec::new();
            mov_rcx_rsi.push(Operand::Reg(abi::RCX));
            mov_rcx_rsi.push(Operand::Reg(abi::RSI));

            let mut mov_rax_rbx = SmallVec::new();
            mov_rax_rbx.push(Operand::Reg(abi::RAX));
            mov_rax_rbx.push(Operand::Reg(abi::RBX));

            let mut shl_rcx = SmallVec::new();
            shl_rcx.push(Operand::Reg(abi::RCX));
            shl_rcx.push(Operand::Imm64(32));

            let mut or_rax_rcx = SmallVec::new();
            or_rax_rcx.push(Operand::Reg(abi::RAX));
            or_rax_rcx.push(Operand::Reg(abi::RCX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    push_rbx(mode),
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rax_rdi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rcx_rsi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Cpuid,
                        operands: SmallVec::new(),
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rax_rbx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    pop_rbx(mode),
                    Instruction {
                        mnemonic: Mnemonic::Shl,
                        operands: shl_rcx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Or,
                        operands: or_rax_rcx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
            }))
        }
        _ => None,
    }
}
