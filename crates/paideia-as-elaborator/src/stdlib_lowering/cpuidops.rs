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
//! # PAS-DEBT-B4-002 retirement status (v0.36.75, paideia-as#1524, Wave 46)
//!
//! **Blockers 1-5 CLEARED** by #1554 Slices A-D (v0.36.68..v0.36.71).
//! **Blocker 6 partially cleared** (register-return pair-unpack gate
//! lifted in Slice D via a caller-side persistent frame slot; RAX+RDX
//! spills into `[RBP + disp]` after CALL, from which field-addressed
//! loads read individual eightbytes).
//!
//! **Retirement STILL DEFERRED** by two mismatches between the newly-
//! landed Slice A-D machinery and the specific composition shape
//! `cpuid_leaf` would need. Neither is a "small enough for this wave"
//! extension; both belong in a dedicated follow-up.
//!
//! ## Where Slice A-D lands cleanly
//!
//! Slice A-D wire the record-return path for user-code Lambda callees
//! whose body is `IrKind::RecordCons` and whose declared return type
//! is a record (inline `record { … }` or a named struct in the
//! `StructRegistry`). For such a callee:
//!   * Slice A: `populate_return_record_layouts` fills
//!     `IrArena::return_record_layout_table`, stamped onto the callee
//!     `Symbol::return_record_layout` at walker construction.
//!   * Slice B: `emit_call.rs` probes the callee Symbol's layout and,
//!     on `Memory` placement, splices the sret prelude / arg-shift /
//!     slot release around the CALL (byte-identical scalar-path on
//!     register-return placements).
//!   * Slice C: caller-side persistent frame slot in
//!     `caller_sret_slot_table`, sized off the callee layout;
//!     `caller_sret_frame_bump_table` sums per-caller-Lambda and
//!     drives a single `sub rsp, total` in the prologue.
//!   * Slice D: `emit_callee_sret_splice` emits `sub rsp, padded` +
//!     per-field `mov [rsp+off], value` (populating from the Lambda's
//!     RecordCons body) + the byte-exact
//!     `sysv_callee_sret_store` / `sysv_callee_load_return_pair`
//!     helper stream.
//!
//! ## Why `cpuid_leaf` does not fit that shape
//!
//! **Gap A — Slice D's RecordCons body arm supports only `Literal`
//! and `Var` (parameter-forwarding) field values.** See
//! `emit_walker/emit_core.rs::emit_record_cons_field_stores_into_sret_buffer`
//! (v0.36.71): any child that is not `IrKind::Literal` or `IrKind::Var`
//! is silently skipped, leaving the slot uninitialised until a future
//! T0522 diagnostic replaces the skip. A record-returning `cpuid_leaf`
//! written as a .pdx wrapper over `cpuid_leaf_ad`/`cpuid_leaf_bc`
//! would have field values shaped as call-result + shift + mask + cast
//! (`(cpuid_leaf_ad(l, s) & 0xFFFFFFFF) as u32`) — an `App` subtree,
//! not a `Literal` or `Var`. Slice D would emit the sret store over an
//! uninitialised buffer.
//!
//! **Gap B — a raw-assembly body (`unsafe { block: { cpuid; mov
//! [rdi+…], … } }`) is clobbered by Slice C's unconditional
//! `emit_callee_sret_splice`.** The splice fires in `emit_ret` for
//! every RET whose enclosing Lambda has `return_record_layout`
//! populated and is not `@no_frame`. For a hand-written body whose
//! block itself writes the return slots, Slice C would append a
//! duplicate `sub rsp, padded` + copy-from-uninitialised-source-buffer
//! + `mov rax, rdi` sequence, garbling the hand-written stores. The
//! `@no_frame` opt-out is the only current escape hatch and interacts
//! poorly with CPUID's callee-saved RBX bracket (push/pop around
//! CPUID is trivially compatible with `@no_frame`, but discipline in
//! the surrounding stdlib is that `@no_frame` is reserved for pure-
//! prologueless leaf helpers — using it here as a "skip the Slice C
//! splice" flag is a semantic overload).
//!
//! **Gap C — stdlib-lowering recipes do not participate in
//! `Symbol::return_record_layout`.** The current `stdlib_lowering`
//! pipeline intercepts calls by `(trait_name, method_name)` pair and
//! substitutes an instruction sequence. Trait-method resolution does
//! not run `populate_return_record_layouts` for the substituted
//! callee, so any sret-shaped recipe would need either (a) a parallel
//! recipe-side layout table + emit_call probe, or (b) trait-method
//! plumbing that produces a Symbol carrying the trait method's return
//! type. Either is substantive elaborator surgery, not a two-line
//! change.
//!
//! ## Path B minimum-scope estimate (not landing here)
//!
//! A minimum-scope Path B (`ArgConvention::SysVSret { layout }`
//! variant + emit_call.rs arg-shift wiring for recipe-driven callees
//! + per-recipe suppression of `emit_callee_sret_splice`) requires:
//!   * A new `ArgConvention` variant (opt-in per recipe).
//!   * A recipe-side layout side-table keyed by trait method (Gap C).
//!   * A callee-side suppression flag so recipes that emit register
//!     packing manually skip the Slice C splice (Gap B in a new form).
//!   * Composition tests against the existing 40+ `SysVRegs` recipes
//!     to prove none of them changed byte-shape.
//! Estimated at one Wave dedicated to Path B; not compatible with
//! landing alongside the Slice A-D reference machinery that just
//! shipped.
//!
//! ## Deferral
//!
//! Retirement remains blocked pending a dedicated follow-up: **PAS-
//! DEBT-B4-002-followup — stdlib-recipe participation in
//! return_record_layout side-table + per-recipe Slice C splice
//! opt-out (Gaps B + C)**, OR **PAS-DEBT-B4-002-alt — extend Slice D's
//! RecordCons body arm to handle App / arithmetic field values
//! (Gap A)**. The two paths are alternatives; either one alone
//! unblocks the record-returning `cpuid_leaf`.
//!
//! Cross-refs: `.plans/scratch/CHANGELOG-1524-b4002-cpuid-sret.md`
//! (the original Wave 35 gap enumeration; blockers 1-5 there are now
//! resolved) and `.plans/scratch/CHANGELOG-1524-b4002-cpuid-retirement-
//! attempt.md` (this Wave 46 re-attempt: Gap A/B/C analysis + Path A
//! infeasibility proof).
//!
//! ## B4-003 (mldsa65_sign) status — NOT similarly retirable
//!
//! B4-003 targets `MlDsa65::sign` (see `mldsaops.rs`). That recipe
//! already uses **Choice A**: caller-allocated `MLDSA65_SIG_BYTES`
//! output buffer passed in RCX + `i64` status return in RAX. It is
//! not a record-return workaround at all — it is the intended long-
//! term extern-C ABI for a 3309-byte signature (matches every other
//! crypto FFI thunk: `argon2id_derive`, `chacha20_poly1305_seal` /
//! `open`, `ml_kem_768_*`, `mldsa65_verify`). Retiring B4-003 does
//! not depend on the record-return machinery landed in #1554 and does
//! not benefit from either Path A or Path B above.
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
                return_record_layout: None,
                skip_sret_splice: false,
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
                return_record_layout: None,
                skip_sret_splice: false,
            }))
        }
        _ => None,
    }
}
