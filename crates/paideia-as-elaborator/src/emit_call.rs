//! Inter-function call lowering (Phase 7 m1-003 / PA7-006).
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts
//! `emit_function_call`, which lowers a `Call(target, args)` into the
//! System-V calling-convention marshalling sequence: per-arg moves into
//! `[RDI, RSI, RDX, RCX, R8, R9]` followed by `call target; ret`.
//!
//! # PAS-DEBT-B4-002 Slice B (paideia-as#1554): aggregate-return wiring
//!
//! Design decision — path (b): sret detection is subsumed into an
//! emit-call-site probe of `Symbol::return_record_layout` rather than a
//! new `ArgConvention::SysVSret` / `MsSret` variant. Rationale:
//!
//!   * `ArgConvention` is the stdlib-recipe input contract (see
//!     `stdlib_lowering.rs`). Aggregate return is a property of the
//!     callee's declared type, not of the recipe author. Threading it
//!     through the recipe convention would force every recipe author
//!     to opt in explicitly per-function; the layout-driven probe
//!     picks up any function whose declared return type is a record.
//!   * The existing `SysVRegs` recipe treatment (extern-thunk rewrites,
//!     scratch-save composition, MS shadow-space alignment) is
//!     orthogonal to the sret decision — one call can be both
//!     "recipe-driven" and "sret-shaped". Keeping the two decisions
//!     separate composes cleanly; a shared enum would force one to
//!     dominate.
//!   * `Symbol::return_record_layout` is populated by
//!     `return_record_layout_pass` (Slice A) and stamped onto the
//!     `Symbol` at walker Let-Lambda symbol construction. Every
//!     emit-call path reaches this side-table via
//!     `arena.symbols().lookup_by_name(&target_name)`.
//!
//! The branch: after resolving `callee_abi` and `arg_regs`, look up
//! the callee symbol's `return_record_layout`. If `Some(layout)`,
//! compute `SysvReturnPlacement` / `MsReturnPlacement` via
//! `abi::sysv_return_placement_from_layout` /
//! `abi::ms_return_placement_from_layout` and:
//!
//!   * On `needs_hidden_sret()`: reserve a stack slot (aligned to
//!     `layout.align`, rounded up to 16 bytes for SysV mod-16
//!     preservation), splice the caller sret prelude (`lea RDI, [rsp]`
//!     or `lea RCX, [rsp]`), shift the arg-register selection right by
//!     one, then let the arg-marshalling loop proceed as usual. After
//!     the CALL, release the slot with `add rsp, <padded>` so the
//!     mod-16 invariant is preserved for downstream code. Slice C will
//!     replace the release with a persistent frame-slot when the
//!     record-cons consumer reads from the sret buffer.
//!   * On register-return placement: leave the CALL emission
//!     byte-identical to the scalar path — the return value stays in
//!     RAX/RDX/XMM0/XMM1 for Slice C's caller-side pair-unpack to
//!     consume with a dest of its own.
//!   * On absent / scalar / `None` placement: the entire scalar path
//!     is preserved byte-identical. Non-record-return regression tests
//!     pin this invariant.
//!
//! Callee-side wiring (emitting `sysv_callee_sret_store` /
//! `ms_callee_sret_store` before the frame-pointer teardown) is
//! deferred to Slice C — it depends on the record-cons codepath
//! materialising the return value at a known callee-local disp so the
//! sret store can name a real source buffer. See
//! `emit_walker/emit_core.rs::emit_ret` for the Slice B / Slice C
//! marker on this path.

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi, PassingConvention};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::symbol::SymbolKind;
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};
use std::collections::HashSet;

use crate::emit_walker::EmitWalker;
use crate::stdlib_lowering::ArgConvention;

/// PAS-DEBT-B4-002 Slice B (paideia-as#1554): resolved aggregate-return
/// shape for a call site, keyed by the callee's ABI.
///
/// `SysvSret { padded_slot }` / `MsSret { padded_slot }` — the caller
/// must reserve `padded_slot` bytes on the stack before arg-marshalling,
/// splice the LEA sret prelude, and shift real arg registers by one.
/// The slot is released with `add rsp, padded_slot` after the CALL
/// (Slice B) so the SysV mod-16 stack invariant is preserved.
///
/// `Absent` — either the callee has no `return_record_layout`, or the
/// placement is `None` / register-shaped and the CALL emission stays
/// byte-identical to the scalar path.
#[derive(Debug, Clone, Copy)]
enum AggregateReturnShape {
    Absent,
    SysvSret { padded_slot: u32 },
    MsSret { padded_slot: u32 },
}

impl AggregateReturnShape {
    /// True when this call site needs a hidden sret pointer in the
    /// first arg register (RDI for SysV, RCX for MS), shifting the
    /// real args right by one.
    fn needs_sret(self) -> bool {
        matches!(self, Self::SysvSret { .. } | Self::MsSret { .. })
    }

    /// Bytes to `sub rsp` before arg-marshalling and `add rsp` after
    /// the CALL. Zero when no sret slot is needed.
    fn sret_slot_bytes(self) -> u32 {
        match self {
            Self::Absent => 0,
            Self::SysvSret { padded_slot } | Self::MsSret { padded_slot } => padded_slot,
        }
    }
}

/// Round `size` up to a multiple of `align`, then up to a multiple of
/// 16 bytes so SysV's `rsp mod 16 == 0 at CALL` invariant survives the
/// `sub rsp, padded` / `add rsp, padded` pair. `align` is unused today
/// (16 dominates every field alignment the layout computer produces)
/// but is threaded through so future oversized-align aggregate types
/// widen the padding rather than silently under-align.
#[inline]
fn sret_padded_slot_bytes(size: u64, align: u8) -> u32 {
    let a = core::cmp::max(align as u64, 16);
    let padded = (size + a - 1) & !(a - 1);
    // Round up to 16 unconditionally for SysV mod-16 preservation.
    let padded16 = (padded + 15) & !15;
    padded16 as u32
}

/// Resolve a target name to (trait_name, method_name) if it's a qualified stdlib trait method.
/// Returns None if the target is not in the form "TraitName::method_name".
fn resolve_stdlib_trait_method(target: &str) -> Option<(String, String)> {
    let (t, m) = target.rsplit_once("::")?;
    Some((t.to_string(), m.to_string()))
}

/// Helper to construct T0521 diagnostic code.
fn t0521_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 521)
        .expect("T0521 is within valid T range")
}

/// Helper to construct T0551 diagnostic code.
fn t0551_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 551)
        .expect("T0551 is within valid T range")
}

/// Helper to construct T0553 diagnostic code — undefined identifier at call site.
///
/// Issue #1260: fired by emit_call_expr when the callee name resolves to
/// none of {module symbol, stdlib trait recipe, local closure/fnptr}.
/// Without this the compiler silently emits an unresolvable relocation
/// and the caller sees a load-time SIGSEGV — a P0 miscompile.
fn t0553_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 553)
        .expect("T0553 is within valid T range")
}

/// Diagnostic helpers for #1226 (caller-side pos-0 pair enum args)
fn u1670_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1670)
        .expect("U1670 is within valid U range")
}

fn u1671_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1671)
        .expect("U1671 is within valid U range")
}

fn u1672_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1672)
        .expect("U1672 is within valid U range")
}

fn u1673_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1673)
        .expect("U1673 is within valid U range")
}

/// Payload source for literal enum constructor at pos 0
#[derive(Debug)]
enum PayloadSrc {
    Imm(i64),
    Reg(RegId),
}

/// Discriminant+payload pair at pos 0 in call arguments
#[derive(Debug)]
enum PosZeroPair {
    None,
    VarPair { disc: RegId, payload: RegId },
    NestedApp { callee: String, nested_args: Vec<IrNodeId> },
    LiteralCons { disc: i64, payload: PayloadSrc },
}

