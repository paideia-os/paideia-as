//! Inter-function call lowering (Phase 7 m1-003 / PA7-006).
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts
//! `emit_function_call`, which lowers a `Call(target, args)` into the
//! System-V calling-convention marshalling sequence: per-arg moves into
//! `[RDI, RSI, RDX, RCX, R8, R9]` followed by `call target; ret`.

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::symbol::SymbolKind;
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};
use std::collections::HashSet;

use crate::emit_walker::EmitWalker;
use crate::stdlib_lowering::ArgConvention;

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

impl EmitWalker {
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
    ) {
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
        let ms_bump: u32 = if callee_abi == CallingConvention::Ms {
            if scratch_save_set.len() % 2 == 1 {
                abi::MS_CALL_STACK_BUMP + abi::MS_CALL_STACK_BUMP_ODD_PAD
            } else {
                abi::MS_CALL_STACK_BUMP
            }
        } else {
            0
        };

        // #1195: Compute dynamic SysV alignment pad based on scratch-save parity.
        // Only for EXPLICIT SysV ABI calls (callee_abi_option == Some(Sysv)) with bridge saves.
        // When paideia→SysV cross-call and scratch count is even, emit 8-byte pad before
        // scratch saves to restore RSP ≡ 0 mod 16 at CALL.
        let sysv_bump: u32 = if callee_abi_option == Some(CallingConvention::Sysv) && !bridge_saves.is_empty() {
            if scratch_save_set.len() % 2 == 0 {
                abi::SYSV_CALL_ALIGN_PAD
            } else {
                0
            }
        } else {
            0
        };

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

        // #1195: Emit SysV alignment pad: sub rsp, 8 (for paideia→SysV with even scratch count)
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
            if arg_idx >= arg_regs.len() {
                // Emit distinct T0521 message for MS branch
                let error_msg = if callee_abi == CallingConvention::Ms {
                    format!("MS x64 ABI: max 4 arguments supported (arg {} out of bounds)", arg_idx)
                } else {
                    format!("SysV ABI: max 6 arguments supported (arg {} out of bounds)", arg_idx)
                };
                self.push_typed_diag(t0521_code(), error_msg);
                break;
            }

            let dest_reg = arg_regs[arg_idx];
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

        // If we have a stashed SysVRegs recipe, splice it now and return
        // (skipping Call+Ret emission).
        if let Some(recipe) = sysv_recipe {
            // PA-r16-007 (#1066): Handle local labels for SysVRegs recipes too.
            let mangle = |local: &str| {
                format!("__recipe_{}_{}", lambda_node_id.get(), local)
            };
            let label_names: HashSet<&str> = recipe.labels.iter().map(|(n, _)| *n).collect();

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
            return;  // Skip Call+Ret block
        }

        // Emit CALL instruction with fresh ID
        let call_id = if first_emission {
            first_id
        } else {
            self.alloc_synthetic_id()
        };

        let mut call_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        call_operands.push(Operand::SymbolRef {
            name: target_name,
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

        // #1195: Emit SysV alignment postlude: add rsp, 8 (matches SysV prelude)
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

    /// Emit RET instruction after a call (or standalone for statement-position calls).
    ///
    /// Issue #1088: For statement-position calls (call expressions whose result is discarded),
    /// emit only the RET, not the full function-call sequence.
    ///
    /// Issue #1165: RET uses alloc_synthetic_id (identity-only post-#1140).
    fn emit_ret_after_call(&mut self, _lambda_node_id: IrNodeId, _callee_abi: CallingConvention) {
        let ret_id = self.alloc_synthetic_id();
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        self.emit_inst(ret_id, ret_inst);
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
        // Determine caller and callee ABIs
        // Use lambda_abi_option to distinguish unannotated (None) from explicitly annotated (Some)
        let caller_abi = self.state.lambda_abi_option(lambda_node_id.get());
        let callee_abi = arena.symbols().lookup_by_name(&target_name)
            .and_then(|s| s.abi)
            .unwrap_or(CallingConvention::Sysv);
        self.emit_call_args_and_call(lambda_node_id, target_name, arg_ids, arena, caller_abi);
        self.emit_ret_after_call(lambda_node_id, callee_abi);
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
        self.emit_call_args_and_call(lambda_node_id, target_name, arg_ids, arena, caller_abi);
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
        let caller_abi = self.state.lambda_abi_option(lambda_node_id.get());
        self.emit_call_args_and_call(lambda_node_id, target_name, arg_ids, arena, caller_abi);
    }
}
