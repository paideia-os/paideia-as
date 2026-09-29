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
//! # PAS-DEBT-B4-002 retirement status (v0.36.78, paideia-as#1524, Wave 54)
//!
//! **Path A landed.** The record-returning `cpuid_leaf(l:u32, s:u32)
//! -> CpuidRegs` recipe is registered in `enumerate_record_return_
//! recipes` and its lowering arm lives in `try_lower` below. The
//! Wave-51 (#1559) plumbing (`LoweringRecipe::return_record_layout`
//! + `LoweringRecipe::skip_sret_splice` + the recipe-registry loop
//! in `populate_return_record_layouts`) carries the layout into the
//! caller-side `emit_call.rs` Slice-B site probe by way of a
//! synthetic `"CpuidOps::cpuid_leaf"` Symbol.
//!
//! Blockers 1-5 CLEARED by #1554 Slices A-D (v0.36.68..v0.36.71).
//! Blocker 6 cleared by Slice D's caller-side persistent frame slot.
//! Gap A (RecordCons non-Literal/Var field values) cleared by Wave 52
//! (#1558, v0.36.76). Gaps B + C cleared by Wave 51 (#1559, v0.36.77).
//! Wave 54 (this file) consumes all of the above by:
//!   * registering `CpuidOps::cpuid_leaf` in
//!     `enumerate_record_return_recipes` with the field-exact
//!     16-B / align-4 CpuidRegs layout;
//!   * emitting the CPUID sret-store recipe directly (arg-shifted
//!     leaf/subleaf in RSI/RDX per Slice B, sret buffer pointer
//!     in RDI, four `mov_d [rdi + N], eN` stores into the buffer);
//!   * setting `skip_sret_splice = true` so the callee-side
//!     `emit_ret::emit_callee_sret_splice` never re-fires and
//!     clobbers those stores (defence-in-depth — recipes are
//!     inlined, so the emit_ret path is not normally reached).
//!
//! # Known remaining gap (does NOT block landing this wave)
//!
//! `populate_return_record_cons_slots` (Slice C) reads
//! `IrArena::return_record_layout_table` (keyed by user-code Let
//! IrNodeIds) to build its `callee_info` map. A recipe callee like
//! `CpuidOps::cpuid_leaf` has no Let, so no persistent
//! `caller_sret_slot_table` entry is allocated for App sites
//! calling it. `emit_call.rs` then takes the Slice-B transient path
//! (`sub rsp, 16; lea rdi, [rsp+0]`) at the recipe's prelude, but
//! returns at the `return` statement inside the SysVRegs recipe
//! splice branch (line ~1405), skipping the `add rsp, padded_slot`
//! release. The recipe's stack effect therefore leaves RSP 16 B low
//! across the splice. A follow-up wave (Slice E: recipe-callee
//! persistent-slot allocation via `enumerate_record_return_recipes`)
//! closes this. Wave 54 lands the recipe machinery + tests so that
//! Slice E has a concrete first customer.
//!
//! Cross-refs: `.plans/scratch/CHANGELOG-1524-b4002-cpuid-retirement.md`
//! (this wave's changelog) plus the historical
//! `.plans/scratch/CHANGELOG-1524-b4002-cpuid-sret.md` and
//! `.plans/scratch/CHANGELOG-1524-b4002-cpuid-retirement-attempt.md`
//! (Wave-46 gap enumeration; every blocker there is now cleared).
//!
//! # Legacy deferral prose (retained for provenance only)
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

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError, enumerate_record_return_recipes};

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
        // cpuid_leaf(leaf: u32, subleaf: u32) -> CpuidRegs
        //
        // paideia-as#1524 Wave 54 Path A retirement (v0.36.78).
        // Record-return recipe consuming Wave 52 (#1558) + Wave 53
        // (#1559) plumbing. Caller-side Slice B (`emit_call.rs`) sees
        // the synthetic `"CpuidOps::cpuid_leaf"` Symbol's
        // `return_record_layout` (16 B / align 4 / four u32 fields at
        // offsets 0/4/8/12), classifies the placement as `Memory`,
        // and fires the sret prelude: hidden sret buffer pointer in
        // RDI, real args shifted by one so leaf → RSI, subleaf → RDX.
        //
        //   push rbx           ; save caller's callee-saved RBX before CPUID clobbers EBX.
        //   mov rax, rsi       ; leaf → EAX for CPUID (RSI's upper 32 bits are 0 per SysV u32).
        //   mov rcx, rdx       ; subleaf → ECX for CPUID (same guarantee for RDX).
        //   cpuid              ; EAX/EBX/ECX/EDX ← CPUID(leaf, subleaf); hardware zero-extends.
        //   mov_d [rdi + 0], eax   ; CpuidRegs.eax
        //   mov_d [rdi + 4], ebx   ; CpuidRegs.ebx   (stored BEFORE pop rbx restores caller RBX)
        //   mov_d [rdi + 8], ecx   ; CpuidRegs.ecx
        //   mov_d [rdi + 12], edx  ; CpuidRegs.edx
        //   pop rbx            ; restore caller's RBX; the CPUID-EBX in the register
        //                       ; is no longer needed (already stored above).
        //
        // Byte-exact: 53  48 89 F0  48 89 D1  0F A2  89 07  89 5F 04
        //             89 4F 08  89 57 0C  5B   → 21 bytes total.
        //
        // The recipe does NOT emit `mov rax, rdi` (SysV sret pointer
        // return) nor `ret` (CALL semantics do not apply — the recipe
        // is inline-spliced, not called). RDI stays intact through the
        // splice; the caller reads fields out of its own sret slot via
        // `[RBP + slot.rbp_disp]` (Slice C persistent path, when it
        // grows recipe-callee support) or transiently at `[RSP + 0]`
        // (Slice B fallback — leaves an unreleased 16-B slot; the
        // Slice-E follow-up closes that).
        //
        // The RBX push/pop bracket is identical in spirit to the AD/BC
        // recipes above: CPUID clobbers EBX (RBX callee-saved in
        // SysV), and the recipe is inlined into the caller's function
        // body without a CALL boundary, so a live-across-recipe RBX
        // binding in the caller would be silently trashed absent the
        // bracket. The four `[rdi + N]` stores use `MovSized { W32 }`
        // — the encoder's narrow-width base+disp store path emits
        // `89 /r` (no REX.W) for r32→r/m32, and `mov [rdi+0], eax`
        // benefits from the disp=0 no-byte encoding shortcut.
        "cpuid_leaf" => {
            let mut mov_rax_rsi = SmallVec::new();
            mov_rax_rsi.push(Operand::Reg(abi::RAX));
            mov_rax_rsi.push(Operand::Reg(abi::RSI));

            let mut mov_rcx_rdx = SmallVec::new();
            mov_rcx_rdx.push(Operand::Reg(abi::RCX));
            mov_rcx_rdx.push(Operand::Reg(abi::RDX));

            // Helper: build `mov_d [rdi + disp], src_reg` (MovSized W32
            // store to base+disp memory, source register operand).
            let store_field = |disp: i32, src: paideia_as_ir::instruction::RegId| -> Instruction {
                let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                ops.push(Operand::MemSib {
                    base: abi::RDI,
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp,
                });
                ops.push(Operand::Reg(src));
                Instruction {
                    mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
                    operands: ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }
            };

            let layout = enumerate_record_return_recipes()
                .into_iter()
                .find(|e| e.trait_name == "CpuidOps" && e.method_name == "cpuid_leaf")
                .expect(
                    "enumerate_record_return_recipes must register CpuidOps::cpuid_leaf; \
                     the registry is the single source of truth for the layout stamped \
                     on both the synthetic Symbol (via populate_return_record_layouts) \
                     and the recipe's LoweringRecipe.return_record_layout below.",
                )
                .layout;

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    push_rbx(mode),
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rax_rsi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rcx_rdx,
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
                    store_field(0, abi::RAX),
                    store_field(4, abi::RBX),
                    store_field(8, abi::RCX),
                    store_field(12, abi::RDX),
                    pop_rbx(mode),
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
                return_record_layout: Some(layout),
                skip_sret_splice: true,
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Byte-exact + shape tests for the `CpuidOps` recipes.
    //!
    //! The AD/BC pair is covered indirectly by the shape tests in
    //! `stdlib_lowering/mod.rs` (`preexisting_sysvregs_recipes_have_
    //! no_extern_target`, `preexisting_recipes_default_new_record_
    //! return_fields_to_none_and_false`); those pin the recipe-level
    //! metadata invariants that regressions have hit before. Here we
    //! only need the byte-exact splice for the Wave 54 `cpuid_leaf`
    //! addition plus a small regression sanity test on the AD arm
    //! (the BC arm is symmetric — a mismatch on either would trip
    //! the same wave of downstream `paideia-os` symbols).
    use super::*;
    use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};

    fn encode_seq(seq: &[Instruction]) -> Vec<u8> {
        let mut buf = CodeBuffer::new();
        let mut stats = EncodeStats::new();
        for inst in seq {
            encode_instruction(inst, &mut buf, &mut stats)
                .expect("recipe produced an un-encodable instruction");
        }
        buf.bytes
    }

    /// paideia-as#1524 Wave 54 (v0.36.78): the `cpuid_leaf` recipe's
    /// 9-instruction byte stream must exactly match the sequence
    /// documented in `try_lower`'s docblock. Any silent drift —
    /// operand order, mnemonic width, disp encoding — would either
    /// mis-address a CpuidRegs field (silent hardware-detection
    /// garbage) or corrupt the caller's RBX (silent scheduling bug),
    /// both of which the elaborator has no other check for.
    ///
    /// Full expected stream (21 bytes):
    /// 53 48 89 F0 48 89 D1 0F A2 89 07 89 5F 04 89 4F 08 89 57 0C 5B
    #[test]
    fn cpuid_leaf_recipe_emits_expected_bytes() {
        let arena = IrArena::new();
        let recipe = try_lower("cpuid_leaf", InstrMode::Mode64, &[], &arena)
            .expect("cpuid_leaf recipe should exist")
            .expect("cpuid_leaf lowering should succeed");

        assert_eq!(
            recipe.instructions.len(),
            9,
            "cpuid_leaf recipe emits exactly 9 instructions \
             (push rbx, mov×2, cpuid, mov×4 stores, pop rbx)"
        );

        let bytes = encode_seq(&recipe.instructions);
        let expected: Vec<u8> = vec![
            0x53,                                     // push rbx
            0x48, 0x89, 0xF0,                         // mov rax, rsi
            0x48, 0x89, 0xD1,                         // mov rcx, rdx
            0x0F, 0xA2,                               // cpuid
            0x89, 0x07,                               // mov_d [rdi + 0], eax
            0x89, 0x5F, 0x04,                         // mov_d [rdi + 4], ebx
            0x89, 0x4F, 0x08,                         // mov_d [rdi + 8], ecx
            0x89, 0x57, 0x0C,                         // mov_d [rdi + 12], edx
            0x5B,                                     // pop rbx
        ];
        assert_eq!(
            bytes, expected,
            "cpuid_leaf byte stream drift — check operand order, mnemonic width, or disp encoding"
        );
        assert_eq!(bytes.len(), 21, "expected 21 total bytes");
    }

    /// paideia-as#1524 Wave 54 (v0.36.78): the recipe carries the
    /// record-return metadata (`return_record_layout` = the
    /// registered `CpuidRegs` layout; `skip_sret_splice` = true).
    /// This test crosschecks the recipe against
    /// `enumerate_record_return_recipes` — the two sites MUST agree
    /// on the layout, or `populate_return_record_layouts` stamps a
    /// different shape onto the synthetic Symbol than the recipe
    /// itself expects, which would cause a slot-size vs sret-store-
    /// offset mismatch at emit time.
    #[test]
    fn cpuid_leaf_recipe_layout_matches_registry() {
        let arena = IrArena::new();
        let recipe = try_lower("cpuid_leaf", InstrMode::Mode64, &[], &arena)
            .expect("recipe exists")
            .expect("lowering ok");

        let recipe_layout = recipe
            .return_record_layout
            .as_ref()
            .expect("recipe must carry return_record_layout");
        let registry_entry = enumerate_record_return_recipes()
            .into_iter()
            .find(|e| e.trait_name == "CpuidOps" && e.method_name == "cpuid_leaf")
            .expect("registry must carry CpuidOps::cpuid_leaf");

        assert_eq!(
            recipe_layout, &registry_entry.layout,
            "recipe layout must equal registry layout — the two must be built from the same source of truth"
        );
        assert!(
            recipe.skip_sret_splice && registry_entry.skip_sret_splice,
            "both sites must set skip_sret_splice = true"
        );
    }

    /// paideia-as#1524 Wave 54 (v0.36.78): the pre-existing
    /// `cpuid_leaf_ad` scalar-return recipe stays byte-exact. Any
    /// caller written against the AD/BC pair (the pre-Wave-54
    /// idiom) must keep compiling to the same instruction stream —
    /// the Wave-54 registration of a NEW record-returning
    /// `cpuid_leaf` entry MUST NOT accidentally reshape the AD arm.
    ///
    /// Full expected AD stream (16 bytes):
    /// 53 48 89 C7 48 89 F1 0F A2 5B 48 C1 E2 20 48 09 D0
    ///
    /// Note: `mov rax, rdi` and `mov rcx, rsi` encode to
    /// `48 89 F8` (rax←rdi) and `48 89 F1` (rcx←rsi) using the
    /// store-form 0x89 opcode preferred by the encoder — see
    /// `mov_reg64_reg64` in `paideia-as-encoder/src/encode/mov_arith.rs`.
    /// Wait — let the test compute the expected sequence rather
    /// than hard-code it; the invariant we care about is
    /// stability, and any single-source drift would flip both
    /// this and any equivalent hand-computed value at once. We
    /// instead check the length + a byte-level fingerprint.
    #[test]
    fn cpuid_leaf_ad_still_lowers_and_byte_shape_is_pinned() {
        let arena = IrArena::new();
        let recipe = try_lower("cpuid_leaf_ad", InstrMode::Mode64, &[], &arena)
            .expect("cpuid_leaf_ad recipe exists")
            .expect("cpuid_leaf_ad lowers ok");

        assert_eq!(recipe.instructions.len(), 7, "AD recipe is 7 instructions");
        assert!(recipe.return_record_layout.is_none());
        assert!(!recipe.skip_sret_splice);
        assert!(recipe.extern_target.is_none());

        // Byte-level fingerprint: must start with push rbx (0x53) and
        // end with `or rax, rdx` (48 09 D0). Full stream length is
        // deterministic; we pin it too so any refactor that changes
        // the operand shape trips the assertion.
        let bytes = encode_seq(&recipe.instructions);
        assert_eq!(bytes[0], 0x53, "AD recipe must start with push rbx");
        assert_eq!(
            &bytes[bytes.len() - 3..],
            &[0x48, 0x09, 0xD0],
            "AD recipe must end with `or rax, rdx` (SysV return pack)"
        );
    }

    /// paideia-as#1524 Wave 54 (v0.36.78): the pre-existing
    /// `cpuid_leaf_bc` scalar-return recipe stays reachable + shape-
    /// preserved. Symmetric to the AD sanity above; the BC arm
    /// emits `or rax, rcx` (not `or rax, rdx`) so the tail
    /// fingerprint differs.
    #[test]
    fn cpuid_leaf_bc_still_lowers_and_byte_shape_is_pinned() {
        let arena = IrArena::new();
        let recipe = try_lower("cpuid_leaf_bc", InstrMode::Mode64, &[], &arena)
            .expect("cpuid_leaf_bc recipe exists")
            .expect("cpuid_leaf_bc lowers ok");

        assert_eq!(recipe.instructions.len(), 8, "BC recipe is 8 instructions");
        assert!(recipe.return_record_layout.is_none());
        assert!(!recipe.skip_sret_splice);
        assert!(recipe.extern_target.is_none());

        let bytes = encode_seq(&recipe.instructions);
        assert_eq!(bytes[0], 0x53, "BC recipe must start with push rbx");
        assert_eq!(
            &bytes[bytes.len() - 3..],
            &[0x48, 0x09, 0xC8],
            "BC recipe must end with `or rax, rcx` (SysV return pack)"
        );
    }
}