impl EmitWalker {
    /// Classify pos-0 argument as a register-pair enum form.
    /// Returns PosZeroPair enum indicating which case applies (if any).
    /// No emission; only reads state + arena.
    fn classify_pos_zero_pair(&self, arg_ids: &[IrNodeId], arena: &IrArena) -> PosZeroPair {
        // If no args, return None
        if arg_ids.is_empty() {
            return PosZeroPair::None;
        }

        let arg_id = arg_ids[0];
        let arg_node = match arena.get(arg_id) {
            Some(node) => node,
            None => return PosZeroPair::None,
        };

        match arg_node.kind {
            IrKind::Var => {
                // Look up binding name and check for pair
                if let Some(name) = arena.binding_names().get(arg_id) {
                    if let Some((disc, Some(payload))) = self.state.local_bindings.get_pair(name) {
                        return PosZeroPair::VarPair { disc, payload };
                    }
                }
                PosZeroPair::None
            }
            IrKind::App => {
                // Look up call site info
                if let Some(call_info) = arena.call_sites().get(arg_id) {
                    let callee_name = call_info.callee_name.clone();
                    let nested_args = arena.children(arg_id)
                        .iter()
                        .skip(1)
                        .copied()
                        .collect();
                    return PosZeroPair::NestedApp { callee: callee_name, nested_args };
                }
                PosZeroPair::None
            }
            IrKind::EnumCons => {
                // Look up enum constructor info
                let info = match arena.enum_cons_info().get(arg_id) {
                    Some(i) => i,
                    None => return PosZeroPair::None,
                };

                let type_id = info.type_id;
                let layout = match arena.enum_layout_table().get(type_id) {
                    Some(l) => l,
                    None => return PosZeroPair::None,
                };

                // Only handle RegisterPair passing with payload
                if layout.passing_convention() != PassingConvention::RegisterPair
                    || layout.payload_size <= 0 {
                    return PosZeroPair::None;
                }

                // Check if there's a payload child
                let children = arena.children(arg_id);
                if children.is_empty() {
                    return PosZeroPair::None;
                }

                let child_id = children[0];
                let payload_src = if let Some(value) = arena.literal_values().get(child_id) {
                    PayloadSrc::Imm(value)
                } else if let Some(name) = arena.binding_names().get(child_id) {
                    if let Some(reg) = self.state.local_bindings.get(name) {
                        PayloadSrc::Reg(reg)
                    } else {
                        return PosZeroPair::None;
                    }
                } else {
                    return PosZeroPair::None;
                };

                PosZeroPair::LiteralCons {
                    disc: info.variant_index as i64,
                    payload: payload_src,
                }
            }
            _ => PosZeroPair::None,
        }
    }

