//! Unit tests for variable assignment emit handler (Pattern 5).
//! SILENT-DRAIN AUDIT: this file uses push_typed_diag exclusively.

#[cfg(test)]
mod tests {
    use paideia_as_ir::instruction::{IntWidth, Mnemonic, Operand};
    use paideia_as_ir::{IrArena, IrKind};
    use paideia_as_ir::symbol::{Symbol, SymbolKind};
    use paideia_as_diagnostics::{FileId, Span};
    use crate::emit_walker::EmitWalker;
    use paideia_as_ir::abi;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// Test 1: module-level u64 symbol assigned from local binding register.
    ///
    /// Scenario: let y = 42; x = y (where x is module-level symbol)
    /// Expected: emit MovSized{W64} [MemRipRelSym{x, 0}, RAX] (after y is in RAX)
    #[test]
    fn visit_var_assign_module_sym_u64_from_reg() {
        let mut arena = IrArena::new();

        // Create Let binding: let y = 42 (will be assigned to RAX during emit)
        let rhs_lit_id = arena.alloc(IrKind::Literal, span());
        arena.literal_values_mut().insert(rhs_lit_id, 42);
        let let_y_id = arena.alloc_with_children(IrKind::Let, span(), [rhs_lit_id]);
        arena.binding_names_mut().insert(let_y_id, "y".to_string());

        // Allocate LHS: Var node for module-level symbol "x"
        let lhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(lhs_id, "x".to_string());

        // Register "x" as a module-level symbol
        let sym_x = Symbol::new("x".to_string(), SymbolKind::Object, lhs_id);
        arena.symbols_mut().insert(sym_x);

        // Allocate RHS: Var node for local binding "y"
        let rhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(rhs_id, "y".to_string());

        // Allocate operator (unused)
        let op_id = arena.alloc(IrKind::Placeholder, span());

        // Allocate Store: [lhs, op, rhs]
        let store_id = arena.alloc_with_children(IrKind::Store, span(), [lhs_id, op_id, rhs_id]);

        // Create Action containing: let y = 42; x = y
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [let_y_id, store_id]);

        // Create Lambda with Action body
        let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.walk(&mut arena);

