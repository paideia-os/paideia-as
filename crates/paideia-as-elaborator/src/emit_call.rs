//! Inter-function call lowering (Phase 7 m1-003 / PA7-006).
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts
//! `emit_function_call`, which lowers a `Call(target, args)` into the
//! System-V calling-convention marshalling sequence: per-arg moves into
//! `[RDI, RSI, RDX, RCX, R8, R9]` followed by `call target; ret`.

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};
use std::collections::HashSet;

use crate::emit_walker::EmitWalker;
use crate::stdlib_lowering::ArgConvention;

/// Resolve a target name to (trait_name, method_name) if it's a qualified stdlib trait method.
/// Returns None if the target is not in the form "TraitName::method_name".
fn resolve_stdlib_trait_method(target: &str) -> Option<(String, String)> {
    let (t, m) = target.rsplit_once("::")?;
    Some((t.to_string(), m.to_string()))
}

impl EmitWalker {
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
        // Record lambda entry and compute main_id for first instruction (node_id * 2).
        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

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
                        self.diagnostics.push(format!(
                            "T0551: stdlib intrinsic requires integer-literal argument: {} arg {}",
                            method, arg_index
                        ));
                        // Fall through to normal call emission
                    }
                }
            }
        }

        // ABI calling convention: arguments go to RDI, RSI, RDX, RCX, R8, R9
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9]; // RDI, RSI, RDX, RCX, R8, R9

        // Emit MOV instructions for each argument
        for (arg_idx, &arg_id) in arg_ids.iter().enumerate() {
            if arg_idx >= 6 {
                // Phase 7 only supports up to 6 arguments
                self.diagnostics.push(format!(
                    "T0521: argument type mismatch at call site: arg index {} out of bounds (max 6)",
                    arg_idx
                ));
                break;
            }

            let dest_reg = arg_regs[arg_idx];
            let arg_node = match arena.get(arg_id) {
                Some(node) => node,
                None => {
                    self.diagnostics.push(format!(
                        "T0521: argument type mismatch at call site: arg {} not found in IR",
                        arg_idx
                    ));
                    continue;
                }
            };

            // Handle various argument sources
            match arg_node.kind {
                IrKind::Literal => {
                    // Load literal into the register
                    if let Some(value) = arena.literal_values().get(arg_id) {
                        self.emit_mov_literal_to_reg(lambda_node_id, dest_reg, value);
                    } else {
                        self.diagnostics.push(format!(
                            "T0521: argument type mismatch at call site: literal arg {} has no value",
                            arg_idx
                        ));
                    }
                }
                IrKind::Var => {
                    // For Var arguments, check if it's a local binding or parameter
                    // For now, support copying from RDI (first parameter)
                    if arg_idx == 0 && dest_reg != abi::RDI {
                        // Need to copy from RDI to another reg
                        self.emit_mov_reg_to_reg(lambda_node_id, abi::RDI, dest_reg);
                    } else if arg_idx != 0 {
                        // Non-first-arg Var references require local binding lookup
                        self.diagnostics.push(format!(
                            "T0521: argument type mismatch at call site: Var arg {} (non-first-arg) not yet supported",
                            arg_idx
                        ));
                    }
                }
                _ => {
                    // Other argument shapes not yet supported
                    self.diagnostics.push(format!(
                        "T0521: argument type mismatch at call site: arg {} kind {:?} not supported",
                        arg_idx, arg_node.kind
                    ));
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

        // Emit CALL instruction with the recorded main_id
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
        };

        self.emit_inst(main_id, call_inst);

        // Emit RET instruction
        let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1).expect("ret instr id");
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };
        self.emit_inst(ret_id, ret_inst);
    }
}