    /// Emit caller-side bridge postlude for paideia→MS/SysV ABI crossing.
    /// Pops the registers in `save_regs` in REVERSE order (LIFO) after shadow-space restoration.
    /// Uses alloc_synthetic_id() per iteration to ensure unique IDs across all call sites.
    fn emit_bridge_postlude(&mut self, save_regs: &[RegId]) {
        if save_regs.is_empty() {
            return;
        }
        for &reg in save_regs.iter().rev() {
            let postlude_ir_id = self.alloc_synthetic_id();

            let mut pop_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            pop_ops.push(Operand::Reg(reg));

            let pop_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands: pop_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                        emission_order: 0,
        };

            self.emit_inst(postlude_ir_id, pop_inst);
        }
    }

    /// Phase 7 m1-003: Emit call arguments and CALL instruction (no RET).
    ///
    /// Emits argument marshalling (MOV to RDI, RSI, ... or RCX, RDX, ... depending on ABI)
    /// and CALL instruction. For MS x64 ABI, emits prelude (sub rsp) and postlude (add rsp).
    /// For paideia→MS/SysV crossing, emits caller-side bridge prelude (push R15, R14) before
    /// shadow-space adjustment and postlude (pop R14, R15) after shadow restoration.
    /// Does NOT emit RET. Call `emit_ret_after_call` separately for statement-position calls.
    ///
    /// Records lambda entry as a side effect.
    /// Uses per-call-site sequential IDs (alloc_synthetic_id) to ensure deterministic
    /// instruction ordering and eliminate ID collisions across multiple call sites in the
    /// same function.
    fn emit_call_args_and_call(
        &mut self,
        lambda_node_id: IrNodeId,
        target_name: String,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
        caller_abi: Option<CallingConvention>,
        // PAS-DEBT-B4-002 Slice C (paideia-as#1554): App node id of
        // this call site. When `Some(id)`, the caller-side sret path
        // consults `arena.caller_sret_slot_table().get(id)` for a
        // persistent caller-owned dest slot; `Some(_) + entry-present`
        // switches from Slice B's transient `sub/lea/add` triplet to
        // a persistent `lea rdi, [rbp - disp]` (slot lives in the
        // caller's frame, released by the enclosing `mov rsp, rbp`).
        // `None` — or an id with no entry — falls back to Slice B
        // byte-identical.
        app_id: Option<IrNodeId>,
    ) {
        // paideia-as#1305: `target_name` becomes mutable so a SysVRegs
        // recipe carrying `extern_target: Some(sym)` can rewrite the CALL
        // destination (see the sysv_recipe splice block below). The ABI
        // lookup below still uses the original `target_name`, which is
        // correct — the extern target is a plain SysV C function so the
        // arg-register pool selection does not change.
        let mut target_name = target_name;
        // Resolve callee's ABI:
        // - callee_abi_option: None if unannotated (paideia default), Some if explicitly annotated
        // - callee_abi: resolved to CallingConvention::Sysv if unannotated (for register selection)
        let callee_abi_option = arena.symbols().lookup_by_name(&target_name)
            .and_then(|s| s.abi);
        let callee_abi = callee_abi_option.unwrap_or(CallingConvention::Sysv);

        // Determine bridge save register set (caller crossing into different ABI)
        let bridge_saves = abi::bridge_save_set(caller_abi, callee_abi_option);

        // Select argument register pool based on ABI
        let arg_regs: &[_] = match callee_abi {
            CallingConvention::Ms => &abi::MS_ARG_REGS,
            CallingConvention::Sysv => &abi::ARG_REGS,
        };

        // PAS-DEBT-B4-002 Slice B (paideia-as#1554): resolve the callee's
        // aggregate-return shape.
        //
        // Look up `return_record_layout` on the callee's symbol.
        // Compose it with the ABI's placement selector — SysV uses
        // `sysv_return_placement_from_layout`, MS uses
        // `ms_return_placement_from_layout`. Only Memory placement
        // needs the caller-side sret dance (stack slot + LEA + arg
        // shift + slot release); register-return placements leave the
        // CALL emission byte-identical to the scalar path so Slice C
        // can wire the caller-side pair-unpack independently.
        //
        // See the module docstring above for the ArgConvention design
        // rationale (path (b)).
        let aggregate_shape: AggregateReturnShape = {
            let sym_layout = arena
                .symbols()
                .lookup_by_name(&target_name)
                .and_then(|s| s.return_record_layout.clone());
            match sym_layout {
                None => AggregateReturnShape::Absent,
                Some(layout) => match callee_abi {
                    CallingConvention::Sysv => {
                        let placement = abi::sysv_return_placement_from_layout(&layout);
                        if placement.needs_hidden_sret() {
                            let padded = sret_padded_slot_bytes(layout.size, layout.align);
                            AggregateReturnShape::SysvSret { padded_slot: padded }
                        } else {
                            // Register-return placement (IntSingle,
                            // SseSingle, IntPair, IntSse, SseInt,
                            // SsePair) or None. CALL emission stays
                            // byte-identical to the scalar path;
                            // Slice C wires caller-side pair-unpack.
                            AggregateReturnShape::Absent
                        }
                    }
                    CallingConvention::Ms => {
                        let placement = abi::ms_return_placement_from_layout(&layout);
                        if placement.needs_hidden_sret() {
                            let padded = sret_padded_slot_bytes(layout.size, layout.align);
                            AggregateReturnShape::MsSret { padded_slot: padded }
                        } else {
                            AggregateReturnShape::Absent
                        }
                    }
                },
            }
        };

        // sret shift: index-0 sret pointer consumes the first arg
        // register (RDI for SysV, RCX for MS); real args start at
        // index 1 in `arg_regs`. Zero for the non-sret path so
        // scalar-return regression tests stay byte-identical.
        let sret_shift: usize = if aggregate_shape.needs_sret() { 1 } else { 0 };

        // Allocate first instruction ID upfront. This will be used for:
        // 1. record_lambda_entry (marks function entry point)
        // 2. The first actual instruction emission (bridge push, MS prelude, first arg MOV, or CALL)
        let first_id = self.alloc_synthetic_id();
        self.record_lambda_entry(lambda_node_id, first_id);

        // Track whether we've emitted the first instruction yet
        let mut first_emission = true;

        // Issue #1163 (corrective): Compute scratch-save set BEFORE MS prelude emission
        // to determine dynamic MS bump based on parity. This hoisting avoids dependency
        // reordering and captures all caller-save registers that hold live bindings.
        // #1220: filter against live_regs() (includes pair-binding payload_reg)
        // rather than iter() (primary only). iter() left payload_regs unsaved
        // across CALLs → silent clobber of the pair's payload.
        let live = self.state.local_bindings.live_regs();
        let caller_save_scratch = [abi::RCX, abi::RDX, abi::R8, abi::R9];
        let scratch_save_set: Vec<RegId> = caller_save_scratch.iter()
            .copied()
            .filter(|r| live.contains(r))
            .collect();

        // v0.21-001 (#1277): count MS x64 stack-passed integer args (idx ≥ 4).
        // These live above the 32-byte shadow area, at caller's [rsp + 32 + 8*(idx-4)]
        // — which the callee sees at [rsp + 40 + 8*(idx-4)] after the CALL push
        // (an 8-byte return-address offset accounts for the difference between the
        // two frames). The bump must reserve room for them (`8 * stack_arg_count`)
        // plus an alignment pad when the count is odd — a single extra 8-byte slot
        // that keeps `ms_bump mod 16` invariant relative to the 4-arg baseline.
        //
        // Only MS callees are handled here. A SysV callee with > 6 args still
        // fires T0521 in the arg-marshalling loop (out of scope for this bundle;
        // the paideia-os UEFI use case is MS-only).
        let ms_stack_arg_count: usize = if callee_abi == CallingConvention::Ms {
            arg_ids.len().saturating_sub(abi::MS_ARG_REGS.len())
        } else {
            0
        };
        let ms_stack_arg_bytes: u32 = (ms_stack_arg_count as u32) * 8;
        let ms_stack_arg_pad: u32 = if ms_stack_arg_count % 2 == 1 { 8 } else { 0 };

        // #1192: Compute dynamic MS shadow-space bump based on scratch-save parity.
        // Entry RSP ≡ 8 mod 16 (return address already on stack).
        // bridge_saves = 2 pushes (R15, R14) if paideia→MS/SysV crossing, else 0.
        // scratch_saves = scratch_save_set.len() pushes.
        // Total prelude bump = bridge_saves*8 + ms_bump + scratch_saves*8.
        // Require: (bridge_saves*8 + ms_bump + scratch_saves*8 + 8) ≡ 16 mod 16
        // ⟹ ms_bump ≡ 0 mod 16 when (bridge_saves + scratch_saves) is even
        // ⟹ ms_bump ≡ 8 mod 16 when (bridge_saves + scratch_saves) is odd
        // When callee_abi == Ms, both even and odd cases need ms_bump ≥ 40.
        // If scratch_save_set.len() is odd, add 8 to restore 0-mod-16 alignment.
        //
        // v0.21-001 (#1277): `ms_stack_arg_bytes + ms_stack_arg_pad` reserves the
        // stack-passed-arg slots above the 32-byte shadow area; both addends are
        // multiples of 16 together (raw bytes + odd-count pad), so the base
        // alignment invariant above is preserved.
        let ms_bump: u32 = if callee_abi == CallingConvention::Ms {
            let base = if scratch_save_set.len() % 2 == 1 {
                abi::MS_CALL_STACK_BUMP + abi::MS_CALL_STACK_BUMP_ODD_PAD
            } else {
                abi::MS_CALL_STACK_BUMP
            };
            base + ms_stack_arg_bytes + ms_stack_arg_pad
        } else {
            0
        };

        // v0.22.0 (#1326 phase 2): count SysV stack-passed integer args
        // (idx ≥ 6, i.e. beyond the 6-register ARG_REGS pool). These live
        // at the caller's [rsp + 8*(idx-6)] (no shadow-space offset — SysV
        // differs from MS here), reserved by the prelude bump below.
        // `sysv_stack_arg_bytes` is always a multiple of 8; the odd-count
        // pad keeps `sysv_stack_arg_bytes + sysv_stack_arg_pad` a multiple
        // of 16, so this addend preserves RSP mod-16 alignment on its own
        // (design doc §4.2). Applies whenever the callee resolves to SysV
        // (the default when unannotated), matching the register-selection
        // logic above.
        let sysv_stack_arg_count: usize = if callee_abi == CallingConvention::Sysv {
            arg_ids.len().saturating_sub(abi::ARG_REGS.len())
        } else {
            0
        };
        let sysv_stack_arg_bytes: u32 = (sysv_stack_arg_count as u32) * 8;
        let sysv_stack_arg_pad: u32 = if sysv_stack_arg_count % 2 == 1 { 8 } else { 0 };

        // #1195: Compute dynamic SysV alignment pad based on scratch-save parity.
        // Only for EXPLICIT SysV ABI calls (callee_abi_option == Some(Sysv)) with bridge saves.
        // When paideia→SysV cross-call and scratch count is even, emit 8-byte pad before
        // scratch saves to restore RSP ≡ 0 mod 16 at CALL.
        let sysv_align_pad: u32 = if callee_abi_option == Some(CallingConvention::Sysv) && !bridge_saves.is_empty() {
            if scratch_save_set.len() % 2 == 0 {
                abi::SYSV_CALL_ALIGN_PAD
            } else {
                0
            }
        } else {
            0
        };

        // v0.22.0 (#1326 phase 2): total SysV prelude/postlude bump combines
        // the stack-arg reservation (independently mod-16, see above) with
        // the pre-existing #1195 bridge-parity pad. Both addends preserve
        // RSP mod 16 on their own, so they compose additively.
        let sysv_bump: u32 = sysv_stack_arg_bytes + sysv_stack_arg_pad + sysv_align_pad;

        // Emit caller-side bridge prelude (push R15, R14) if crossing paideia→MS/SysV
        // Use first_id for the first push, subsequent pushes get fresh IDs
        if !bridge_saves.is_empty() {
            for (idx, &reg) in bridge_saves.iter().enumerate() {
                let prelude_ir_id = if idx == 0 && first_emission {
                    first_emission = false;
                    first_id
                } else {
                    self.alloc_synthetic_id()
                };

                let mut push_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                push_ops.push(Operand::Reg(reg));

                let push_inst = Instruction {
                    mnemonic: Mnemonic::Push,
                    operands: push_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };

                self.emit_inst(prelude_ir_id, push_inst);
            }
        }

        // Emit MS x64 prelude: sub rsp, ms_bump (dynamic based on scratch-save parity)
        if callee_abi == CallingConvention::Ms {
            let ms_prelude_id = if first_emission {
                first_emission = false;
                first_id
            } else {
                self.alloc_synthetic_id()
            };

            let mut prelude_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            prelude_ops.push(Operand::Reg(abi::RSP));
            prelude_ops.push(Operand::Imm64(ms_bump as i64));

            let prelude_inst = Instruction {
                mnemonic: Mnemonic::Sub,
                operands: prelude_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };

            self.emit_inst(ms_prelude_id, prelude_inst);
        }

        // #1195 / v0.22.0 (#1326 phase 2): Emit combined SysV prelude bump —
        // stack-arg reservation (idx ≥ 6) plus the #1195 bridge-parity pad.
        if sysv_bump > 0 {
            let sysv_prelude_id = if first_emission {
                first_emission = false;
                first_id
            } else {
                self.alloc_synthetic_id()
            };

            let mut prelude_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            prelude_ops.push(Operand::Reg(abi::RSP));
            prelude_ops.push(Operand::Imm64(sysv_bump as i64));

            let prelude_inst = Instruction {
                mnemonic: Mnemonic::Sub,
                operands: prelude_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };

            self.emit_inst(sysv_prelude_id, prelude_inst);
        }

        // Issue #1163 (corrective): Spill live caller-save scratch bindings before arg-MOVs
        // scratch_save_set was computed earlier (before MS prelude) for #1192 alignment calculation.
        // Now emit the push instructions for each register in the set.
        for &reg in &scratch_save_set {
            let scratch_save_id = self.alloc_synthetic_id();

            let mut push_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            push_ops.push(Operand::Reg(reg));

            let push_inst = Instruction {
                mnemonic: Mnemonic::Push,
                operands: push_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };

            self.emit_inst(scratch_save_id, push_inst);
        }

        // PAS-DEBT-B4-002 Slice B (paideia-as#1554): caller-side sret
        // prelude for a Memory-classified aggregate-returning callee.
        //
        // Emitted here — after every RSP-adjusting prelude (bridge
        // pushes, MS shadow-space bump, SysV stack-arg bump, scratch
        // pushes) and BEFORE arg-marshalling — so the LEA can name
        // `[rsp]` with a zero displacement. The reserved slot sits at
        // the top of the caller's stack until the matching `add rsp`
        // fires after the CALL (see the postlude block near the CALL
        // emission below).
        //
        // Slot size (`padded_slot`) is rounded up to 16 bytes so the
        // sub / add pair preserves SysV's `rsp mod 16 == 0 at CALL`
        // invariant on its own (composes additively with `sysv_bump`
        // and `ms_bump`, both of which are also 16-multiples).
        //
        // Sret pointer register: RDI for SysV (real args shift right
        // to RSI, RDX, RCX, R8, R9), RCX for MS (real args shift right
        // to RDX, R8, R9). The arg-marshalling loop below uses
        // `arg_regs[arg_idx + sret_shift]` to consume the shifted
        // register pool.
        // PAS-DEBT-B4-002 Slice C (paideia-as#1554): persistent-slot
        // path. When `app_id` names an entry in the caller-side sret
        // slot table (populated by `return_record_cons_pass`), skip
        // the Slice B transient `sub rsp, N` and emit only the LEA
        // pointing into the caller's frame at `[rbp - disp]`. The
        // caller's prologue reserved this slot once at function entry
        // (see `emit_visit_lambda`'s sret bump block); the enclosing
        // `mov rsp, rbp` teardown releases it at RET time. Nothing to
        // add rsp after the CALL — the slot lives on.
        //
        // PAS-DEBT-B4-002 Slice D (paideia-as#1554): the pass now
        // allocates slot entries for BOTH Memory and register-return
        // callees so the post-CALL pair-unpack has a durable
        // destination (see `return_record_cons_pass::PlacementShapeInner`).
        // The `@no_frame` caller-side guard below discards the
        // persistent slot for callers whose prologue never emits
        // `push rbp; mov rbp, rsp` — those callers have no RBP
        // anchor to reference, and Slice D preserves their Slice B
        // byte identity (transient sret for Memory; no wiring for
        // register-return). Slice C's `sret_bump` prologue in
        // `emit_visit_lambda.rs` carries the matching `!is_no_frame`
        // guard.
        let is_caller_no_frame = self.state.is_lambda_no_frame(lambda_node_id.get());
        let persistent_slot: Option<paideia_as_ir::CallerSretSlot> = if is_caller_no_frame {
            None
        } else {
            app_id.and_then(|id| arena.caller_sret_slot_table().get(id).copied())
        };
        if aggregate_shape.needs_sret() {
            let sret_reg = match aggregate_shape {
                AggregateReturnShape::SysvSret { .. } => abi::RDI,
                AggregateReturnShape::MsSret { .. } => abi::RCX,
                AggregateReturnShape::Absent => unreachable!(),
            };

            if let Some(slot) = persistent_slot {
                // Slice C persistent-slot path: single LEA at
                // `[rbp + slot.rbp_disp]` (rbp_disp is negative).
                // No `sub rsp` here — the caller's frame prologue
                // reserved the slot; no `add rsp` after CALL either.
                let lea_id = if first_emission {
                    first_emission = false;
                    first_id
                } else {
                    self.alloc_synthetic_id()
                };
                let mut lea_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                lea_ops.push(Operand::Reg(sret_reg));
                lea_ops.push(Operand::MemSib {
                    base: abi::RBP,
                    index: None,
                    scale: Scale::X1,
                    disp: slot.rbp_disp,
                });
                let lea_inst = Instruction {
                    mnemonic: Mnemonic::Lea,
                    operands: lea_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(lea_id, lea_inst);
            } else {
                let padded_slot = aggregate_shape.sret_slot_bytes();

                // Slice B fallback: transient slot around the CALL.
                // sub rsp, <padded_slot>
                let sret_sub_id = if first_emission {
                    first_emission = false;
                    first_id
                } else {
                    self.alloc_synthetic_id()
                };
                let mut sub_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                sub_ops.push(Operand::Reg(abi::RSP));
                sub_ops.push(Operand::Imm64(padded_slot as i64));
                let sub_inst = Instruction {
                    mnemonic: Mnemonic::Sub,
                    operands: sub_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(sret_sub_id, sub_inst);

                // lea <sret_reg>, [rsp + 0]
                let lea_id = self.alloc_synthetic_id();
                let mut lea_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                lea_ops.push(Operand::Reg(sret_reg));
                lea_ops.push(Operand::MemSib {
                    base: abi::RSP,
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                });
                let lea_inst = Instruction {
                    mnemonic: Mnemonic::Lea,
                    operands: lea_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(lea_id, lea_inst);
            }
        }

        // #1226: Classify pos-0 argument for register-pair enum handling.
        // This determines whether pos-0 should be hoisted out of the sequential loop.
        let pos_zero_pair = self.classify_pos_zero_pair(arg_ids, arena);

        // PA-r16-007-registry-runtime-args (#1062): stdlib trait method lowering with
        // arg-convention awareness. Literal recipes skip arg-marshalling entirely;
        // SysVRegs recipes fall through to arg-marshalling then splice.
        let mut sysv_recipe: Option<crate::stdlib_lowering::LoweringRecipe> = None;
        if let Some((trait_name, method_name)) = resolve_stdlib_trait_method(&target_name) {
            if let Some(recipe_result) = crate::stdlib_lowering::lower_stdlib_method(
                &trait_name,
                &method_name,
                self.current_mode(),
                arg_ids,
                arena,
            ) {
                match recipe_result {
                    Ok(recipe) => match recipe.arg_convention {
                        ArgConvention::Literal => {
                            // Literal path: splice instructions and return immediately,
                            // skipping arg-marshalling and Call+Ret.
                            // PA-r16-007 (#1066): Handle local labels by mangling and registering.
                            let mangle = |local: &str| {
                                format!("__recipe_{}_{}", lambda_node_id.get(), local)
                            };
                            let label_names: HashSet<&str> =
                                recipe.labels.iter().map(|(n, _)| *n).collect();

                            for (i, mut inst) in recipe.instructions.into_iter().enumerate() {
                                // Rewrite label refs in operands
                                for op in inst.operands.iter_mut() {
                                    if let Operand::LabelRef { name, .. } = op {
                                        if label_names.contains(name.as_str()) {
                                            *name = mangle(name);
                                        }
                                    }
                                }

                                let iid = IrNodeId::new(lambda_node_id.get() * 16 + (i as u32) + 1)
                                    .expect("stdlib recipe virtual id");

                                // Register this instruction's label bindings BEFORE emit_inst
                                for (local_name, idx) in &recipe.labels {
                                    if *idx == i {
                                        self.state.insert_label(mangle(local_name), iid);
                                    }
                                }

                                self.emit_inst(iid, inst);
                            }
                            return;
                        }
                        ArgConvention::SysVRegs => {
                            // SysVRegs path: stash recipe, fall through to arg-marshalling,
                            // then splice and return.
                            sysv_recipe = Some(recipe);
                        }
                    },
                    Err(crate::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
                        arg_index,
                        method,
                    }) => {
                        // T0551: stdlib intrinsic requires integer-literal argument
                        self.push_typed_diag(
                            t0551_code(),
                            format!(
                                "stdlib intrinsic requires integer-literal argument: {} arg {}",
                                method, arg_index
                            ),
                        );
                        // Fall through to normal call emission
                    }
                }
            }
        }

        // Emit MOV instructions for each argument
        for (arg_idx, &arg_id) in arg_ids.iter().enumerate() {
            // #1226: Skip pos-0 if it's a register-pair enum (hoisted for separate handling)
            if arg_idx == 0 && !matches!(pos_zero_pair, PosZeroPair::None) {
                continue;
            }

            // PAS-DEBT-B4-002 Slice B (paideia-as#1554): shift real args
            // right by one when the callee takes a hidden sret pointer
            // (which occupies arg_regs[0]).
            //
            // A sret-carrying call with more real args than fit in the
            // shifted register pool (SysV: 5+ real args; MS: 3+ real
            // args) would need stack-passed args at an offset above
            // the sret slot. Slice B rejects this shape with T0521 —
            // no fixture in scope exercises it, and the stack-off math
            // in the stack-passed branch below does not yet account
            // for the sret slot's `sub rsp` bump. Slice C revisits
            // if a real fixture demands it.
            let effective_arg_idx = arg_idx + sret_shift;
            if effective_arg_idx >= arg_regs.len() {
                if sret_shift > 0 {
                    self.push_typed_diag(
                        t0521_code(),
                        format!(
                            "sret-shape call with real arg {} spilling past register \
                             pool not yet supported (Slice C follow-up); \
                             reduce the callee's arg count or split its return type",
                            arg_idx
                        ),
                    );
                    continue;
                }
                if callee_abi == CallingConvention::Ms {
                    // v0.21-001 (#1277): MS x64 stack passing for arg 5+.
                    //
                    // Callee expects arg[idx] (idx ≥ 4) at [callee_rsp + 40 + 8*(idx-4)]
                    // after the CALL push. Caller writes to [rsp + 32 + 8*(idx-4)],
                    // which shifts by -8 across the CALL push to the callee slot.
                    //
                    // Emission-order note: writing at idx ≥ 4 only ever runs after
                    // the 4 register-arg MOVs (idx 0..4) have already fired, so
                    // `first_id` has already been consumed by bridge_saves and/or
                    // the MS prelude — both fire before this loop is entered on
                    // every MS-callee call path. Nothing to hand `first_id` here;
                    // always allocate a fresh id.
                    let stack_off: i32 = 32
                        + 8 * (arg_idx as i32 - abi::MS_ARG_REGS.len() as i32);
                    let arg_node_kind = arena.get(arg_id).map(|n| n.kind);
                    match arg_node_kind {
                        Some(IrKind::Literal) => {
                            if let Some(value) = arena.literal_values().get(arg_id) {
                                let store_id = self.alloc_synthetic_id();
                                self.emit_mov_stack_slot_imm(store_id, stack_off, value);
                            } else {
                                self.push_typed_diag(
                                    t0521_code(),
                                    format!(
                                        "MS x64 stack arg {}: literal has no value",
                                        arg_idx
                                    ),
                                );
                            }
                        }
                        Some(IrKind::Var) => {
                            let src_reg = arena
                                .binding_names()
                                .get(arg_id)
                                .and_then(|name| self.state.local_bindings.get(name));
                            match src_reg {
                                Some(src) => {
                                    let store_id = self.alloc_synthetic_id();
                                    self.emit_mov_stack_slot_reg(store_id, stack_off, src);
                                }
                                None => {
                                    // Module-level Object constant path: materialise
                                    // via a RIP-relative load into R10, then store.
                                    let module_const = arena
                                        .binding_names()
                                        .get(arg_id)
                                        .and_then(|name| {
                                            arena
                                                .symbols()
                                                .lookup_by_name(name)
                                                .filter(|s| {
                                                    matches!(s.kind, SymbolKind::Object)
                                                })
                                                .map(|_| name.to_string())
                                        });
                                    if let Some(name) = module_const {
                                        let load_id = self.alloc_synthetic_id();
                                        self.emit_mem_read_via_rip_sym(
                                            load_id, abi::R10, name, 0, 8, false,
                                        );
                                        let store_id = self.alloc_synthetic_id();
                                        self.emit_mov_stack_slot_reg(
                                            store_id, stack_off, abi::R10,
                                        );
                                    } else {
                                        self.push_typed_diag(
                                            t0521_code(),
                                            format!(
                                                "MS x64 stack arg {}: Var has no local \
                                                 binding entry; source register \
                                                 unresolvable",
                                                arg_idx
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        Some(IrKind::EnumCons) => {
                            if let Some(info) = arena.enum_cons_info().get(arg_id) {
                                if arena.children(arg_id).is_empty() {
                                    let store_id = self.alloc_synthetic_id();
                                    self.emit_mov_stack_slot_imm(
                                        store_id,
                                        stack_off,
                                        info.variant_index as i64,
                                    );
                                    self.state.mark_enum_cons_handled(arg_id.get());
                                } else {
                                    self.push_typed_diag(
                                        t0521_code(),
                                        format!(
                                            "MS x64 stack arg {}: payload-bearing enum \
                                             literal not yet supported (use let-binding)",
                                            arg_idx
                                        ),
                                    );
                                }
                            } else {
                                self.push_typed_diag(
                                    t0521_code(),
                                    format!(
                                        "MS x64 stack arg {}: EnumCons missing metadata",
                                        arg_idx
                                    ),
                                );
                            }
                        }
                        _ => {
                            self.push_typed_diag(
                                t0521_code(),
                                format!(
                                    "MS x64 stack arg {}: kind not yet supported for \
                                     stack passing",
                                    arg_idx
                                ),
                            );
                        }
                    }
                    continue;
                }
                // v0.22.0 (#1326 phase 2): SysV stack passing for arg 7+
                // (idx ≥ 6, i.e. beyond ARG_REGS' 6 slots).
                //
                // Callee expects arg[idx] (idx ≥ 6) at [callee_rsp + 8 +
                // 8*(idx-6)] after the CALL push (no shadow-space offset —
                // SysV differs from MS here). Caller writes to
                // [rsp + 8*(idx-6)], which shifts by +8 across the CALL
                // push to the callee-observed slot.
                //
                // Emission-order note (mirrors #1277): writing at idx ≥ 6
                // only ever runs after the 6 register-arg MOVs (idx 0..6)
                // have already fired, so `first_id` has already been
                // consumed by bridge_saves and/or the SysV prelude — both
                // fire before this loop is entered on every SysV-callee
                // call path. Always allocate a fresh id.
                let stack_off: i32 =
                    8 * (arg_idx as i32 - abi::ARG_REGS.len() as i32);
                let arg_node_kind = arena.get(arg_id).map(|n| n.kind);
                match arg_node_kind {
                    Some(IrKind::Literal) => {
                        if let Some(value) = arena.literal_values().get(arg_id) {
                            let store_id = self.alloc_synthetic_id();
                            self.emit_mov_stack_slot_imm(store_id, stack_off, value);
                        } else {
                            self.push_typed_diag(
                                t0521_code(),
                                format!(
                                    "SysV stack arg {}: literal has no value",
                                    arg_idx
                                ),
                            );
                        }
                    }
                    Some(IrKind::Var) => {
                        let src_reg = arena
                            .binding_names()
                            .get(arg_id)
                            .and_then(|name| self.state.local_bindings.get(name));
                        match src_reg {
                            Some(src) => {
                                let store_id = self.alloc_synthetic_id();
                                self.emit_mov_stack_slot_reg(store_id, stack_off, src);
                            }
                            None => {
                                // Module-level Object constant path: materialise
                                // via a RIP-relative load into R10, then store.
                                let module_const = arena
                                    .binding_names()
                                    .get(arg_id)
                                    .and_then(|name| {
                                        arena
                                            .symbols()
                                            .lookup_by_name(name)
                                            .filter(|s| {
                                                matches!(s.kind, SymbolKind::Object)
                                            })
                                            .map(|_| name.to_string())
                                    });
                                if let Some(name) = module_const {
                                    let load_id = self.alloc_synthetic_id();
                                    self.emit_mem_read_via_rip_sym(
                                        load_id, abi::R10, name, 0, 8, false,
                                    );
                                    let store_id = self.alloc_synthetic_id();
                                    self.emit_mov_stack_slot_reg(
                                        store_id, stack_off, abi::R10,
                                    );
                                } else {
                                    self.push_typed_diag(
                                        t0521_code(),
                                        format!(
                                            "SysV stack arg {}: Var has no local \
                                             binding entry; source register \
                                             unresolvable",
                                            arg_idx
                                        ),
                                    );
                                }
                            }
                        }
                    }
                    Some(IrKind::EnumCons) => {
                        if let Some(info) = arena.enum_cons_info().get(arg_id) {
                            if arena.children(arg_id).is_empty() {
                                let store_id = self.alloc_synthetic_id();
                                self.emit_mov_stack_slot_imm(
                                    store_id,
                                    stack_off,
                                    info.variant_index as i64,
                                );
                                self.state.mark_enum_cons_handled(arg_id.get());
                            } else {
                                self.push_typed_diag(
                                    t0521_code(),
                                    format!(
                                        "SysV stack arg {}: payload-bearing enum \
                                         literal not yet supported (use let-binding)",
                                        arg_idx
                                    ),
                                );
                            }
                        } else {
                            self.push_typed_diag(
                                t0521_code(),
                                format!(
                                    "SysV stack arg {}: EnumCons missing metadata",
                                    arg_idx
                                ),
                            );
                        }
                    }
                    _ => {
                        self.push_typed_diag(
                            t0521_code(),
                            format!(
                                "SysV stack arg {}: kind not yet supported for \
                                 SysV stack passing",
                                arg_idx
                            ),
                        );
                    }
                }
                continue;
            }

            // PAS-DEBT-B4-002 Slice B: real args shift right by
            // `sret_shift` when the callee takes a hidden sret
            // pointer. `effective_arg_idx` was computed above.
            let dest_reg = arg_regs[effective_arg_idx];
            let arg_node = match arena.get(arg_id) {
                Some(node) => node,
                None => {
                    self.push_typed_diag(
                        t0521_code(),
                        format!("arg {} not found in IR", arg_idx),
                    );
                    continue;
                }
            };

            // Handle various argument sources
            match arg_node.kind {
                IrKind::Literal => {
                    // Load literal into the register
                    if let Some(value) = arena.literal_values().get(arg_id) {
                        // Allocate a fresh ID for this MOV; mark first if still needed
                        let mov_id = if first_emission {
                            first_emission = false;
                            first_id
                        } else {
                            self.alloc_synthetic_id()
                        };
                        self.emit_mov_literal_to_reg_with_id(mov_id, dest_reg, value);
                    } else {
                        self.push_typed_diag(
                            t0521_code(),
                            format!("literal arg {} has no value", arg_idx),
                        );
                    }
                }
                IrKind::Var => {
                    // #1226: Check for pair-form Var at pos > 0 (not yet supported)
                    if arg_idx > 0 {
                        if let Some(name) = arena.binding_names().get(arg_id) {
                            if let Some((_, Some(_))) = self.state.local_bindings.get_pair(name) {
                                self.push_typed_diag(
                                    u1670_code(),
                                    format!("pair-form Var arg at position {} (> 0) not yet supported", arg_idx),
                                );
                                continue;
                            }
                        }
                    }

                    // Resolve the Var's source register: look up its binding name in
                    // local_bindings (the caller's parameter/let scratch table). If the
                    // source differs from dest_reg, emit `mov dest, src`; equal-reg is
                    // a no-op.
                    let src_reg = arena
                        .binding_names()
                        .get(arg_id)
                        .and_then(|name| self.state.local_bindings.get(name));
                    match src_reg {
                        Some(src) if src == dest_reg => {
                            // No-op: caller's binding already lives in the target arg reg.
                        }
                        Some(src) => {
                            let mov_id = if first_emission {
                                first_emission = false;
                                first_id
                            } else {
                                self.alloc_synthetic_id()
                            };
                            self.emit_mov_reg_to_reg_with_id(mov_id, src, dest_reg);
                        }
                        None => {
                            // Issue #1176: Check for module-level Object constant.
                            // If the Var references a module-level let CONST : u64 = LIT,
                            // it lives in arena.symbols(), not state.local_bindings.
                            let module_const = arena
                                .binding_names()
                                .get(arg_id)
                                .and_then(|name| {
                                    arena.symbols().lookup_by_name(name)
                                        .filter(|s| matches!(s.kind, SymbolKind::Object))
                                        .map(|s| (name.to_string(), s))
                                })
                            ;

                            if let Some((name, _sym)) = module_const {
                                // Emit RIP-relative load: mov dest_reg, [rip+name]
                                let mov_id = if first_emission {
                                    first_emission = false;
                                    first_id
                                } else {
                                    self.alloc_synthetic_id()
                                };
                                self.emit_mem_read_via_rip_sym(mov_id, dest_reg, name, 0, 8, false);
                            } else {
                                // Legacy fallback for arg 0: if the binding table is not
                                // populated (older test IR shapes) and dest_reg != RDI, assume
                                // the caller's first param is in RDI.
                                if arg_idx == 0 && dest_reg != abi::RDI {
                                    let mov_id = if first_emission {
                                        first_emission = false;
                                        first_id
                                    } else {
                                        self.alloc_synthetic_id()
                                    };
                                    self.emit_mov_reg_to_reg_with_id(mov_id, abi::RDI, dest_reg);
                                } else {
                                    self.push_typed_diag(
                                        t0521_code(),
                                        format!(
                                            "Var arg {} has no local binding entry; source register unresolvable",
                                            arg_idx
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
                IrKind::App => {
                    // #1226: App args at pos > 0 with pair return not yet supported
                    self.push_typed_diag(
                        u1670_code(),
                        format!("nested pair-returning App arg at position {} (> 0) not yet supported; use stack spilling", arg_idx),
                    );
                }
                IrKind::EnumCons => {
                    let info = match arena.enum_cons_info().get(arg_id) {
                        Some(i) => i,
                        None => {
                            self.push_typed_diag(
                                t0521_code(),
                                format!("EnumCons arg {} missing EnumConsInfo", arg_idx),
                            );
                            continue;
                        }
                    };
                    // Payload-bearing enum literal as call arg: not supported by this fix.
                    if !arena.children(arg_id).is_empty() {
                        self.push_typed_diag(
                            t0521_code(),
                            format!(
                                "payload-bearing enum literal as call arg {} not yet supported (use let-binding)",
                                arg_idx
                            ),
                        );
                        continue;
                    }
                    let mov_id = if first_emission {
                        first_emission = false;
                        first_id
                    } else {
                        self.alloc_synthetic_id()
                    };
                    self.emit_mov_literal_to_reg_with_id(mov_id, dest_reg, info.variant_index as i64);
                    self.state.mark_enum_cons_handled(arg_id.get());
                }
                _ => {
                    // Other argument shapes not yet supported
                    self.push_typed_diag(
                        t0521_code(),
                        format!("arg {} kind not supported", arg_idx),
                    );
                }
            }
        }

        // #1226: Emit pos-0 register-pair enum arguments AFTER loop, BEFORE sysv_recipe.
        // Ordering-critical: ensures discriminant and payload are set up correctly
        // for the register-pair calling convention (RAX + RDX).
        // Check for multi-arg conflicts first.
        let mut pair_emission_ok = true;
        if !matches!(pos_zero_pair, PosZeroPair::None) && arg_ids.len() >= 2 {
            // U1671: nested App + more args
            if matches!(pos_zero_pair, PosZeroPair::NestedApp { .. }) {
                self.push_typed_diag(
                    u1671_code(),
                    "nested pair-returning App arg with additional args (RDX/RSI clobber risk)".to_string(),
                );
                pair_emission_ok = false;
            }
            // U1672: pos-2 clobbers RDX
            if arg_ids.len() >= 3 {
                self.push_typed_diag(
                    u1672_code(),
                    "pair-form arg at position 0 conflicts with pos-2 arg (RDX clobber)".to_string(),
                );
                pair_emission_ok = false;
            }
        }

        // U1673: Check for source-reg alias in additional args
        if pair_emission_ok && !matches!(pos_zero_pair, PosZeroPair::None) {
            if let PosZeroPair::VarPair { disc, payload } = pos_zero_pair {
                for (arg_idx, &arg_id) in arg_ids.iter().enumerate().skip(1) {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        if let Some(reg) = self.state.local_bindings.get(name) {
                            if reg == disc || reg == payload {
                                self.push_typed_diag(
                                    u1673_code(),
                                    format!("pair-arg source register {} aliases a pos-{} arg-reg target",
                                            if reg == disc { "disc" } else { "payload" }, arg_idx),
                                );
                                pair_emission_ok = false;
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Emit the pair if no conflicts
        if pair_emission_ok {
            match &pos_zero_pair {
                PosZeroPair::None => {}
                PosZeroPair::VarPair { disc, payload } => {
                    // Emit discriminant to RAX if not already there
                    if *disc != abi::RAX {
                        let mov_id = self.alloc_synthetic_id();
                        self.emit_mov_reg_to_reg_with_id(mov_id, *disc, abi::RAX);
                    }
                    // Emit payload to RDX if not already there
                    if *payload != abi::RDX {
                        let mov_id = self.alloc_synthetic_id();
                        self.emit_mov_reg_to_reg_with_id(mov_id, *payload, abi::RDX);
                    }
                }
                PosZeroPair::NestedApp { callee, nested_args } => {
                    // Recursive call to emit nested app; return value lands in RAX + RDX
                    self.emit_call_expr(lambda_node_id, callee.clone(), nested_args, arena);
                }
                PosZeroPair::LiteralCons { disc, payload } => {
                    // Emit discriminant to RAX
                    let mov_id = self.alloc_synthetic_id();
                    self.emit_mov_literal_to_reg_with_id(mov_id, abi::RAX, *disc);
                    // Emit payload to RDX
                    match payload {
                        PayloadSrc::Imm(v) => {
                            let mov_id = self.alloc_synthetic_id();
                            self.emit_mov_literal_to_reg_with_id(mov_id, abi::RDX, *v);
                        }
                        PayloadSrc::Reg(r) if *r != abi::RDX => {
                            let mov_id = self.alloc_synthetic_id();
                            self.emit_mov_reg_to_reg_with_id(mov_id, *r, abi::RDX);
                        }
                        PayloadSrc::Reg(_) => {
                            // Already in RDX, no-op
                        }
                    }
                    if let Some(arg_id) = arg_ids.first() {
                        self.state.mark_enum_cons_handled(arg_id.get());
                    }
                }
            }
        }

        // If we have a stashed SysVRegs recipe, splice it now.
        //
        // Two flavours:
        //  * `extern_target: None` (existing behaviour) — the recipe is
        //    self-contained (e.g. rdmsr, cpuid_leaf_ad); we splice the
        //    instructions and return, skipping the CALL / scratch-pop /
        //    postlude that follows.
        //  * `extern_target: Some(sym)` (paideia-as#1305) — the recipe
        //    optionally splices preamble instructions and then rewrites
        //    `target_name` to the extern symbol; we fall through to the
        //    normal CALL emission so scratch-pop and postlude fire
        //    correctly. This is how `Argon2id::derive` reaches the
        //    `paideia_crypto_argon2id_derive` thunk in `paideia-as-crypto`.
        if let Some(recipe) = sysv_recipe {
            // PA-r16-007 (#1066): Handle local labels for SysVRegs recipes too.
            let mangle = |local: &str| {
                format!("__recipe_{}_{}", lambda_node_id.get(), local)
            };
            let label_names: HashSet<&str> = recipe.labels.iter().map(|(n, _)| *n).collect();

            let extern_target = recipe.extern_target.clone();

            for (i, mut inst) in recipe.instructions.into_iter().enumerate() {
                // Rewrite label refs in operands
                for op in inst.operands.iter_mut() {
                    if let Operand::LabelRef { name, .. } = op {
                        if label_names.contains(name.as_str()) {
                            *name = mangle(name);
                        }
                    }
                }

                let iid = IrNodeId::new(lambda_node_id.get() * 16 + 100 + (i as u32) + 1)
                    .expect("stdlib SysVRegs recipe virtual id");

                // Register this instruction's label bindings BEFORE emit_inst
                for (local_name, idx) in &recipe.labels {
                    if *idx == i {
                        self.state.insert_label(mangle(local_name), iid);
                    }
                }

                self.emit_inst(iid, inst);
            }

            match extern_target {
                None => {
                    return; // Skip Call+Ret block (existing SysVRegs behaviour).
                }
                Some(sym) => {
                    // paideia-as#1305: rewrite the CALL target to the extern
                    // symbol and fall through to the normal Call / scratch-pop
                    // / postlude block below. Caller-save preservation, SysV
                    // alignment, and the CALL relocation all go through the
                    // same code path as any other cross-function call.
                    target_name = sym;
                }
            }
        }

        // Emit CALL instruction with fresh ID
        let call_id = if first_emission {
            first_id
        } else {
            self.alloc_synthetic_id()
        };

        let mut call_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        call_operands.push(Operand::SymbolRef {
            name: target_name.clone(),
            addend: 0,
        });

        let call_inst = Instruction {
            mnemonic: Mnemonic::Call,
            operands: call_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };

        self.emit_inst(call_id, call_inst);

        // PAS-DEBT-B4-002 Slice B (paideia-as#1554): release the caller
        // sret slot immediately after the CALL, BEFORE scratch-pop.
        //
        // Emission order symmetry: the sret slot was pushed via
        // `sub rsp, padded_slot` AFTER the scratch pushes (see the
        // prelude block above), so RSP at CALL time sits on the sret
        // slot with scratch saves below. Popping scratch first would
        // read random bytes from the sret buffer, not the saved
        // registers. Release the sret slot first so RSP re-lands on
        // the scratch saves and the existing pop loop can consume
        // them from the right offset.
        //
        // PAS-DEBT-B4-002 Slice C (paideia-as#1554): skipped when the
        // persistent-slot path fired above (no matching `sub rsp` was
        // emitted, so no `add rsp` release is needed — the slot lives
        // in the caller's frame and is released by `mov rsp, rbp` at
        // RET time). `persistent_slot.is_some()` acts as the gate.
        if aggregate_shape.needs_sret() && persistent_slot.is_none() {
            let padded_slot = aggregate_shape.sret_slot_bytes();
            let sret_add_id = self.alloc_synthetic_id();
            let mut add_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RSP));
            add_ops.push(Operand::Imm64(padded_slot as i64));
            let add_inst = Instruction {
                mnemonic: Mnemonic::Add,
                operands: add_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            self.emit_inst(sret_add_id, add_inst);
        }

        // PAS-DEBT-B4-002 Slice C (paideia-as#1554) Piece 4:
        // caller-side register-return read-back. Fires when the
        // callee has a `return_record_layout` with a register-return
        // placement AND this call site has a persistent slot in the
        // caller (so there's a destination buffer to write into).
        //
        // PAS-DEBT-B4-002 Slice D (paideia-as#1554): now active.
        // The `return_record_cons_pass` gate that Slice C set to
        // Memory-only has been lifted; register-return placements
        // (IntSingle, SseSingle, IntPair, IntSse, SseInt, SsePair,
        // XmmSingle) allocate a persistent slot too, and this
        // splice writes RAX / RDX / XMM0 / XMM1 into it right after
        // the CALL. Callers marked `@no_frame` still land in the
        // scalar path because `persistent_slot` was cleared above
        // — `is_caller_no_frame` treats them as "no dest buffer",
        // preserving the Slice B byte-identity for the IntPair
        // fixture.
        if !aggregate_shape.needs_sret() {
            if let Some(slot) = persistent_slot {
                let sym_layout = arena
                    .symbols()
                    .lookup_by_name(&target_name)
                    .and_then(|s| s.return_record_layout.clone());
                if let Some(layout) = sym_layout {
                    let insts: Vec<Instruction> = match callee_abi {
                        CallingConvention::Sysv => {
                            let placement =
                                paideia_as_ir::abi::sysv_return_placement_from_layout(&layout);
                            crate::aggregate_return::sysv_caller_read_return_pair(
                                placement, abi::RBP, slot.rbp_disp,
                            )
                        }
                        CallingConvention::Ms => {
                            let placement =
                                paideia_as_ir::abi::ms_return_placement_from_layout(&layout);
                            crate::aggregate_return::ms_caller_read_return_reg(
                                placement, abi::RBP, slot.rbp_disp,
                            )
                        }
                    };
                    for inst in insts {
                        let iid = self.alloc_synthetic_id();
                        self.emit_inst(iid, inst);
                    }
                }
            }
        }

        // Issue #1163 (corrective): Restore spilled caller-save scratch bindings after CALL,
        // BEFORE MS postlude. This ensures RSP is still pointing to the saved registers
        // when we pop them. After we restore all scratch regs, THEN adjust RSP by MS postlude.
        // Pop in REVERSE order (LIFO)
        for &reg in scratch_save_set.iter().rev() {
            let scratch_restore_id = self.alloc_synthetic_id();

            let mut pop_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            pop_ops.push(Operand::Reg(reg));

            let pop_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands: pop_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };

            self.emit_inst(scratch_restore_id, pop_inst);
        }

        // #1195 / v0.22.0 (#1326 phase 2): Emit combined SysV postlude
        // (matches the combined SysV prelude above).
        if sysv_bump > 0 {
            let sysv_postlude_id = self.alloc_synthetic_id();

            let mut postlude_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            postlude_ops.push(Operand::Reg(abi::RSP));
            postlude_ops.push(Operand::Imm64(sysv_bump as i64));

            let postlude_inst = Instruction {
                mnemonic: Mnemonic::Add,
                operands: postlude_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };

            self.emit_inst(sysv_postlude_id, postlude_inst);
        }

        // Emit MS x64 postlude: add rsp, ms_bump (dynamic, matches prelude)
        if callee_abi == CallingConvention::Ms {
            let ms_postlude_id = self.alloc_synthetic_id();

            let mut postlude_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            postlude_ops.push(Operand::Reg(abi::RSP));
            postlude_ops.push(Operand::Imm64(ms_bump as i64));

            let postlude_inst = Instruction {
                mnemonic: Mnemonic::Add,
                operands: postlude_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };

            self.emit_inst(ms_postlude_id, postlude_inst);
        }

        // Emit caller-side bridge postlude (pop R14, R15) if crossing paideia→MS/SysV
        self.emit_bridge_postlude(bridge_saves);
    }

    /// v0.21-001 (#1277): Emit `mov qword ptr [rsp + disp], src_reg` for an
    /// MS-x64 stack-passed argument. The 64-bit generic Mov form encodes as
    /// `48 89 <ModRM> <SIB> [disp8|disp32]` (5 or 8 bytes depending on disp).
    ///
    /// `disp` is the byte offset from RSP at the moment of the store — i.e.
    /// AFTER the `sub rsp, ms_bump` prelude and any scratch pushes have run.
    /// See the caller's stack-layout calculation for how `disp` is chosen so
    /// the callee will see the value at `[rsp + 40 + 8*(idx-4)]` after CALL.
    fn emit_mov_stack_slot_reg(&mut self, inst_id: IrNodeId, disp: i32, src_reg: RegId) {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::MemSib {
            base: abi::RSP,
            index: None,
            scale: Scale::X1,
            disp,
        });
        ops.push(Operand::Reg(src_reg));

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        self.emit_inst(inst_id, inst);
    }

    /// v0.21-001 (#1277): Emit `mov qword ptr [rsp + disp], imm` for an MS-x64
    /// stack-passed literal argument. The encoder narrows to `48 C7` (rm64,
    /// imm32-sign-extended, 8 bytes) when the value fits; a value outside the
    /// i32 sign-extended range would trip the encoder's Unsupported error
    /// (see encode_instruction.rs — the generic-Mov MemSib+Imm64 arm), which
    /// surfaces as a build failure rather than a silent miscompile.
    fn emit_mov_stack_slot_imm(&mut self, inst_id: IrNodeId, disp: i32, value: i64) {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::MemSib {
            base: abi::RSP,
            index: None,
            scale: Scale::X1,
            disp,
        });
        ops.push(Operand::Imm64(value));

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        self.emit_inst(inst_id, inst);
    }

    /// Emit RET instruction after a call (or standalone for statement-position calls).
    ///
    /// Issue #1088: For statement-position calls (call expressions whose result is discarded),
    /// emit only the RET, not the full function-call sequence.
    ///
    /// Issue #1165: RET uses alloc_synthetic_id (identity-only post-#1140).
    fn emit_ret_after_call(&mut self, _lambda_node_id: IrNodeId, _callee_abi: CallingConvention, arena: &IrArena) {
        let ret_id = self.alloc_synthetic_id();
        self.emit_ret(ret_id, arena);
    }

    /// Phase 7 m1-003: Emit inter-function call.
    ///
    /// PA7-006: Handles 0-6 argument calls to other functions:
    /// - 0-arg call: `call target; ret` (6 bytes total)
    /// - 1-arg call: `mov rdi, arg0; call target; ret` (3+5+1 bytes)
    /// - 2-arg call: `mov rdi, arg0; mov rsi, arg1; call target; ret` (3+3+5+1 bytes)
    /// - 3-6 arg calls: extend to RDX, RCX, R8, R9
    ///
    /// Supports arg sources: immediate literals, local-binding via LocalBindingTable,
    /// symbol refs to globals. > 6 args rejected with EncodeError::Unsupported.
    pub(crate) fn emit_function_call(
        &mut self,
        lambda_node_id: IrNodeId,
        target_name: String,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        self.emit_function_call_with_app(
            lambda_node_id,
            None,
            target_name,
            arg_ids,
            arena,
        );
    }

    /// PAS-DEBT-B4-002 Slice C (paideia-as#1554): explicit-App-id
    /// variant of `emit_function_call`. Callers that know the App
    /// node id of this call site (e.g. `emit_visit_lambda`'s
    /// `IrKind::App` tail-call arm) pass it here so the caller-side
    /// sret path can consult `CallerSretSlotTable` for a persistent
    /// slot. Callers that don't yet track the App id use
    /// `emit_function_call` (which forwards `None` and preserves
    /// Slice B fallback behavior).
    pub(crate) fn emit_function_call_with_app(
        &mut self,
        lambda_node_id: IrNodeId,
        app_id: Option<IrNodeId>,
        target_name: String,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        // Determine caller and callee ABIs
        // Use lambda_abi_option to distinguish unannotated (None) from explicitly annotated (Some)
        let caller_abi = self.state.lambda_abi_option(lambda_node_id.get());
        let callee_abi = arena.symbols().lookup_by_name(&target_name)
            .and_then(|s| s.abi)
            .unwrap_or(CallingConvention::Sysv);
        self.emit_call_args_and_call(
            lambda_node_id, target_name, arg_ids, arena, caller_abi, app_id,
        );
        self.emit_ret_after_call(lambda_node_id, callee_abi, arena);
    }

    /// Phase 7 m4-003: Emit call statement (expression-statement form).
    ///
    /// Issue #1088: Route call expressions inside unsafe blocks through the emit pipeline.
    /// Issue #1183: Emits ONLY arguments + CALL. The terminal RET (when the
    /// enclosing lambda body is an `Action` block) is the responsibility of
    /// `emit_block_body` (see `emit_block_body.rs:562-571`). When the enclosing
    /// lambda body is an `Unsafe` block, the RET must be emitted explicitly by
    /// the author (matching sibling `pa_r17_unsafe_*` fixtures).
    pub(crate) fn emit_call_stmt(
        &mut self,
        lambda_node_id: IrNodeId,
        target_name: String,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        let caller_abi = self.state.lambda_abi_option(lambda_node_id.get());
        self.emit_call_args_and_call(
            lambda_node_id, target_name, arg_ids, arena, caller_abi, None,
        );
    }

    /// #1136: Emit an expression-position call (args + CALL, no RET).
    /// The return value lands in RAX per both SysV and MS integer-return conventions;
    /// the caller consumes it (e.g. by writing it into a module symbol).
    pub(crate) fn emit_call_expr(
        &mut self,
        lambda_node_id: IrNodeId,
        target_name: String,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        // #1260: Validate the callee is defined before emitting the call.
        //
        // Prior to this check, `let x = does_not_exist(1, 2)` compiled
        // silently: emit_call_args_and_call happily wrote a SymbolRef
        // relocation for an unresolvable name, and the failure surfaced
        // only at link time (or as a load-time SIGSEGV in the produced
        // binary). That defeated the whole compile-time-safety story.
        //
        // Paideia has no cross-module import mechanism — every callable
        // identifier reaching emit_call_expr must be resolvable to one
        // of the sources below within this compilation unit:
        //   1. A module symbol (arena.symbols() — populated by the
        //      emit_walker Let pre-pass for every top-level let/lambda).
        //   2. A stdlib trait recipe (Trait::method routed through
        //      stdlib_lowering::lower_stdlib_method).
        //   3. Local closure / function-pointer bindings — those are
        //      dispatched by emit_closure_call BEFORE reaching this
        //      entry (see emit_block_body.rs ~line 821), so any name
        //      hitting this function that fails (1) and (2) is a real
        //      undefined identifier.
        //
        // Operators are filtered out at the emit_block_body call site
        // (is_operator_callee), and enum constructors (Foo::Variant) go
        // through visit_enum_cons rather than emit_call_expr.
        let is_stdlib_recipe = resolve_stdlib_trait_method(&target_name)
            .and_then(|(t, m)| {
                crate::stdlib_lowering::lower_stdlib_method(
                    &t,
                    &m,
                    self.current_mode(),
                    arg_ids,
                    arena,
                )
                .map(|_res| ())
            })
            .is_some();
        let is_module_symbol =
            arena.symbols().lookup_by_name(&target_name).is_some();
        if !is_stdlib_recipe && !is_module_symbol {
            self.push_typed_diag(
                t0553_code(),
                format!("undefined identifier: {}", &target_name),
            );
            // Return without emitting the call. Downstream code that
            // depended on RAX carrying a return value will observe
            // whatever RAX held on entry — the diagnostic makes the
            // build fail, so no correctness invariant on RAX matters.
            return;
        }

        let caller_abi = self.state.lambda_abi_option(lambda_node_id.get());
        self.emit_call_args_and_call(
            lambda_node_id, target_name, arg_ids, arena, caller_abi, None,
        );
    }
}
