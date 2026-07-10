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

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_elaborator::LocalBindingTable;
use paideia_as_ir::{IrNodeId, Operand, SymbolTable, instruction::InstructionSideTable};

#[cfg(test)]
use paideia_as_ir::InstrMode;

/// Build the canonical T0528 diagnostic for an unresolved local binding.
fn make_t0528(name: &str) -> Diagnostic {
    let code = DiagnosticCode::new(Category::T, Severity::Error, 528)
        .expect("T0528 is registered in the diagnostic catalog");
    Diagnostic::error(code)
        .message(format!("unresolved local binding '{}'", name))
        .finish()
}

/// Build the canonical T0531 diagnostic for a name found only in the module
/// SymbolTable (PA10-005 §3.5).
fn make_t0531(name: &str) -> Diagnostic {
    let code = DiagnosticCode::new(Category::T, Severity::Error, 531)
        .expect("T0531 is registered in the diagnostic catalog");
    Diagnostic::error(code)
        .message(format!(
            "local binding not found; found '{}' in module SymbolTable but it is module-scoped",
            name
        ))
        .finish()
}

/// Resolve all Operand::Var operands in the instruction table to Operand::Reg.
///
/// Walks every instruction in the side table; for each Var operand, performs
/// a local binding lookup. On success, rewrites to Reg. On failure, checks the
/// module SymbolTable; if found there, pushes a T0531 diagnostic; if not found
/// anywhere, pushes a T0528 diagnostic and leaves Var in place.
///
/// PA10-005 §3.4-3.5: `symbol_table` parameter enables T0531 diagnostic emission.
///
/// Step 2 of the v0.17 refactor plan (2026-07-07): the diagnostics parameter
/// is now `&mut Vec<Diagnostic>` rather than `&mut Vec<String>`, so callers
/// no longer need to string-parse `T####:` prefixes to route into the
/// canonical `DiagnosticSink`. Retires the `700` catch-all fallback that
/// used to fabricate an unregistered code on parse failure.
///
/// # Arguments
///
/// * `instructions` - The instruction side-table to mutate in place.
/// * `bindings` - The local binding table (populated by EmitWalker).
/// * `symbol_table` - Owned SymbolTable clone (for distinguishing module-scope symbols).
/// * `diagnostics` - Mutable typed diagnostics vec; T0528/T0531 entries are pushed here.
pub(crate) fn resolve_var_operands(
    instructions: &mut InstructionSideTable,
    bindings: &LocalBindingTable,
    symbol_table: Option<SymbolTable>,
    diagnostics: &mut Vec<Diagnostic>,
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
                                diagnostics.push(make_t0531(name));
                                // Leave Var in place for downstream error recovery.
                                continue;
                            }
                        }
                        diagnostics.push(make_t0528(name));
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
            mode: InstrMode::default(),
            emission_order: 0,
};
        instructions.insert(node_id, inst);

        // Resolve variables.
        let mut diags: Vec<Diagnostic> = Vec::new();
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
            mode: InstrMode::default(),
            emission_order: 0,
};
        instructions.insert(node_id, inst);

        // Resolve variables.
        let mut diags: Vec<Diagnostic> = Vec::new();
        resolve_var_operands(&mut instructions, &bindings, None, &mut diags);

        // Verify a diagnostic was emitted and the Var was left in place.
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.code().category(), Category::T);
        assert_eq!(d.code().number(), 528);
        assert!(d.message().contains("undefined_var"));
        let unresolved = instructions.get(node_id).unwrap();
        assert!(matches!(&unresolved.operands[0], Operand::Var { .. }));
    }
}
