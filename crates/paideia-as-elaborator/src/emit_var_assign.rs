//! Variable assignment lowering (Pattern 5: `x = value`).
//!
//! Extracted from `emit_walker.rs` during Phase 17 m6-c. Hosts the emit
//! handler for bare variable assignment (`x = value`) where LHS is a bare Ident
//! or single-segment ExprPath.
//!
//! Pattern 5 assignment routing:
//! - LHS can be a module-level symbol or a local binding
//! - RHS can be a Var (local binding), Literal, or module-level symbol
//! - Module-level writes emit via `emit_mem_write_via_rip_sym`
//! - Local bindings use reg-to-reg moves via `Operand::Var`
//! - Unsupported shapes fire T0518 diagnostic

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec};
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};

use crate::emit_walker::EmitWalker;

/// Helper to construct T0518 diagnostic code.
fn t0518_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 518)
        .expect("T0518 is within valid T range")
}

impl EmitWalker {
    /// Phase 17 m6-c: Emit variable assignment lowering for Pattern 5 (bare variable/path).
    ///
    /// Handles: `x = value` where LHS is bare Ident or single-segment ExprPath.
    ///
    /// Store IR children layout: `[lhs, op, rhs]`
    /// - children[0] = Var node (LHS variable)
    /// - children[1] = operator (unused)
    /// - children[2] = RHS expression (Var, Literal, or module symbol)
    ///
    /// Branches on LHS:
    /// - Module-level symbol: emit via `emit_mem_write_via_rip_sym`
    /// - Local binding: emit reg-to-reg move via `Operand::Var`
    /// - Unresolved: fire T0518
    ///
    /// Branches on RHS:
    /// - Var: resolve via `Operand::Var` or register lookup
    /// - Literal: emit `mov RAX, imm64` then store
    /// - Unsupported: fire T0518
    pub(crate) fn visit_var_assign(&mut self, store_id: IrNodeId, arena: &IrArena) {
        let children = arena.children(store_id);
        if children.len() != 3 {
            self.push_typed_diag(
                t0518_code(),
                format!(
                    "Store node {} has {} children; expected 3 for variable assignment",
                    store_id.get(),
                    children.len()
                ),
            );
            return;
        }

        let lhs_id = children[0];
        let _op_id = children[1];
        let rhs_id = children[2];

        // Extract LHS variable name
        let lhs_name = match arena.binding_names().get(lhs_id) {
            Some(name) => name.to_string(),
            None => {
                self.push_typed_diag(
                    t0518_code(),
                    format!(
                        "LHS variable {} has no binding name (unresolved identifier)",
                        lhs_id.get()
                    ),
                );
                return;
            }
        };

        // Check if LHS is a module-level symbol or local binding
        let is_module_symbol = !self.state.local_bindings.contains(&lhs_name)
            && arena.symbols().lookup_by_name(&lhs_name).is_some();

        // Get RHS node for dispatch
        let rhs_node = match arena.get(rhs_id) {
            Some(node) => node,
            None => {
                self.push_typed_diag(
                    t0518_code(),
                    format!("RHS {} not found in arena", rhs_id.get()),
                );
                return;
            }
        };

        // Dispatch on RHS shape
        match rhs_node.kind {
            IrKind::Var => {
                // RHS is a variable — resolve its register or module-level location
                let rhs_name = match arena.binding_names().get(rhs_id) {
                    Some(name) => name.to_string(),
                    None => {
                        self.push_typed_diag(
                            t0518_code(),
                            format!(
                                "RHS variable {} has no binding name (unresolved)",
                                rhs_id.get()
                            ),
                        );
                        return;
                    }
                };

                if is_module_symbol {
                    // Module-level LHS: emit RIP-relative write
                    // Use the existing helper which dispatches on size
                    if let Some(rhs_reg) = self.state.local_bindings.get(&rhs_name) {
                        // RHS is a local binding: use as source register
                        self.emit_mem_write_via_rip_sym(store_id, rhs_reg, lhs_name.clone(), 0, 8, false);
                    } else {
                        // RHS is not a local binding (must be module-level or unresolved)
                        // For now, defer module-level RHS support
                        self.push_typed_diag(
                            t0518_code(),
                            format!(
                                "module-level {} = <{} (module-level)> not yet supported; deferred to future phase",
                                lhs_name, rhs_name
                            ),
                        );
                    }
                } else {
                    // Local binding LHS: emit reg-to-reg move
                    // Resolve RHS register from local_bindings
                    if let Some(rhs_reg) = self.state.local_bindings.get(&rhs_name) {
                        if let Some(lhs_reg) = self.state.local_bindings.get(&lhs_name) {
                            // Both are local: emit mov lhs_reg, rhs_reg
                            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            operands.push(Operand::Reg(lhs_reg));
                            operands.push(Operand::Reg(rhs_reg));

                            let inst = Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            };

                            self.emit_inst(store_id, inst);
                        } else {
                            // LHS not in local_bindings: unresolved
                            self.push_typed_diag(
                                t0518_code(),
                                format!(
                                    "LHS variable {} not resolved (not a module symbol or local binding)",
                                    lhs_name
                                ),
                            );
                        }
                    } else {
                        // RHS not in local_bindings: unresolved
                        self.push_typed_diag(
                            t0518_code(),
                            format!(
                                "RHS variable {} not resolved (not a local binding)",
                                rhs_name
                            ),
                        );
                    }
                }
            }
            IrKind::Literal => {
                // RHS is a literal — extract value and emit assignment
                if let Some(value) = arena.literal_values().get(rhs_id) {
                    if is_module_symbol {
                        // Module-level LHS with literal RHS: emit RIP-relative write
                        // For now, fire T0518 for module-level + literal (deferred)
                        self.push_typed_diag(
                            t0518_code(),
                            format!(
                                "module-level {} = <literal> not yet supported; deferred to future phase",
                                lhs_name
                            ),
                        );
                    } else {
                        // Local binding LHS with literal RHS: emit mov reg, imm
                        if let Some(lhs_reg) = self.state.local_bindings.get(&lhs_name) {
                            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            operands.push(Operand::Reg(lhs_reg));
                            operands.push(Operand::Imm64(value));

                            let inst = Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            };

                            self.emit_inst(store_id, inst);
                        }
                    }
                } else {
                    self.push_typed_diag(
                        t0518_code(),
                        format!("Literal {} has no value", rhs_id.get()),
                    );
                }
            }
            _ => {
                // Unsupported RHS shape
                self.push_typed_diag(
                    t0518_code(),
                    format!(
                        "unsupported RHS shape {:?} in variable assignment {} = {}",
                        rhs_node.kind, lhs_id.get(), rhs_id.get()
                    ),
                );
            }
        }
    }
}