        // Verify: at least one MovSized instruction was emitted
        let mut found_rip_rel_mov = false;
        let mut found_mov_rax_42 = false;
        for (_, inst) in walker.state().instructions.entries().iter() {
            match &inst.mnemonic {
                Mnemonic::Mov => {
                    // Check for mov RAX, 42
                    if inst.operands.len() == 2 {
                        if let (Operand::Reg(dst), Operand::Imm64(val)) = (&inst.operands[0], &inst.operands[1]) {
                            if *dst == abi::RAX && *val == 42 {
                                found_mov_rax_42 = true;
                            }
                        }
                    }
                }
                Mnemonic::MovSized { width: IntWidth::W64 } => {
                    // Check if this has MemRipRelSym operand
                    if inst.operands.len() >= 2 {
                        if let (Operand::MemRipRelSym { name, addend }, Operand::Reg(src)) = (&inst.operands[0], &inst.operands[1]) {
                            if name == "x" && *addend == 0 && *src == abi::RAX {
                                found_rip_rel_mov = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        assert!(found_mov_rax_42, "Should emit mov RAX, 42 for Let binding");
        assert!(found_rip_rel_mov, "Should emit MovSized W64 [MemRipRelSym(x), RAX]");
    }

    /// Test 2: module-level u64 symbol assigned from literal.
    ///
    /// Scenario: x = 42 (where x is module-level symbol)
    /// Expected: emit deferred T0518 (module-level LHS + literal RHS not yet supported)
    #[test]
    fn visit_var_assign_module_sym_u64_from_literal() {
        let mut arena = IrArena::new();

        // Allocate LHS: Var node for module-level symbol "x"
        let lhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(lhs_id, "x".to_string());

        // Register "x" as a module-level symbol
        let sym_x = Symbol::new("x".to_string(), SymbolKind::Object, lhs_id);
        arena.symbols_mut().insert(sym_x);

        // Allocate RHS: Literal(42)
        let rhs_id = arena.alloc(IrKind::Literal, span());
        arena.literal_values_mut().insert(rhs_id, 42);

        // Allocate operator (unused)
        let op_id = arena.alloc(IrKind::Placeholder, span());

        // Allocate Store: [lhs, op, rhs]
        let store_id = arena.alloc_with_children(IrKind::Store, span(), [lhs_id, op_id, rhs_id]);

        // Create Action containing the Store
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [store_id]);

        // Create Lambda with Action body
        let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.walk(&mut arena);

        // Verify: T0518 diagnostic for deferred module-level + literal case
        let has_t0518 = walker
            .take_typed_diagnostics()
            .iter()
            .any(|d| d.code().to_string().contains("0518"));
        assert!(has_t0518, "Should emit T0518 for deferred module-level + literal case");

        // Verify: legacy diagnostics are empty (SILENT-DRAIN)
        assert_eq!(
            walker.diagnostics().len(),
            0,
            "Legacy diagnostics should be empty"
        );
    }

    /// Test 3: local binding assigned from another local binding (reg-to-reg move).
    ///
    /// Scenario: let x = 10; let y = 20; x = y
    /// Expected: emit mov (x_reg, y_reg) for register-to-register
    #[test]
    fn visit_var_assign_local_binding_reg_to_reg() {
        let mut arena = IrArena::new();

        // Create Let binding: let x = 10
        let x_lit_id = arena.alloc(IrKind::Literal, span());
        arena.literal_values_mut().insert(x_lit_id, 10);
        let let_x_id = arena.alloc_with_children(IrKind::Let, span(), [x_lit_id]);
        arena.binding_names_mut().insert(let_x_id, "x".to_string());

        // Create Let binding: let y = 20
        let y_lit_id = arena.alloc(IrKind::Literal, span());
        arena.literal_values_mut().insert(y_lit_id, 20);
        let let_y_id = arena.alloc_with_children(IrKind::Let, span(), [y_lit_id]);
        arena.binding_names_mut().insert(let_y_id, "y".to_string());

        // Allocate LHS: Var node for local binding "x"
        let lhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(lhs_id, "x".to_string());

        // Allocate RHS: Var node for local binding "y"
        let rhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(rhs_id, "y".to_string());

        // Allocate operator (unused)
        let op_id = arena.alloc(IrKind::Placeholder, span());

        // Allocate Store: [lhs, op, rhs]
        let store_id = arena.alloc_with_children(IrKind::Store, span(), [lhs_id, op_id, rhs_id]);

        // Create Action containing: let x = 10; let y = 20; x = y
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [let_x_id, let_y_id, store_id]);

        // Create Lambda with Action body
        let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.walk(&mut arena);

        // Verify: local_bindings contains both x and y
        assert!(walker.state().local_bindings.contains("x"), "x should be in local_bindings");
        assert!(walker.state().local_bindings.contains("y"), "y should be in local_bindings");

        // Verify: at least one Mov instruction for x = y
        let mut found_reg_to_reg_mov = false;
        for (_, inst) in walker.state().instructions.entries().iter() {
            if inst.mnemonic == Mnemonic::Mov {
                if inst.operands.len() == 2 {
                    if let (Operand::Reg(dst), Operand::Reg(src)) = (&inst.operands[0], &inst.operands[1]) {
                        // Check if this is a move from y_reg to x_reg
                        let x_reg = walker.state().local_bindings.get("x");
                        let y_reg = walker.state().local_bindings.get("y");
                        if Some(*dst) == x_reg && Some(*src) == y_reg {
                            found_reg_to_reg_mov = true;
                        }
                    }
                }
            }
        }
        assert!(found_reg_to_reg_mov, "Should emit reg-to-reg move x = y");
    }

    /// Test 4: unresolved LHS fires T0518 diagnostic.
    ///
    /// Scenario: LHS is neither module symbol nor local binding.
    /// Expected: typed T0518 diagnostic, walker.diagnostics() is empty (SILENT-DRAIN)
    #[test]
    fn visit_var_assign_unknown_lhs_fires_typed_t0518() {
        let mut arena = IrArena::new();

        // Allocate LHS: Var node for undefined symbol "undefined_var"
        let lhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(lhs_id, "undefined_var".to_string());

        // Allocate RHS: Var node for local binding "y"
        let rhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(rhs_id, "y".to_string());

        // Allocate operator (unused)
        let op_id = arena.alloc(IrKind::Placeholder, span());

        // Allocate Store: [lhs, op, rhs]
        let store_id = arena.alloc_with_children(IrKind::Store, span(), [lhs_id, op_id, rhs_id]);

        // Create Action containing the Store
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [store_id]);

        // Create Lambda with Action body
        let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.state_mut().local_bindings.insert("y".to_string(), abi::RAX);

        walker.walk(&mut arena);

        // Verify: T0518 in typed diagnostics
        let has_t0518 = walker
            .take_typed_diagnostics()
            .iter()
            .any(|d| d.code().to_string().contains("0518"));
        assert!(has_t0518, "Should emit T0518 for unresolved LHS");

        // Verify: legacy diagnostics are empty (SILENT-DRAIN guardrail)
        assert_eq!(
            walker.diagnostics().len(),
            0,
            "Legacy diagnostics should be empty (SILENT-DRAIN)"
        );
    }

    /// Test 5: unsupported RHS shape fires T0518 diagnostic.
    ///
    /// Scenario: RHS is App (function call) which is unsupported.
    /// Expected: typed T0518 diagnostic
    #[test]
    fn visit_var_assign_unsupported_rhs_shape_fires_typed() {
        let mut arena = IrArena::new();

        // Allocate LHS: Var node for local binding "x"
        let lhs_id = arena.alloc(IrKind::Var, span());
        arena.binding_names_mut().insert(lhs_id, "x".to_string());

        // Allocate RHS: App (unsupported for now)
        let rhs_id = arena.alloc(IrKind::App, span());

        // Allocate operator (unused)
        let op_id = arena.alloc(IrKind::Placeholder, span());

        // Allocate Store: [lhs, op, rhs]
        let store_id = arena.alloc_with_children(IrKind::Store, span(), [lhs_id, op_id, rhs_id]);

        // Create Action containing the Store
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [store_id]);

        // Create Lambda with Action body
        let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.state_mut().local_bindings.insert("x".to_string(), abi::RAX);

        walker.walk(&mut arena);

        // Verify: T0518 in typed diagnostics
        let has_t0518 = walker
            .take_typed_diagnostics()
            .iter()
            .any(|d| d.code().to_string().contains("0518"));
        assert!(has_t0518, "Should emit T0518 for unsupported RHS shape");

        // Verify: legacy diagnostics are empty
        assert_eq!(
            walker.diagnostics().len(),
            0,
            "Legacy diagnostics should be empty (SILENT-DRAIN)"
        );
    }
}
