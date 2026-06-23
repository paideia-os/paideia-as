//! Resolve Operand::Var to Operand::Reg via local binding lookup.
//!
//! Phase 7 m2-003: After UnsafeWalker produces Operand::Var entries in the
//! InstructionSideTable, this pass walks every instruction and resolves variable
//! references to their assigned scratch registers. On successful lookup, the Var
//! is replaced with Reg; on missing bindings, T0528 is emitted and the Var is left
//! in place for downstream error reporting.
//!
//! PA10-005 §3.4-3.5: Thread SymbolTable through to distinguish module-scope symbols
//! (emit T0531) from unresolved bindings (emit T0528).

use paideia_as_elaborator::LocalBindingTable;
use paideia_as_ir::{IrNodeId, Operand, SymbolTable, instruction::InstructionSideTable};

/// Resolve all Operand::Var operands in the instruction table to Operand::Reg.
///
/// Walks every instruction in the side table; for each Var operand, performs
/// a local binding lookup. On success, rewrites to Reg. On failure, checks the
/// module SymbolTable; if found there, emits T0531; if not found anywhere, emits T0528
/// and leaves Var in place.
///
/// PA10-005 §3.4-3.5: `symbol_table` parameter enables T0531 diagnostic emission.
///
/// # Arguments
///
/// * `instructions` - The instruction side-table to mutate in place.
/// * `bindings` - The local binding table (populated by EmitWalker).
/// * `symbol_table` - Owned SymbolTable clone (for distinguishing module-scope symbols).
/// * `diagnostics` - Mutable diagnostics vec; T0528/T0531 entries are pushed here.
pub(crate) fn resolve_var_operands(
    instructions: &mut InstructionSideTable,
    bindings: &LocalBindingTable,
    symbol_table: Option<SymbolTable>,
    diagnostics: &mut Vec<String>,
) {
    // Collect all node IDs to avoid borrow conflicts.
    let node_ids: Vec<IrNodeId> = instructions.entries().keys().copied().collect();

    for node_id in node_ids {
        if let Some(inst) = instructions.get_mut(node_id) {
            // Walk every operand in the instruction and resolve Var → Reg.
            for operand in inst.operands.iter_mut() {
                if let Operand::Var { name } = operand {
                    if let Some(reg) = bindings.get(name) {
                        // Found binding: rewrite Var → Reg.
                        *operand = Operand::Reg(reg);
                    } else {
                        // PA10-005 §3.5: Binding not found in local table;
                        // check module SymbolTable for module-scoped symbol.
                        if let Some(ref sym_table) = symbol_table {
                            if sym_table.lookup_by_name(name).is_some() {
                                // Found in module SymbolTable: emit T0531.
                                let msg = format!(
                                    "T0531: local binding not found; checking module SymbolTable (found '{}' but it is module-scoped)",
                                    name
                                );
                                diagnostics.push(msg);
                                // Leave Var in place for downstream error recovery.
                                continue;
                            }
                        }
                        // Not found anywhere: emit T0528.
                        let msg = format!("T0528: unresolved local binding '{}'", name);
                        diagnostics.push(msg);
                        // Leave Var in place for downstream error recovery.
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{Mnemonic, RegId, instruction::Instruction};
    use smallvec::SmallVec;

    #[test]
    fn resolve_var_operands_rewrites_var_to_reg() {
        // Set up a binding table with one binding.
        let mut bindings = LocalBindingTable::new();
        bindings.insert("x".to_string(), RegId(0)); // x → rax

        // Create an instruction with a Var operand.
        let mut instructions = InstructionSideTable::new();
        let node_id = IrNodeId::new(1).unwrap();
        let mut operands = SmallVec::new();
        operands.push(Operand::Var {
            name: "x".to_string(),
        });
        operands.push(Operand::Reg(RegId(1))); // rcx
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        instructions.insert(node_id, inst);

        // Resolve variables.
        let mut diags = Vec::new();
        resolve_var_operands(&mut instructions, &bindings, None, &mut diags);

        // Verify the Var was rewritten to Reg and no diagnostics were emitted.
        let resolved = instructions.get(node_id).unwrap();
        assert!(matches!(&resolved.operands[0], Operand::Reg(RegId(0))));
        assert!(diags.is_empty());
    }

    #[test]
    fn resolve_var_operands_emits_t0528_for_unknown_name() {
        // Create a binding table without the binding.
        let bindings = LocalBindingTable::new();

        // Create an instruction with an unresolved Var operand.
        let mut instructions = InstructionSideTable::new();
        let node_id = IrNodeId::new(1).unwrap();
        let mut operands = SmallVec::new();
        operands.push(Operand::Var {
            name: "undefined_var".to_string(),
        });
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        instructions.insert(node_id, inst);

        // Resolve variables.
        let mut diags = Vec::new();
        resolve_var_operands(&mut instructions, &bindings, None, &mut diags);

        // Verify a diagnostic was emitted and the Var was left in place.
        assert_eq!(diags.len(), 1);
        assert!(diags[0].contains("T0528"));
        assert!(diags[0].contains("undefined_var"));
        let unresolved = instructions.get(node_id).unwrap();
        assert!(matches!(&unresolved.operands[0], Operand::Var { .. }));
    }
}
