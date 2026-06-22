//! PA7C-m2-002: Unit tests for Let-literal scratch binding in emit_walker.
//!
//! These tests verify the scratch register assignment for let-bindings in multi-statement
//! function bodies:
//! - Let with Literal RHS → scratch_assignment[i] and local_bindings[(name → scratch_reg)]
//! - Multiple lets → distinct scratch regs from [RAX, RCX, RDX, R8]
//! - Register exhaustion → T0527 diagnostic

#[cfg(test)]
mod tests {
    use paideia_as_elaborator::emit_walker::EmitWalker;
    use paideia_as_ir::instruction::RegId;
    use paideia_as_ir::{IrArena, IrKind};
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// Test 1: Single Let with Literal(0x10) RHS assigns first scratch register.
    #[test]
    fn let_literal_assigns_first_scratch_reg() {
        let mut arena = IrArena::new();

        // Allocate: Literal node, then Let with Literal as child.
        let lit_id = arena.alloc(IrKind::Literal, span());
        let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);

        // Register binding name
        arena.binding_names_mut().insert(let_id, "x".to_string());

        // Register the literal value 0x10
        arena.literal_values_mut().insert(lit_id, 0x10);

        // Create a block containing the let statement
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [let_id]);

        // Create a lambda with the action as its body
        let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.walk(&mut arena);

        // Verify scratch_assignment[0] == RAX (RegId(0))
        assert_eq!(
            walker.state().scratch_assignment.len(),
            1,
            "Should have 1 scratch assignment"
        );
        assert_eq!(
            walker.state().scratch_assignment[0],
            RegId(0),
            "First scratch should be RAX"
        );

        // Verify local_bindings.get("x") == Some(RAX)
        assert_eq!(
            walker.state().local_bindings.get("x"),
            Some(RegId(0)),
            "Binding 'x' should map to RAX"
        );

        // Verify 1 Mov instruction was emitted
        let mov_id = paideia_as_ir::IrNodeId::new(let_id.get() * 3).unwrap();
        let inst = walker.state().instructions.get(mov_id);
        assert!(inst.is_some(), "Mov instruction should be emitted");
        if let Some(mov) = inst {
            assert_eq!(mov.mnemonic, paideia_as_ir::instruction::Mnemonic::Mov);
        }
    }

    /// Test 2: Three Lets (a, b, c) with Literal RHS assign distinct scratch regs.
    #[test]
    fn three_let_chain_assigns_distinct_scratch_regs() {
        let mut arena = IrArena::new();

        // Allocate three Let nodes with Literal RHS
        let lit_a = arena.alloc(IrKind::Literal, span());
        let let_a = arena.alloc_with_children(IrKind::Let, span(), [lit_a]);
        arena.binding_names_mut().insert(let_a, "a".to_string());
        arena.literal_values_mut().insert(lit_a, 0x10);

        let lit_b = arena.alloc(IrKind::Literal, span());
        let let_b = arena.alloc_with_children(IrKind::Let, span(), [lit_b]);
        arena.binding_names_mut().insert(let_b, "b".to_string());
        arena.literal_values_mut().insert(lit_b, 0x20);

        let lit_c = arena.alloc(IrKind::Literal, span());
        let let_c = arena.alloc_with_children(IrKind::Let, span(), [lit_c]);
        arena.binding_names_mut().insert(let_c, "c".to_string());
        arena.literal_values_mut().insert(lit_c, 0x30);

        // Create a block containing the three let statements
        let action_id = arena.alloc_with_children(IrKind::Action, span(), [let_a, let_b, let_c]);

        // Create a lambda with the action as its body
        let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.walk(&mut arena);

        // Verify scratch_assignment has 3 entries
        assert_eq!(
            walker.state().scratch_assignment.len(),
            3,
            "Should have 3 scratch assignments"
        );

        // Verify they are RAX, RCX, RDX
        assert_eq!(walker.state().scratch_assignment[0], RegId(0), "First should be RAX");
        assert_eq!(walker.state().scratch_assignment[1], RegId(1), "Second should be RCX");
        assert_eq!(walker.state().scratch_assignment[2], RegId(2), "Third should be RDX");

        // Verify local_bindings
        assert_eq!(
            walker.state().local_bindings.get("a"),
            Some(RegId(0)),
            "Binding 'a' should map to RAX"
        );
        assert_eq!(
            walker.state().local_bindings.get("b"),
            Some(RegId(1)),
            "Binding 'b' should map to RCX"
        );
        assert_eq!(
            walker.state().local_bindings.get("c"),
            Some(RegId(2)),
            "Binding 'c' should map to RDX"
        );

        // Verify 3 Mov instructions were emitted
        let mut mov_count = 0;
        for (_, inst) in walker.state().instructions.entries().iter() {
            if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov {
                mov_count += 1;
            }
        }
        assert_eq!(mov_count, 3, "Should have emitted 3 Mov instructions");
    }

    /// Test 3: Five Lets exhaust the 4-register pool and emit T0527.
    #[test]
    fn five_let_chain_exhausts_pool_and_emits_t0527() {
        let mut arena = IrArena::new();

        // Allocate five Let nodes with Literal RHS
        let mut let_ids = Vec::new();
        for i in 1..=5 {
            let lit = arena.alloc(IrKind::Literal, span());
            let let_node = arena.alloc_with_children(IrKind::Let, span(), [lit]);
            let name = format!("var_{}", i);
            arena.binding_names_mut().insert(let_node, name);
            arena.literal_values_mut().insert(lit, (i as i64) * 0x10);
            let_ids.push(let_node);
        }

        // Create a block containing the five let statements
        let action_id = arena.alloc_with_children(IrKind::Action, span(), let_ids.as_slice());

        // Create a lambda with the action as its body
        let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

        // Walk the arena
        let mut walker = EmitWalker::new();
        walker.walk(&mut arena);

        // Verify T0527 was emitted in diagnostics
        let has_t0527 = walker
            .diagnostics()
            .iter()
            .any(|d| d.contains("T0527"));
        assert!(has_t0527, "Should emit T0527 diagnostic for register exhaustion");

        // Verify scratch_assignment stopped at 4 registers
        assert_eq!(
            walker.state().scratch_assignment.len(),
            4,
            "Should have only 4 scratch assignments"
        );

        // Verify they are RAX, RCX, RDX, R8
        assert_eq!(walker.state().scratch_assignment[0], RegId(0), "First should be RAX");
        assert_eq!(walker.state().scratch_assignment[1], RegId(1), "Second should be RCX");
        assert_eq!(walker.state().scratch_assignment[2], RegId(2), "Third should be RDX");
        assert_eq!(walker.state().scratch_assignment[3], RegId(8), "Fourth should be R8");
    }
}
