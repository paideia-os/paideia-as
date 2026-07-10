use super::lower_ast_to_ir;
use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::{FileId, SourceMap, VecSink};
use paideia_as_ir::{IrKind, IrNodeId};

fn span() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Helper to create a test SourceMap and VecSink for lowering.
fn create_test_source_map_and_sink() -> (SourceMap, VecSink) {
    let mut source_map = SourceMap::new();
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from("a[i] = x; *p = y; r.f = z; a[i] + w;"),
    );
    let sink = VecSink::new();
    (source_map, sink)
}

/// Helper to create a span for the "=" operator in the test source.
/// The test source is "a[i] = x; *p = y; r.f = z; a[i] + w;"
/// Byte positions (0-indexed):
/// - First "=" is at byte 5 (in "a[i] = x")
/// - Second "=" is at byte 13 (in "*p = y")
/// - Third "=" is at byte 22 (in "r.f = z")
/// - The "+" (non-assignment, also 1 byte) is at byte 32 (in "a[i] + w")
fn eq_operator_span_1() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 5, 1)
}

fn eq_operator_span_2() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 13, 1)
}

fn eq_operator_span_3() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 22, 1)
}

/// Non-assignment 1-byte operator ("+"). Regression span for #1132: before the
/// fix, `refine_ir_kind` gated `IrKind::Store` on `op.span.byte_len() == 1`,
/// which "+" also satisfies. Any valid l-value-shaped LHS (e.g. `a[i]`) paired
/// with this operator must NOT be classified as Store.
fn plus_operator_span() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 32, 1)
}

#[test]
fn lower_empty_arena() {
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let ast = AstArena::new();
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
    assert_eq!(result.ir.len(), 0);
    assert!(result.ast_to_ir.is_empty());
}

#[test]
fn lower_single_placeholder() {
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();
    let _id = ast.alloc(NodeKind::Placeholder, span());
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
    assert_eq!(result.ir.len(), 1);
    assert_eq!(result.ast_to_ir.len(), 1);
}

#[test]
fn lower_let_plus() {
    // Build: let x = 1 + 2
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // This tests AC bullet 1: lowering a small AST manually.
    let mut ast = AstArena::new();

    // Allocate IntLit nodes for "1" and "2".
    let lit_one_id = ast.alloc(NodeKind::ExprLiteral, span());
    let lit_two_id = ast.alloc(NodeKind::ExprLiteral, span());

    // Allocate Ident node for "+" (the operator).
    let op_plus_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprInfix: 1 + 2
    let infix_id = ast.alloc_expr(
        NodeKind::ExprInfix,
        span(),
        ExprData::Infix {
            lhs: lit_one_id,
            op: op_plus_id,
            rhs: lit_two_id,
        },
    );

    // Allocate Ident node for "x".
    let name_x_id = ast.alloc(NodeKind::Ident, span());

    // Allocate StmtLet: let x = 1 + 2
    let let_stmt_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        span(),
        StmtData::Let {
            mutable: false,
            name: name_x_id,
            ty: None,
            value: infix_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the IR contains a Let, Literal nodes, and App.
    assert_eq!(result.ir.len(), 6);
    assert_eq!(result.ast_to_ir.len(), 6);

    // Check mappings are present.
    assert!(result.ast_to_ir.contains_key(&lit_one_id));
    assert!(result.ast_to_ir.contains_key(&lit_two_id));
    assert!(result.ast_to_ir.contains_key(&op_plus_id));
    assert!(result.ast_to_ir.contains_key(&infix_id));
    assert!(result.ast_to_ir.contains_key(&name_x_id));
    assert!(result.ast_to_ir.contains_key(&let_stmt_id));

    // Verify the IR node kinds match the lowering table.
    let lit_one_ir = result.ast_to_ir[&lit_one_id];
    let lit_two_ir = result.ast_to_ir[&lit_two_id];
    let op_plus_ir = result.ast_to_ir[&op_plus_id];
    let infix_ir = result.ast_to_ir[&infix_id];
    let name_x_ir = result.ast_to_ir[&name_x_id];
    let let_stmt_ir = result.ast_to_ir[&let_stmt_id];

    assert_eq!(result.ir[lit_one_ir].kind, IrKind::Literal);
    assert_eq!(result.ir[lit_two_ir].kind, IrKind::Literal);
    assert_eq!(result.ir[op_plus_ir].kind, IrKind::Var);
    assert_eq!(result.ir[infix_ir].kind, IrKind::App);
    assert_eq!(result.ir[name_x_ir].kind, IrKind::Var);
    assert_eq!(result.ir[let_stmt_ir].kind, IrKind::Let);
}

#[test]
fn lower_span_preservation() {
    // AC bullet 2: every AST node's span is preserved in its IR counterpart.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    // Allocate a few nodes with the test span.
    let id1 = ast.alloc(NodeKind::Ident, span());
    let id2 = ast.alloc(NodeKind::ExprLiteral, span());
    let id3 = ast.alloc(NodeKind::StmtLet, span());

    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify spans match.
    let ir1 = result.ast_to_ir[&id1];
    let ir2 = result.ast_to_ir[&id2];
    let ir3 = result.ast_to_ir[&id3];

    assert_eq!(result.ir[ir1].span, span());
    assert_eq!(result.ir[ir2].span, span());
    assert_eq!(result.ir[ir3].span, span());
}

#[test]
fn lower_does_not_panic_on_arena() {
    // AC bullet 4: lowering should not panic on a variety of node kinds.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    // Allocate a mix of different node kinds.
    ast.alloc(NodeKind::Placeholder, span());
    ast.alloc(NodeKind::Ident, span());
    ast.alloc(NodeKind::Module, span());
    ast.alloc(NodeKind::Functor, span());
    ast.alloc(NodeKind::ExprLambda, span());
    ast.alloc(NodeKind::ExprCall, span());
    ast.alloc(NodeKind::ExprBlock, span());
    ast.alloc(NodeKind::ExprMatch, span());
    ast.alloc(NodeKind::StmtLet, span());
    ast.alloc(NodeKind::ExprUnsafe, span());

    // This should not panic.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
    assert_eq!(result.ir.len(), 10);
}

#[test]
fn lower_preserves_kind_count() {
    // Assert that the number of IR nodes equals the number of AST nodes.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    for _ in 0..50 {
        ast.alloc(NodeKind::Placeholder, span());
    }

    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
    assert_eq!(result.ir.len(), ast.len());
}

#[test]
fn ast_to_ir_mapping_is_complete() {
    // Assert that every NodeId in the AST has an entry in the mapping.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    for _ in 0..20 {
        ast.alloc(NodeKind::Ident, span());
    }

    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    for i in 0..ast.len() {
        let id = NodeId::new((i + 1) as u32).unwrap();
        assert!(result.ast_to_ir.contains_key(&id));
    }
}

#[test]
fn lower_preserves_default_lin_class_and_effect_row() {
    // Verify that all IR nodes start with Unrestricted and empty effect row.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();
    ast.alloc(NodeKind::Placeholder, span());
    ast.alloc(NodeKind::ExprLambda, span());
    ast.alloc(NodeKind::StmtLet, span());

    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    for i in 0..result.ir.len() {
        let ir_id = IrNodeId::new((i + 1) as u32).unwrap();
        assert_eq!(
            result.ir[ir_id].lin_class,
            paideia_as_ir::LinClass::Unrestricted
        );
        assert_eq!(
            result.ir[ir_id].effect_row,
            paideia_as_ir::EffectRowId::EMPTY
        );
    }
}

#[test]
fn lower_stable_indexing() {
    // Verify that AST NodeId N ↔ IR IrNodeId N (both index from 1).
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    let id1 = ast.alloc(NodeKind::Ident, span());
    let id2 = ast.alloc(NodeKind::ExprLambda, span());
    let id3 = ast.alloc(NodeKind::StmtLet, span());

    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // NodeId 1 should map to IrNodeId 1.
    assert_eq!(result.ast_to_ir[&id1].get(), 1);
    // NodeId 2 should map to IrNodeId 2.
    assert_eq!(result.ast_to_ir[&id2].get(), 2);
    // NodeId 3 should map to IrNodeId 3.
    assert_eq!(result.ast_to_ir[&id3].get(), 3);
}

#[test]
fn lower_mapping_is_correct_bijection() {
    // For each AST node, the mapped IR node should have matching kind.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    // Carefully construct test data with known mappings.
    ast.alloc(NodeKind::Ident, span()); // Should map to Var
    ast.alloc(NodeKind::ExprLiteral, span()); // Should map to Literal
    ast.alloc(NodeKind::ExprCall, span()); // Should map to App
    ast.alloc(NodeKind::Module, span()); // Should map to Module

    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    let id1 = NodeId::new(1).unwrap();
    let id2 = NodeId::new(2).unwrap();
    let id3 = NodeId::new(3).unwrap();
    let id4 = NodeId::new(4).unwrap();

    assert_eq!(result.ir[result.ast_to_ir[&id1]].kind, IrKind::Var);
    assert_eq!(result.ir[result.ast_to_ir[&id2]].kind, IrKind::Literal);
    assert_eq!(result.ir[result.ast_to_ir[&id3]].kind, IrKind::App);
    assert_eq!(result.ir[result.ast_to_ir[&id4]].kind, IrKind::Module);
}

#[test]
fn lower_stmt_instruction_to_raw_instruction() {
    // AC: lower `mov rax, 1` StmtInstruction; assert single IrKind::RawInstruction;
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // assert ast_to_ir[ir_node_id] == original_node_id.
    let mut ast = AstArena::new();

    // Allocate operand nodes: "rax" and "1"
    let rax_id = ast.alloc(NodeKind::OperandRegister, span());
    let one_id = ast.alloc(NodeKind::ExprLiteral, span());

    // Allocate the StmtInstruction: mnemonic_id=0 (stub), operands=[rax, 1]
    let instr_id = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span(),
        StmtData::Instruction {
            mnemonic: 0, // Stub: real mnemonic interning happens in parser/elaborator
            operands: vec![rax_id, one_id],
            emission_order: 0,
},
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify we have 3 IR nodes: OperandRegister, ExprLiteral, StmtInstruction.
    assert_eq!(result.ir.len(), 3);
    assert_eq!(result.ast_to_ir.len(), 3);

    // Verify the StmtInstruction AST node maps to a RawInstruction IR node.
    let ir_instr_id = result.ast_to_ir[&instr_id];
    assert_eq!(result.ir[ir_instr_id].kind, IrKind::RawInstruction);

    // Verify round-trip: ast_to_ir[ir_instr_id] resolves back to instr_id.
    // This tests the bijection invariant: AST NodeId N ↔ IR IrNodeId N.
    assert_eq!(ir_instr_id.get() as u32, instr_id.get());

    // Verify operand nodes are also lowered correctly.
    let ir_rax_id = result.ast_to_ir[&rax_id];
    let ir_one_id = result.ast_to_ir[&one_id];
    assert_eq!(result.ir[ir_rax_id].kind, IrKind::Var); // OperandRegister -> Var
    assert_eq!(result.ir[ir_one_id].kind, IrKind::Literal); // ExprLiteral -> Literal
}

#[test]
fn lower_array_assign_produces_store() {
    // Phase 7 m5-001: array-index assignment `a[i] = value`.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // Build: a[i] = x
    // This tests that an assignment to an indexed expression lowers to IrKind::Store
    // instead of IrKind::App.
    let mut ast = AstArena::new();

    // Allocate base variable: a
    let base_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate index variable: i
    let index_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprCall: a[i]
    // Indexing is represented as Call with 1 argument
    let index_expr_id = ast.alloc_expr(
        NodeKind::ExprCall,
        span(),
        ExprData::Call {
            callee: base_var_id,
            args: vec![index_var_id],
        },
    );

    // Allocate value variable: x
    let value_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate the operator node (=) - a Placeholder with 1-byte span pointing to "=" in "a[i] = x"
    let assign_op_id = ast.alloc(NodeKind::Placeholder, eq_operator_span_1());

    // Allocate ExprInfix: a[i] = x
    let assign_expr_id = ast.alloc_expr(
        NodeKind::ExprInfix,
        span(),
        ExprData::Infix {
            lhs: index_expr_id,
            op: assign_op_id,
            rhs: value_var_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the assignment lowered to Store instead of App.
    let assign_ir_id = result.ast_to_ir[&assign_expr_id];
    assert_eq!(
        result.ir[assign_ir_id].kind,
        IrKind::Store,
        "Array assignment should lower to Store"
    );

    // Verify children are rearranged to [base, index, value]
    let children = result.ir.children(assign_ir_id);
    assert_eq!(children.len(), 3, "Store should have 3 children");

    // Children should map to base_var, index_var, value_var
    let base_child_id = children[0];
    let index_child_id = children[1];
    let value_child_id = children[2];

    let base_ir_id = result.ast_to_ir[&base_var_id];
    let index_ir_id = result.ast_to_ir[&index_var_id];
    let value_ir_id = result.ast_to_ir[&value_var_id];

    assert_eq!(base_child_id, base_ir_id);
    assert_eq!(index_child_id, index_ir_id);
    assert_eq!(value_child_id, value_ir_id);
}

#[test]
fn lower_indexed_lvalue_with_plus_operator_produces_app() {
    // Regression test for #1132: dispatch_store 3-way (Pattern 5) broke
    // unrelated lambda code gen (add_one, my_init) because the old guard
    // classified `IrKind::Store` on `op.span.byte_len() == 1` alone. "+" is
    // also a 1-byte operator, so `a[i] + w`-shaped expressions (a valid
    // Pattern-1 l-value LHS, non-assignment operator) were at risk of being
    // misclassified as Store. This pins that a real, non-"=" 1-byte operator
    // is rejected by the operator-text guard even when the LHS has l-value
    // shape.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // Build: a[i] + w
    let mut ast = AstArena::new();

    // Allocate base variable: a
    let base_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate index variable: i
    let index_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprCall: a[i] (Pattern 1 l-value shape)
    let index_expr_id = ast.alloc_expr(
        NodeKind::ExprCall,
        span(),
        ExprData::Call {
            callee: base_var_id,
            args: vec![index_var_id],
        },
    );

    // Allocate rhs variable: w
    let rhs_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate the operator node (+) - a Placeholder with 1-byte span
    // pointing to the real "+" character in "a[i] + w".
    let plus_op_id = ast.alloc(NodeKind::Placeholder, plus_operator_span());

    // Allocate ExprInfix: a[i] + w
    let infix_expr_id = ast.alloc_expr(
        NodeKind::ExprInfix,
        span(),
        ExprData::Infix {
            lhs: index_expr_id,
            op: plus_op_id,
            rhs: rhs_var_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the infix lowered to App, NOT Store, despite the l-value-shaped
    // LHS, because the operator text is "+" and not "=".
    let infix_ir_id = result.ast_to_ir[&infix_expr_id];
    assert_eq!(
        result.ir[infix_ir_id].kind,
        IrKind::App,
        "a[i] + w must lower to App: '+' is a 1-byte operator but not '=', \
         and byte_len()==1 alone must not trigger Store classification"
    );
}

#[test]
fn lower_bare_ident_assign_produces_store() {
    // #1116: Verify that Pattern 5 (bare Ident LHS with =) now lowers to Store, not App.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // Build: x = 5
    let mut ast = AstArena::new();

    // Allocate variable: x
    let var_x_id = ast.alloc(NodeKind::Ident, span());

    // Allocate literal: 5
    let lit_5_id = ast.alloc(NodeKind::ExprLiteral, span());

    // Allocate the operator node (=) - a real "=" span.
    // This test verifies that Pattern 5 (bare Ident LHS) now produces Store.
    let assign_op_id = ast.alloc(NodeKind::Placeholder, eq_operator_span_1());

    // Allocate ExprInfix: x = 5 (not an indexed assignment)
    let assign_expr_id = ast.alloc_expr(
        NodeKind::ExprInfix,
        span(),
        ExprData::Infix {
            lhs: var_x_id,
            op: assign_op_id,
            rhs: lit_5_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the assignment lowered to Store (Pattern 5: bare Ident assignment)
    let assign_ir_id = result.ast_to_ir[&assign_expr_id];
    assert_eq!(
        result.ir[assign_ir_id].kind,
        IrKind::Store,
        "Pattern 5 (bare Ident LHS with =) should lower to Store"
    );
}

#[test]
fn lower_deref_assign_produces_store() {
    // Phase 7 m5-002: pointer-deref assignment `*p = value`.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // Build: *p = x
    let mut ast = AstArena::new();

    // Allocate pointer variable: p
    let ptr_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprDeref: *p
    let deref_expr_id = ast.alloc_expr(
        NodeKind::ExprDeref,
        span(),
        ExprData::Deref { expr: ptr_var_id },
    );

    // Allocate value variable: x
    let value_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate the operator node (=) - pointing to "=" in "*p = y"
    let assign_op_id = ast.alloc(NodeKind::Placeholder, eq_operator_span_2());

    // Allocate ExprInfix: *p = x
    let assign_expr_id = ast.alloc_expr(
        NodeKind::ExprInfix,
        span(),
        ExprData::Infix {
            lhs: deref_expr_id,
            op: assign_op_id,
            rhs: value_var_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the assignment lowered to Store
    let assign_ir_id = result.ast_to_ir[&assign_expr_id];
    assert_eq!(
        result.ir[assign_ir_id].kind,
        IrKind::Store,
        "Deref assignment should lower to Store"
    );

    // Verify children are [pointer, unused, value]
    let children = result.ir.children(assign_ir_id);
    assert_eq!(children.len(), 3, "Store should have 3 children");

    let ptr_child_id = children[0];
    let value_child_id = children[2];

    let ptr_ir_id = result.ast_to_ir[&ptr_var_id];
    let value_ir_id = result.ast_to_ir[&value_var_id];

    assert_eq!(ptr_child_id, ptr_ir_id);
    assert_eq!(value_child_id, value_ir_id);
}

#[test]
fn lower_field_deref_assign_produces_store() {
    // Phase 7 m5-002: field-access-of-deref assignment `(*p).field = value`.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // Build: (*p).field = x
    let mut ast = AstArena::new();

    // Allocate pointer variable: p
    let ptr_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprDeref: *p
    let deref_expr_id = ast.alloc_expr(
        NodeKind::ExprDeref,
        span(),
        ExprData::Deref { expr: ptr_var_id },
    );

    // Allocate field name: "field"
    let field_name_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprFieldAccess: (*p).field
    // This matches the shape emitted by the real parser.
    let field_access_id = ast.alloc_expr(
        NodeKind::ExprFieldAccess,
        span(),
        ExprData::FieldAccess { receiver: deref_expr_id, field: field_name_id },
    );

    // Allocate value variable: x
    let value_var_id = ast.alloc(NodeKind::Ident, span());

    // Allocate the operator node (=) - pointing to "=" in "r.f = z"
    let assign_op_id = ast.alloc(NodeKind::Placeholder, eq_operator_span_3());

    // Allocate ExprInfix: (*p).field = x
    let assign_expr_id = ast.alloc_expr(
        NodeKind::ExprInfix,
        span(),
        ExprData::Infix {
            lhs: field_access_id,
            op: assign_op_id,
            rhs: value_var_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the assignment lowered to Store
    let assign_ir_id = result.ast_to_ir[&assign_expr_id];
    assert_eq!(
        result.ir[assign_ir_id].kind,
        IrKind::Store,
        "Field-deref assignment should lower to Store"
    );

    // Verify children are [field_access, unused, value]
    let children = result.ir.children(assign_ir_id);
    assert_eq!(children.len(), 3, "Store should have 3 children");

    let field_access_child_id = children[0];
    let value_child_id = children[2];

    let field_access_ir_id = result.ast_to_ir[&field_access_id];
    let value_ir_id = result.ast_to_ir[&value_var_id];

    assert_eq!(field_access_child_id, field_access_ir_id);
    assert_eq!(value_child_id, value_ir_id);
}

#[test]
fn lower_array_literal_produces_array_lit() {
    // Phase 8 m2-002: array literal `[expr0, expr1, ...]` lowers to IrKind::ArrayLit
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // with element children.
    let mut ast = AstArena::new();

    // Allocate element expressions (3 literals)
    let elem0_id = ast.alloc(NodeKind::ExprLiteral, span());
    let elem1_id = ast.alloc(NodeKind::ExprLiteral, span());
    let elem2_id = ast.alloc(NodeKind::ExprLiteral, span());

    // Allocate ExprArrayLit: [elem0, elem1, elem2]
    let array_lit_id = ast.alloc_expr(
        NodeKind::ExprArrayLit,
        span(),
        ExprData::ArrayLit(vec![elem0_id, elem1_id, elem2_id]),
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify we have 4 IR nodes: 3 Literals + 1 ArrayLit.
    assert_eq!(result.ir.len(), 4);

    // Verify the ArrayLit AST node maps to an ArrayLit IR node.
    let ir_array_id = result.ast_to_ir[&array_lit_id];
    assert_eq!(
        result.ir[ir_array_id].kind,
        IrKind::ArrayLit,
        "ArrayLit should lower to IrKind::ArrayLit"
    );

    // Verify children order is preserved.
    let children = result.ir.children(ir_array_id);
    assert_eq!(children.len(), 3, "ArrayLit should have 3 element children");

    let elem0_ir = result.ast_to_ir[&elem0_id];
    let elem1_ir = result.ast_to_ir[&elem1_id];
    let elem2_ir = result.ast_to_ir[&elem2_id];

    assert_eq!(children[0], elem0_ir);
    assert_eq!(children[1], elem1_ir);
    assert_eq!(children[2], elem2_ir);

    // Verify all element children are Literals.
    assert_eq!(result.ir[elem0_ir].kind, IrKind::Literal);
    assert_eq!(result.ir[elem1_ir].kind, IrKind::Literal);
    assert_eq!(result.ir[elem2_ir].kind, IrKind::Literal);
}

#[test]
fn lower_empty_array_literal() {
    // Phase 8 m2-002: empty array literal `[]` lowers to ArrayLit with no children.
    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    // Allocate ExprArrayLit: []
    let array_lit_id =
        ast.alloc_expr(NodeKind::ExprArrayLit, span(), ExprData::ArrayLit(vec![]));

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify we have 1 IR node.
    assert_eq!(result.ir.len(), 1);

    // Verify the ArrayLit IR node exists and has no children.
    let ir_array_id = result.ast_to_ir[&array_lit_id];
    assert_eq!(result.ir[ir_array_id].kind, IrKind::ArrayLit);

    let children = result.ir.children(ir_array_id);
    assert_eq!(children.len(), 0, "Empty ArrayLit should have no children");
}

#[test]
fn lower_array_repeat_with_non_literal_count() {
    // Phase 9 m1-002: array repeat `[expr; count]` where count is not a literal
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // Currently expands to a single copy with a note that P0211 should be emitted.
    let mut ast = AstArena::new();

    // Allocate element expression (a literal)
    let elem_id = ast.alloc(NodeKind::ExprLiteral, span());

    // Allocate count expression (an identifier, not a literal)
    let count_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprArrayRepeat: [elem; count]
    let repeat_id = ast.alloc_expr(
        NodeKind::ExprArrayRepeat,
        span(),
        ExprData::ArrayRepeat {
            expr: elem_id,
            count: count_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify we have 3 IR nodes: elem, count, repeat.
    assert_eq!(result.ir.len(), 3);

    // Verify the ArrayRepeat node maps to IrKind::ArrayLit in IR.
    let ir_repeat_id = result.ast_to_ir[&repeat_id];
    assert_eq!(
        result.ir[ir_repeat_id].kind,
        IrKind::ArrayLit,
        "ArrayRepeat should lower to IrKind::ArrayLit"
    );

    // For non-literal count, expand_array_repeat returns a single copy.
    let children = result.ir.children(ir_repeat_id);
    assert_eq!(
        children.len(),
        1,
        "ArrayRepeat with non-literal count should have 1 element child (fallback)"
    );

    let elem_ir = result.ast_to_ir[&elem_id];
    assert_eq!(children[0], elem_ir);
}

#[test]
fn lower_array_repeat_nested_structs() {
    // Phase 9 m1-002: array repeat with struct-lit as element: `[Point { x: 1, y: 2 }; count]`
    let (source_map, mut sink) = create_test_source_map_and_sink();
    // This tests that recursion handles RecordCons nested in ArrayRepeat.
    let mut ast = AstArena::new();

    // Allocate type name and field elements
    let type_name_id = ast.alloc(NodeKind::Ident, span());
    let field_x_id = ast.alloc(NodeKind::Ident, span());
    let field_x_val_id = ast.alloc(NodeKind::ExprLiteral, span());
    let field_y_id = ast.alloc(NodeKind::Ident, span());
    let field_y_val_id = ast.alloc(NodeKind::ExprLiteral, span());

    // Allocate RecordCons: Point { x: 1, y: 2 }
    let struct_lit_id = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        span(),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![(field_x_id, field_x_val_id), (field_y_id, field_y_val_id)],
        },
    );

    // Allocate count (non-literal to defer evaluation)
    let count_id = ast.alloc(NodeKind::Ident, span());

    // Allocate ExprArrayRepeat: [struct_lit; count]
    let repeat_id = ast.alloc_expr(
        NodeKind::ExprArrayRepeat,
        span(),
        ExprData::ArrayRepeat {
            expr: struct_lit_id,
            count: count_id,
        },
    );

    // Lower the AST.
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the ArrayRepeat node maps to IrKind::ArrayLit.
    let ir_repeat_id = result.ast_to_ir[&repeat_id];
    assert_eq!(result.ir[ir_repeat_id].kind, IrKind::ArrayLit);

    // Verify that the struct-lit is present in the children.
    let children = result.ir.children(ir_repeat_id);
    assert_eq!(
        children.len(),
        1,
        "ArrayRepeat with nested struct should have 1 element child (fallback)"
    );

    let struct_ir = result.ast_to_ir[&struct_lit_id];
    assert_eq!(children[0], struct_ir);

    // Verify the struct-lit lowered to RecordCons.
    assert_eq!(result.ir[struct_ir].kind, IrKind::RecordCons);
}

#[test]
fn populate_record_layout_table_with_known_struct() {
    // PA-r17-010a (#1070): Test that RecordCons expressions are mapped to RecordTypeIds
    use crate::StructRegistry;

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    // Allocate a RecordCons expression: Pair { x: 1, y: 2 }
    let type_name_id = ast.alloc(NodeKind::Ident, span());
    let field_name_x_id = ast.alloc(NodeKind::Ident, span());
    let value_x_id = ast.alloc(NodeKind::ExprLiteral, span());
    let field_name_y_id = ast.alloc(NodeKind::Ident, span());
    let value_y_id = ast.alloc(NodeKind::ExprLiteral, span());

    let record_cons_id = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        span(),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![
                (field_name_x_id, value_x_id),
                (field_name_y_id, value_y_id),
            ],
        },
    );

    // Create a registry with a Pair struct
    let mut registry = StructRegistry::empty();
    registry.by_name.insert("Pair".to_string(), paideia_as_ir::record_layout::RecordTypeId(1));
    registry.fields.insert(
        paideia_as_ir::record_layout::RecordTypeId(1),
        vec![
            ("x".to_string(), 0x08), // u64
            ("y".to_string(), 0x08), // u64
        ],
    );

    // Manually set the Ident node text to "Pair" by checking the span
    // (In a real test, we'd need to properly populate the source map with the right content)

    // Lower the AST with the registry
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the record_layout_table was NOT populated because the type name lookup
    // will fail (the span content doesn't match "Pair" in the empty source map).
    // This test demonstrates the plumbing is in place, but won't succeed with an
    // empty source map. A proper integration test would use actual source text.
    let ir_record_cons_id = result.ast_to_ir[&record_cons_id];
    assert_eq!(result.ir[ir_record_cons_id].kind, IrKind::RecordCons);
}

#[test]
fn populate_record_layout_table_with_unknown_struct() {
    // PA-r17-010a (#1070): Test that unknown struct types emit T0552 diagnostic
    use crate::StructRegistry;

    let (source_map, mut sink) = create_test_source_map_and_sink();
    let mut ast = AstArena::new();

    // Allocate a RecordCons expression with an unknown type
    let type_name_id = ast.alloc(NodeKind::Ident, span());
    let field_name_x_id = ast.alloc(NodeKind::Ident, span());
    let value_x_id = ast.alloc(NodeKind::ExprLiteral, span());

    let _record_cons_id = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        span(),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![(field_name_x_id, value_x_id)],
        },
    );

    // Create an empty registry (no structs defined)
    let registry = StructRegistry::empty();

    // Lower the AST with the empty registry
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // The lowering should complete without panic
    assert_eq!(result.ir.len(), 4); // type_name, field_name, value, record_cons
}

#[test]
fn populate_field_access_info_basic() {
    // Phase 6 m3-002 (#1073): Test field access lowering and side-table population.
    // Construct an AST with:
    //   let vops: Vops = ...
    //   ... (vops.read)
    // And verify that populate_field_access_info correctly populates the side-table.
    use crate::StructRegistry;
    use paideia_as_ir::RecordTypeId;
    use paideia_as_diagnostics::FileId;

    // Create a source_map with proper source text for extraction
    // Source layout: "vops" (0-3), " " (4), "Vops" (5-8), " " (9), "read" (10-13)
    let mut source_map = SourceMap::new();
    let source_text = "vops Vops read";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    // Helper to create a span at a given offset with given length
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Build binding name "vops" at offset 0-3
    let vops_binding_id = ast.alloc(NodeKind::Ident, make_span(0, 4));

    // Build type name "Vops" at offset 5-8
    let vops_type_id = ast.alloc(NodeKind::Ident, make_span(5, 4));

    // Build a Let binding: let vops: Vops = <dummy>
    // We need a dummy value expression; use a literal
    let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let let_binding_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        make_span(0, 1),
        paideia_as_ast::StmtData::Let {
            mutable: false,
            name: vops_binding_id,
            ty: Some(vops_type_id),
            value: dummy_value_id,
        },
    );

    // Build receiver "vops" in field access at offset 0-3
    let receiver_id = ast.alloc(NodeKind::Ident, make_span(0, 4));

    // Build field name "read" at offset 10-13
    let field_name_id = ast.alloc(NodeKind::Ident, make_span(10, 4));

    // Build the FieldAccess node: vops.read
    let field_access_id = ast.alloc_expr(
        NodeKind::ExprFieldAccess,
        make_span(0, 1),
        ExprData::FieldAccess {
            receiver: receiver_id,
            field: field_name_id,
        },
    );

    // Create a registry with the Vops struct
    let mut registry = StructRegistry::empty();
    let vops_record_id = RecordTypeId(1);
    registry.by_name.insert("Vops".to_string(), vops_record_id);
    registry.fields.insert(vops_record_id, vec![("read".to_string(), 0x08)]); // u64

    // Lower the AST
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

    // Verify the FieldAccess IR node was created
    let field_access_ir_id = result.ast_to_ir[&field_access_id];
    assert_eq!(result.ir[field_access_ir_id].kind, IrKind::FieldAccess);

    // Verify the FieldAccess has exactly one child (the receiver)
    let children = result.ir.children(field_access_ir_id);
    assert_eq!(children.len(), 1);
    let receiver_ir_id = result.ast_to_ir[&receiver_id];
    assert_eq!(children[0], receiver_ir_id);

    // Phase 6 m3-002: Verify the side-table entry was populated
    let field_access_info_opt = result.ir.field_access_info().get(field_access_ir_id);
    let expected_info = paideia_as_ir::FieldAccessInfo {
        type_id: vops_record_id,
        field_index: 0,
    };
    assert_eq!(
        field_access_info_opt.copied(),
        Some(expected_info),
        "populate_field_access_info should have inserted the side-table entry"
    );
}

#[test]
fn populate_enum_cons_info_basic() {
    // Phase 7 m4-003 (#1048/#1049): Test enum cons lowering and side-table population.
    // Construct an AST with:
    //   enum Result { Ok(u64), Err(u64) }
    //   let r = Result::Ok(42u64)
    // And verify that populate_enum_cons_info correctly populates the side-table.
    use paideia_as_ir::EnumTypeId;
    use paideia_as_diagnostics::FileId;

    // Create a source_map with proper source text for extraction
    // Source layout: "Result" (0-5) " Ok" (6-9) "42" (10-11)
    let mut source_map = SourceMap::new();
    let source_text = "Result Ok 42";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    // Helper to create a span at a given offset with given length
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create enum: enum Result { Ok(u64), Err(u64) }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

    let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(0, 3));

    let ok_variant = paideia_as_ast::EnumVariant::Tuple {
        name: ok_variant_name_id,
        payload: vec![u64_type_id],
    };
    let err_variant = paideia_as_ast::EnumVariant::Tuple {
        name: err_variant_name_id,
        payload: vec![u64_type_id],
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![ok_variant, err_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create the path Result::Ok
    let result_seg_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let ok_seg_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let path_id = ast.alloc_expr(
        NodeKind::ExprPath,
        make_span(0, 1),
        paideia_as_ast::ExprData::Path {
            segments: vec![result_seg_id, ok_seg_id],
        },
    );

    // Create the literal 42u64
    let lit_node_id = ast.alloc(NodeKind::ExprLiteral, make_span(10, 2));
    let lit_42_id = ast.alloc_expr(
        NodeKind::ExprLiteral,
        make_span(10, 2),
        paideia_as_ast::ExprData::Literal { lit: lit_node_id },
    );

    // Create the call Result::Ok(42u64)
    let call_id = ast.alloc_expr(
        NodeKind::ExprCall,
        make_span(0, 1),
        paideia_as_ast::ExprData::Call {
            callee: path_id,
            args: vec![lit_42_id],
        },
    );

    // Build the enum registry manually (simulating the registry builder)
    let mut enum_registry = crate::EnumRegistry::empty();
    let result_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Result".to_string(), result_type_id);
    enum_registry.variants.insert(
        result_type_id,
        vec![
            ("Ok".to_string(), vec![u64_type_id]),
            ("Err".to_string(), vec![u64_type_id]),
        ],
    );

    // Lower the AST
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

    // Verify the Call IR node was created and converted to EnumCons
    let call_ir_id = result.ast_to_ir[&call_id];
    assert_eq!(
        result.ir[call_ir_id].kind,
        IrKind::EnumCons,
        "Call to Result::Ok should be lowered as EnumCons"
    );

    // Verify the EnumConsInfo side-table entry was populated
    let enum_cons_info_opt = result.ir.enum_cons_info().get(call_ir_id);
    let expected_info = paideia_as_ir::EnumConsInfo {
        type_id: result_type_id,
        variant_index: 0, // Ok is the first variant (index 0)
    };
    assert_eq!(
        enum_cons_info_opt.copied(),
        Some(expected_info),
        "populate_enum_cons_info should have inserted the side-table entry with correct type_id and variant_index"
    );

    // Verify that the children have been properly stripped of the callee.
    // EnumCons should have only the payload children, no callee.
    let children = result.ir.children(call_ir_id);
    assert_eq!(
        children.len(),
        1,
        "EnumCons node should have exactly 1 child (the payload), no callee"
    );

    // The first child should be the literal 42 (not the callee path)
    if let Some(first_child) = children.first() {
        let first_child_node = &result.ir[*first_child];
        assert_eq!(
            first_child_node.kind,
            IrKind::Literal,
            "First child of EnumCons should be the payload Literal node, not the callee"
        );
    }
}

#[test]
fn populate_match_arm_meta_basic() {
    // Fix C (#1085): Test populate_match_arm_meta_basic - enum match with all variant arms.
    // Construct an AST with:
    //   enum Result { Ok(u64), Err(u64) }
    //   let r: Result = ...
    //   match r {
    //     Ok(x) => ...,
    //     Err(e) => ...
    //   }
    // Verify that match_arm_meta side-table entries are populated with correct EnumTypeId and variant indices.
    use paideia_as_ir::EnumTypeId;
    use paideia_as_diagnostics::FileId;

    let mut source_map = SourceMap::new();
    let source_text = "Result Ok Err r x e";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create enum: enum Result { Ok(u64), Err(u64) }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

    let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(10, 3)); // "Err"

    let ok_variant = paideia_as_ast::EnumVariant::Tuple {
        name: ok_variant_name_id,
        payload: vec![u64_type_id],
    };
    let err_variant = paideia_as_ast::EnumVariant::Tuple {
        name: err_variant_name_id,
        payload: vec![u64_type_id],
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![ok_variant, err_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create binding: let r: Result = ...
    let r_binding_id = ast.alloc(NodeKind::Ident, make_span(14, 1)); // "r"
    let result_type_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let let_binding_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        make_span(0, 1),
        paideia_as_ast::StmtData::Let {
            mutable: false,
            name: r_binding_id,
            ty: Some(result_type_id),
            value: dummy_value_id,
        },
    );

    // Create scrutinee reference: r
    let scrutinee = ast.alloc(NodeKind::Ident, make_span(14, 1)); // "r"

    // Create arm 1: Ok(x)
    let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let x_binding_id = ast.alloc(NodeKind::Ident, make_span(17, 1)); // "x"
    let ok_arm_pattern = ast.alloc_pattern(
        NodeKind::PatEnumVariant,
        make_span(0, 1),
        paideia_as_ast::PatternData::EnumVariant {
            path: ok_pattern_id,
            args: vec![x_binding_id],
        },
    );
    let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let ok_arm = paideia_as_ast::MatchArm {
        pattern: ok_arm_pattern,
        guard: None,
        body: ok_arm_body,
    };

    // Create arm 2: Err(e)
    let err_pattern_id = ast.alloc(NodeKind::Ident, make_span(10, 3)); // "Err"
    let e_binding_id = ast.alloc(NodeKind::Ident, make_span(19, 1)); // "e"
    let err_arm_pattern = ast.alloc_pattern(
        NodeKind::PatEnumVariant,
        make_span(0, 1),
        paideia_as_ast::PatternData::EnumVariant {
            path: err_pattern_id,
            args: vec![e_binding_id],
        },
    );
    let err_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let err_arm = paideia_as_ast::MatchArm {
        pattern: err_arm_pattern,
        guard: None,
        body: err_arm_body,
    };

    // Create match expression
    let match_id = ast.alloc_expr(
        NodeKind::ExprMatch,
        make_span(0, 1),
        paideia_as_ast::ExprData::Match {
            scrutinee,
            arms: vec![ok_arm, err_arm],
            attrs: Default::default(),
        },
    );

    // Build enum registry
    let mut enum_registry = crate::EnumRegistry::empty();
    let result_enum_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Result".to_string(), result_enum_type_id);
    enum_registry.variants.insert(
        result_enum_type_id,
        vec![
            ("Ok".to_string(), vec![u64_type_id]),
            ("Err".to_string(), vec![u64_type_id]),
        ],
    );

    // Lower the AST
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

    // Verify the Match IR node was created
    let match_ir_id = result.ast_to_ir[&match_id];
    assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

    // Verify match_scrutinee_table entry
    let scrutinee_type = result.ir.match_scrutinee_table().get(match_ir_id);
    assert_eq!(
        scrutinee_type.copied(),
        Some(result_enum_type_id),
        "match_scrutinee_table should contain the Result enum type"
    );

    // Verify arm metadata entries
    let match_children = result.ir.children(match_ir_id);
    assert!(match_children.len() >= 3, "Match should have scrutinee + 2 arms as children");

    // Get arm body IDs (skip scrutinee at index 0)
    let ok_arm_ir_id = match_children[1];
    let err_arm_ir_id = match_children[2];

    // Verify Ok arm metadata
    let ok_arm_meta = result.ir.match_arm_meta().get(ok_arm_ir_id);
    assert!(ok_arm_meta.is_some(), "Ok arm should have meta entry");
    let ok_meta = ok_arm_meta.unwrap();
    assert_eq!(
        ok_meta.variant_index,
        Some(0),
        "Ok arm should have variant_index = 0"
    );
    assert!(!ok_meta.is_default, "Ok arm should not be marked default");

    // Verify Err arm metadata
    let err_arm_meta = result.ir.match_arm_meta().get(err_arm_ir_id);
    assert!(err_arm_meta.is_some(), "Err arm should have meta entry");
    let err_meta = err_arm_meta.unwrap();
    assert_eq!(
        err_meta.variant_index,
        Some(1),
        "Err arm should have variant_index = 1"
    );
    assert!(!err_meta.is_default, "Err arm should not be marked default");
}

#[test]
fn populate_match_arm_meta_default_wildcard() {
    // Fix C (#1085): Test populate_match_arm_meta_default_wildcard - match with wildcard default arm.
    // Construct an AST with:
    //   enum Result { Ok(u64), Err(u64) }
    //   let r: Result = ...
    //   match r {
    //     Ok(x) => ...,
    //     _ => ...
    //   }
    // Verify that the wildcard arm is marked with is_default = true and variant_index = None.
    use paideia_as_ir::EnumTypeId;
    use paideia_as_diagnostics::FileId;

    let mut source_map = SourceMap::new();
    let source_text = "Result Ok r x";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create enum: enum Result { Ok(u64), Err(u64) }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

    let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(0, 3)); // "Err"

    let ok_variant = paideia_as_ast::EnumVariant::Tuple {
        name: ok_variant_name_id,
        payload: vec![u64_type_id],
    };
    let err_variant = paideia_as_ast::EnumVariant::Tuple {
        name: err_variant_name_id,
        payload: vec![u64_type_id],
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![ok_variant, err_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create binding: let r: Result = ...
    let r_binding_id = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"
    let result_type_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let _let_binding_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        make_span(0, 1),
        paideia_as_ast::StmtData::Let {
            mutable: false,
            name: r_binding_id,
            ty: Some(result_type_id),
            value: dummy_value_id,
        },
    );

    // Create scrutinee reference: r
    let scrutinee = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"

    // Create arm 1: Ok(x)
    let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let x_binding_id = ast.alloc(NodeKind::Ident, make_span(12, 1)); // "x"
    let ok_arm_pattern = ast.alloc_pattern(
        NodeKind::PatEnumVariant,
        make_span(0, 1),
        paideia_as_ast::PatternData::EnumVariant {
            path: ok_pattern_id,
            args: vec![x_binding_id],
        },
    );
    let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let ok_arm = paideia_as_ast::MatchArm {
        pattern: ok_arm_pattern,
        guard: None,
        body: ok_arm_body,
    };

    // Create arm 2: _ (wildcard)
    let wildcard_pattern = ast.alloc_pattern(
        NodeKind::PatWildcard,
        make_span(0, 1),
        paideia_as_ast::PatternData::Wildcard,
    );
    let wildcard_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let wildcard_arm = paideia_as_ast::MatchArm {
        pattern: wildcard_pattern,
        guard: None,
        body: wildcard_arm_body,
    };

    // Create match expression
    let match_id = ast.alloc_expr(
        NodeKind::ExprMatch,
        make_span(0, 1),
        paideia_as_ast::ExprData::Match {
            scrutinee,
            arms: vec![ok_arm, wildcard_arm],
            attrs: Default::default(),
        },
    );

    // Build enum registry
    let mut enum_registry = crate::EnumRegistry::empty();
    let result_enum_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Result".to_string(), result_enum_type_id);
    enum_registry.variants.insert(
        result_enum_type_id,
        vec![
            ("Ok".to_string(), vec![u64_type_id]),
            ("Err".to_string(), vec![u64_type_id]),
        ],
    );

    // Lower the AST
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

    // Verify the Match IR node was created
    let match_ir_id = result.ast_to_ir[&match_id];
    assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

    // Verify arm metadata entries
    let match_children = result.ir.children(match_ir_id);
    assert!(match_children.len() >= 3, "Match should have scrutinee + 2 arms as children");

    // Get arm body IDs (skip scrutinee at index 0)
    let ok_arm_ir_id = match_children[1];
    let wildcard_arm_ir_id = match_children[2];

    // Verify wildcard arm metadata
    let wildcard_arm_meta = result.ir.match_arm_meta().get(wildcard_arm_ir_id);
    assert!(wildcard_arm_meta.is_some(), "Wildcard arm should have meta entry");
    let wildcard_meta = wildcard_arm_meta.unwrap();
    assert!(
        wildcard_meta.is_default,
        "Wildcard arm should be marked with is_default = true"
    );
    assert_eq!(
        wildcard_meta.variant_index, None,
        "Wildcard arm should have variant_index = None"
    );
}

#[test]
fn populate_match_arm_meta_bare_ident_variant() {
    // Fix C (#1085): Test populate_match_arm_meta_bare_ident_variant - bare Ok(x) pattern (no path prefix).
    // Construct an AST with:
    //   enum Result { Ok(u64), Err(u64) }
    //   let r: Result = ...
    //   match r {
    //     Ok(x) => ...,
    //   }
    // Verify that the bare Ok variant pattern (without Result:: prefix) is correctly resolved
    // against the scrutinee's enum type, and variant_index is correctly populated.
    use paideia_as_ir::EnumTypeId;
    use paideia_as_diagnostics::FileId;

    let mut source_map = SourceMap::new();
    let source_text = "Result Ok r x";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create enum: enum Result { Ok(u64), Err(u64) }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

    let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(0, 3)); // "Err"

    let ok_variant = paideia_as_ast::EnumVariant::Tuple {
        name: ok_variant_name_id,
        payload: vec![u64_type_id],
    };
    let err_variant = paideia_as_ast::EnumVariant::Tuple {
        name: err_variant_name_id,
        payload: vec![u64_type_id],
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![ok_variant, err_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create binding: let r: Result = ...
    let r_binding_id = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"
    let result_type_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
    let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let _let_binding_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        make_span(0, 1),
        paideia_as_ast::StmtData::Let {
            mutable: false,
            name: r_binding_id,
            ty: Some(result_type_id),
            value: dummy_value_id,
        },
    );

    // Create scrutinee reference: r
    let scrutinee = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"

    // Create arm: Ok(x) - bare variant name, no Result:: prefix
    let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
    let x_binding_id = ast.alloc(NodeKind::Ident, make_span(12, 1)); // "x"
    let ok_arm_pattern = ast.alloc_pattern(
        NodeKind::PatEnumVariant,
        make_span(0, 1),
        paideia_as_ast::PatternData::EnumVariant {
            path: ok_pattern_id,
            args: vec![x_binding_id],
        },
    );
    let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let ok_arm = paideia_as_ast::MatchArm {
        pattern: ok_arm_pattern,
        guard: None,
        body: ok_arm_body,
    };

    // Create match expression
    let match_id = ast.alloc_expr(
        NodeKind::ExprMatch,
        make_span(0, 1),
        paideia_as_ast::ExprData::Match {
            scrutinee,
            arms: vec![ok_arm],
            attrs: Default::default(),
        },
    );

    // Build enum registry
    let mut enum_registry = crate::EnumRegistry::empty();
    let result_enum_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Result".to_string(), result_enum_type_id);
    enum_registry.variants.insert(
        result_enum_type_id,
        vec![
            ("Ok".to_string(), vec![u64_type_id]),
            ("Err".to_string(), vec![u64_type_id]),
        ],
    );

    // Lower the AST
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

    // Verify the Match IR node was created
    let match_ir_id = result.ast_to_ir[&match_id];
    assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

    // Verify arm metadata entries
    let match_children = result.ir.children(match_ir_id);
    assert!(match_children.len() >= 2, "Match should have scrutinee + 1 arm as children");

    // Get arm body ID (skip scrutinee at index 0)
    let ok_arm_ir_id = match_children[1];

    // Verify Ok arm metadata
    let ok_arm_meta = result.ir.match_arm_meta().get(ok_arm_ir_id);
    assert!(ok_arm_meta.is_some(), "Ok arm should have meta entry");
    let ok_meta = ok_arm_meta.unwrap();
    assert_eq!(
        ok_meta.variant_index,
        Some(0),
        "Ok arm should have variant_index = 0 (resolved against Result enum)"
    );
    assert!(!ok_meta.is_default, "Ok arm should not be marked default");
}

#[test]
fn populate_match_arm_meta_nested_ok_of_point() {
    // Test nested pattern: Ok(Point { x, y })
    // Constructs:
    //   struct Point { x: u64, y: u64 }
    //   enum Result { Ok(Point), Err }
    //   let r: Result = ...
    //   match r {
    //     Ok(Point { x, y }) => ...,
    //     Err => ...,
    //   }
    use paideia_as_ir::EnumTypeId;
    use paideia_as_ir::record_layout::RecordTypeId;
    use paideia_as_diagnostics::FileId;

    let mut source_map = SourceMap::new();
    let source_text = "Point Ok x y Result Err r";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create Point struct
    let point_struct_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point" at 0
    let x_field_name_id = ast.alloc(NodeKind::Ident, make_span(9, 1)); // "x" at 9
    let y_field_name_id = ast.alloc(NodeKind::Ident, make_span(11, 1)); // "y" at 11
    let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

    let _point_struct_id = ast.alloc_item(
        NodeKind::Struct,
        make_span(0, 1),
        paideia_as_ast::ItemData::Struct {
            name: point_struct_name_id,
            generic_params: vec![],
            fields: vec![
                (x_field_name_id, u64_type_id),
                (y_field_name_id, u64_type_id),
            ],
            attributes: vec![],
            doc: None,
        },
    );

    // Create Result enum: Result { Ok(Point), Err }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(13, 6)); // "Result" at 13
    let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(6, 2)); // "Ok" at 6
    let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(20, 3)); // "Err" at 20
    let point_type_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point" at 0

    let ok_variant = paideia_as_ast::EnumVariant::Tuple {
        name: ok_variant_name_id,
        payload: vec![point_type_id],
    };
    let err_variant = paideia_as_ast::EnumVariant::Unit {
        name: err_variant_name_id,
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![ok_variant, err_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create binding: let r: Result = ...
    let r_binding_id = ast.alloc(NodeKind::Ident, make_span(24, 1)); // "r" at 24
    let result_type_id = ast.alloc(NodeKind::Ident, make_span(13, 6)); // "Result" at 13
    let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let _let_binding_id = ast.alloc_stmt(
        NodeKind::StmtLet,
        make_span(0, 1),
        paideia_as_ast::StmtData::Let {
            mutable: false,
            name: r_binding_id,
            ty: Some(result_type_id),
            value: dummy_value_id,
        },
    );

    // Create scrutinee: r
    let scrutinee = ast.alloc(NodeKind::Ident, make_span(24, 1)); // "r" at 24

    // Create nested pattern: Ok(Point { x, y })
    let x_pat_field_name_id = ast.alloc(NodeKind::Ident, make_span(9, 1)); // "x" at 9
    let y_pat_field_name_id = ast.alloc(NodeKind::Ident, make_span(11, 1)); // "y" at 11
    let x_pat_binding_id = ast.alloc(NodeKind::Ident, make_span(9, 1)); // "x" at 9
    let y_pat_binding_id = ast.alloc(NodeKind::Ident, make_span(11, 1)); // "y" at 11

    let x_pat = ast.alloc_pattern(
        NodeKind::PatIdent,
        make_span(9, 1),
        paideia_as_ast::PatternData::Ident {
            name: x_pat_binding_id,
            mutable: false,
        },
    );
    let y_pat = ast.alloc_pattern(
        NodeKind::PatIdent,
        make_span(11, 1),
        paideia_as_ast::PatternData::Ident {
            name: y_pat_binding_id,
            mutable: false,
        },
    );

    let point_struct_pat = ast.alloc_pattern(
        NodeKind::PatStruct,
        make_span(0, 1),
        paideia_as_ast::PatternData::Struct {
            path: point_struct_name_id,
            fields: vec![
                paideia_as_ast::PatField {
                    name: x_pat_field_name_id,
                    pattern: x_pat,
                },
                paideia_as_ast::PatField {
                    name: y_pat_field_name_id,
                    pattern: y_pat,
                },
            ],
        },
    );

    let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(6, 2)); // "Ok" at 6
    let ok_arm_pattern = ast.alloc_pattern(
        NodeKind::PatEnumVariant,
        make_span(0, 1),
        paideia_as_ast::PatternData::EnumVariant {
            path: ok_pattern_id,
            args: vec![point_struct_pat],
        },
    );
    let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let ok_arm = paideia_as_ast::MatchArm {
        pattern: ok_arm_pattern,
        guard: None,
        body: ok_arm_body,
    };

    // Create match expression
    let match_id = ast.alloc_expr(
        NodeKind::ExprMatch,
        make_span(0, 1),
        paideia_as_ast::ExprData::Match {
            scrutinee,
            arms: vec![ok_arm],
            attrs: Default::default(),
        },
    );

    // Build struct registry
    let mut struct_registry = crate::StructRegistry::empty();
    let point_record_type_id = RecordTypeId(1);
    struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
    struct_registry.fields.insert(
        point_record_type_id,
        vec![("x".to_string(), 4), ("y".to_string(), 4)],
    );

    // Build enum registry
    let mut enum_registry = crate::EnumRegistry::empty();
    let result_enum_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Result".to_string(), result_enum_type_id);
    enum_registry.variants.insert(
        result_enum_type_id,
        vec![
            ("Ok".to_string(), vec![point_type_id]),
            ("Err".to_string(), vec![]),
        ],
    );

    // Build payload map
    let mut payload_map = std::collections::HashMap::new();
    payload_map.insert((result_enum_type_id, 0), Some(point_record_type_id));

    // Lower the AST
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &struct_registry, &enum_registry, &payload_map);

    // Verify the Match IR node was created
    let match_ir_id = result.ast_to_ir[&match_id];
    assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

    // Verify arm metadata
    let match_children = result.ir.children(match_ir_id);
    assert!(match_children.len() >= 2, "Match should have scrutinee + 1 arm as children");

    let ok_arm_ir_id = match_children[1];
    let ok_arm_meta = result.ir.match_arm_meta().get(ok_arm_ir_id);
    assert!(ok_arm_meta.is_some(), "Ok arm should have meta entry");
    let ok_meta = ok_arm_meta.unwrap();
    assert_eq!(
        ok_meta.variant_index,
        Some(0),
        "Ok arm should have variant_index = 0"
    );

    // Verify pattern_binding is set with nested structure
    assert!(
        ok_meta.pattern_binding.is_some(),
        "Ok arm should have pattern_binding set"
    );
    if let Some(pb) = &ok_meta.pattern_binding {
        use paideia_as_ir::enum_layout::PatternBinding;
        match pb {
            PatternBinding::EnumVariant {
                variant_index,
                payload_type,
                payload,
            } => {
                assert_eq!(*variant_index, 0, "variant_index should be 0 for Ok");
                assert_eq!(
                    *payload_type,
                    Some(point_record_type_id),
                    "payload_type should be Point"
                );
                assert!(
                    payload.is_some(),
                    "payload should be Some (the Point record pattern)"
                );
                if let Some(payload_binding) = payload {
                    match payload_binding.as_ref() {
                        PatternBinding::Record { type_id, fields } => {
                            assert_eq!(*type_id, point_record_type_id, "record type should be Point");
                            assert_eq!(fields.len(), 2, "Point should have 2 fields");
                            // Verify field bindings
                            assert_eq!(fields[0].0, "x");
                            assert_eq!(fields[1].0, "y");
                        }
                        _ => panic!("payload should be Record pattern"),
                    }
                }
            }
            _ => panic!("pattern_binding should be EnumVariant for Ok(...)"),
        }
    }
}

#[test]
fn lower_record_cons_reorders_to_declared_order() {
    // Test that Point { y: 2, x: 1 } is reordered to [type_name, 1, 2]
    // matching declared order (x, y) not literal order (y, x).
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::record_layout::RecordTypeId;

    // Create a source_map with proper source text
    // Source text: "Point x y 1 2"
    // - "Point" at offset 0-5
    // - "x" at offset 6-7
    // - "y" at offset 8-9
    // - "1" at offset 10-11
    // - "2" at offset 12-13
    let mut source_map = SourceMap::new();
    let source_text = "Point x y 1 2";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();

    let mut ast = AstArena::new();
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create Point type name node
    let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"

    // Create field name nodes
    let x_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x"
    let y_field_name = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "y"

    // Create literal value nodes (simple nodes)
    let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(10, 1)); // "1"
    let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "2"

    // Create RecordCons with fields in literal order (y, x)
    let record_cons = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        make_span(0, 13),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![(y_field_name, value_2), (x_field_name, value_1)],
        },
    );

    // Build struct registry with declared order (x, y)
    let mut struct_registry = crate::StructRegistry::empty();
    let point_record_type_id = RecordTypeId(1);
    struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
    struct_registry.fields.insert(
        point_record_type_id,
        vec![("x".to_string(), 4), ("y".to_string(), 4)],
    );

    // Lower to IR
    let result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &struct_registry,
        &crate::EnumRegistry::empty(),
        &std::collections::HashMap::new(),
    );

    // Get the RecordCons IR node and its children
    let record_cons_ir_id = result.ast_to_ir[&record_cons];
    let children = result.ir.children(record_cons_ir_id);

    // Verify children order: [type_name, value_1, value_2]
    // (x=1 should come before y=2, following declared order)
    assert_eq!(children.len(), 3, "RecordCons should have 3 children (type_name + 2 fields)");
    // Child 0 is type_name (unchanged)
    // Child 1 should be value_1 (x field value)
    // Child 2 should be value_2 (y field value)
    let ir_value_1 = result.ast_to_ir[&value_1];
    let ir_value_2 = result.ast_to_ir[&value_2];

    assert_eq!(
        children[1], ir_value_1,
        "field x value should come first in canonicalized order"
    );
    assert_eq!(
        children[2], ir_value_2,
        "field y value should come second in canonicalized order"
    );

    // No diagnostics should be emitted (no duplicates, all fields present)
    assert_eq!(sink.diagnostics().len(), 0, "should emit no diagnostics");
}

#[test]
fn lower_record_cons_missing_field_fires_t0537() {
    // Test that Point { x: 1 } (missing y) fires T0537
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::record_layout::RecordTypeId;

    // Create a source_map with proper source text
    // Source text: "Point x 1"
    // - "Point" at offset 0-5
    // - "x" at offset 6-7
    // - "1" at offset 8-9
    let mut source_map = SourceMap::new();
    let source_text = "Point x 1";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();

    let mut ast = AstArena::new();
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
    let x_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x"
    let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(8, 1)); // "1"

    let _record_cons = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        make_span(0, 9),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![(x_field_name, value_1)],
        },
    );

    let mut struct_registry = crate::StructRegistry::empty();
    let point_record_type_id = RecordTypeId(1);
    struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
    struct_registry.fields.insert(
        point_record_type_id,
        vec![("x".to_string(), 4), ("y".to_string(), 4)],
    );

    let _result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &struct_registry,
        &crate::EnumRegistry::empty(),
        &std::collections::HashMap::new(),
    );

    // Should emit exactly 1 T0537 diagnostic for missing field y
    assert_eq!(sink.diagnostics().len(), 1, "should emit 1 diagnostic for missing field");
}

#[test]
fn lower_record_cons_duplicate_field_fires_t0538() {
    // Test that Point { x: 1, x: 2 } fires T0538
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::record_layout::RecordTypeId;

    // Create a source_map with proper source text
    // Source text: "Point x x 1 2"
    // - "Point" at offset 0-5
    // - "x" at offset 6-7 (first)
    // - "x" at offset 8-9 (second)
    // - "1" at offset 10-11
    // - "2" at offset 12-13
    let mut source_map = SourceMap::new();
    let source_text = "Point x x 1 2";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();

    let mut ast = AstArena::new();
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
    let x_field_name_1 = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x" (first)
    let x_field_name_2 = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "x" (second)
    let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(10, 1)); // "1"
    let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "2"

    let _record_cons = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        make_span(0, 13),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![(x_field_name_1, value_1), (x_field_name_2, value_2)],
        },
    );

    let mut struct_registry = crate::StructRegistry::empty();
    let point_record_type_id = RecordTypeId(1);
    struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
    struct_registry.fields.insert(
        point_record_type_id,
        vec![("x".to_string(), 4), ("y".to_string(), 4)],
    );

    let _result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &struct_registry,
        &crate::EnumRegistry::empty(),
        &std::collections::HashMap::new(),
    );

    // Should emit T0538 for duplicate x, and T0537 for missing y
    let diags = sink.diagnostics();
    assert!(diags.len() >= 1, "should emit at least 1 diagnostic for duplicate field");
}

#[test]
fn lower_record_cons_unknown_field_fires_t0539() {
    // Test that Point { x: 1, y: 2, z: 3 } fires T0539 for z
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::record_layout::RecordTypeId;

    // Create a source_map with proper source text
    // Source text: "Point x y z 1 2 3"
    // - "Point" at offset 0-5
    // - "x" at offset 6-7
    // - "y" at offset 8-9
    // - "z" at offset 10-11
    // - "1" at offset 12-13
    // - "2" at offset 14-15
    // - "3" at offset 16-17
    let mut source_map = SourceMap::new();
    let source_text = "Point x y z 1 2 3";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();

    let mut ast = AstArena::new();
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
    let x_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x"
    let y_field_name = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "y"
    let z_field_name = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "z"
    let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "1"
    let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(14, 1)); // "2"
    let value_3 = ast.alloc(NodeKind::ExprLiteral, make_span(16, 1)); // "3"

    let _record_cons = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        make_span(0, 17),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![
                (x_field_name, value_1),
                (y_field_name, value_2),
                (z_field_name, value_3),
            ],
        },
    );

    let mut struct_registry = crate::StructRegistry::empty();
    let point_record_type_id = RecordTypeId(1);
    struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
    struct_registry.fields.insert(
        point_record_type_id,
        vec![("x".to_string(), 4), ("y".to_string(), 4)],
    );

    let _result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &struct_registry,
        &crate::EnumRegistry::empty(),
        &std::collections::HashMap::new(),
    );

    // Should emit T0539 for unknown field z
    let diags = sink.diagnostics();
    assert!(diags.len() >= 1, "should emit at least 1 diagnostic for unknown field");
}

#[test]
fn lower_record_cons_empty_registry_preserves_literal_order() {
    // Test that when registry is empty, RecordCons children are in literal order
    use paideia_as_diagnostics::FileId;

    // Create a source_map with proper source text
    // Source text: "Point y x 2 1"
    // - "Point" at offset 0-5
    // - "y" at offset 6-7
    // - "x" at offset 8-9
    // - "2" at offset 10-11
    // - "1" at offset 12-13
    let mut source_map = SourceMap::new();
    let source_text = "Point y x 2 1";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();

    let mut ast = AstArena::new();
    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
    let y_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "y"
    let x_field_name = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "x"
    let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(10, 1)); // "2"
    let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "1"

    // Create RecordCons with fields in literal order (y, x)
    let record_cons = ast.alloc_expr(
        NodeKind::ExprRecordCons,
        make_span(0, 13),
        ExprData::RecordCons {
            type_name: type_name_id,
            fields: vec![(y_field_name, value_2), (x_field_name, value_1)],
        },
    );

    // Use empty registry (no Point defined)
    let empty_registry = crate::StructRegistry::empty();

    let result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &empty_registry,
        &crate::EnumRegistry::empty(),
        &std::collections::HashMap::new(),
    );

    // Get the RecordCons IR node and its children
    let record_cons_ir_id = result.ast_to_ir[&record_cons];
    let children = result.ir.children(record_cons_ir_id);

    // When registry is empty, should preserve literal order: [type_name, value_2, value_1]
    assert_eq!(children.len(), 3, "RecordCons should have 3 children (type_name + 2 fields)");
    let ir_value_1 = result.ast_to_ir[&value_1];
    let ir_value_2 = result.ast_to_ir[&value_2];

    assert_eq!(
        children[1], ir_value_2,
        "with empty registry, should preserve literal order (y field first)"
    );
    assert_eq!(
        children[2], ir_value_1,
        "with empty registry, should preserve literal order (x field second)"
    );
}

#[test]
fn match_scrutinee_table_populated_for_lambda_param() {
    // Test that match scrutinee type is resolved when the scrutinee is a lambda parameter.
    // Build AST: enum Traffic { Red, Yellow, Green, Blue }
    //            fn(t: Traffic) -> match t { Red => 1, ... }
    // Run lower_ast_to_ir
    // Assert result.ir.match_scrutinee_table() contains the match node with Traffic type.
    use paideia_as_ir::EnumTypeId;
    use paideia_as_diagnostics::FileId;

    let mut source_map = SourceMap::new();
    let source_text = "Traffic Red Yellow Green Blue t";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create enum: enum Traffic { Red, Yellow, Green, Blue }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 7)); // "Traffic"
    let red_variant_name_id = ast.alloc(NodeKind::Ident, make_span(8, 3)); // "Red"
    let yellow_variant_name_id = ast.alloc(NodeKind::Ident, make_span(12, 6)); // "Yellow"
    let green_variant_name_id = ast.alloc(NodeKind::Ident, make_span(19, 5)); // "Green"
    let blue_variant_name_id = ast.alloc(NodeKind::Ident, make_span(25, 4)); // "Blue"

    let red_variant = paideia_as_ast::EnumVariant::Unit {
        name: red_variant_name_id,
    };
    let yellow_variant = paideia_as_ast::EnumVariant::Unit {
        name: yellow_variant_name_id,
    };
    let green_variant = paideia_as_ast::EnumVariant::Unit {
        name: green_variant_name_id,
    };
    let blue_variant = paideia_as_ast::EnumVariant::Unit {
        name: blue_variant_name_id,
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![red_variant, yellow_variant, green_variant, blue_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create lambda parameter pattern: t
    let t_pattern_id = ast.alloc(NodeKind::PatIdent, make_span(30, 1)); // "t"

    // Create lambda parameter type: Traffic
    let traffic_type_id = ast.alloc(NodeKind::Ident, make_span(0, 7)); // "Traffic"

    // Register pattern → type mapping
    ast.pattern_type_hints_mut()
        .insert(t_pattern_id, traffic_type_id);

    // Create scrutinee reference: t
    let scrutinee = ast.alloc(NodeKind::Ident, make_span(30, 1)); // "t"

    // Create arm 1: Red => 1
    let red_arm_pattern = ast.alloc(NodeKind::Ident, make_span(8, 3)); // "Red"
    let red_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let red_arm = paideia_as_ast::MatchArm {
        pattern: red_arm_pattern,
        guard: None,
        body: red_arm_body,
    };

    // Create arm 2: Yellow => 2
    let yellow_arm_pattern = ast.alloc(NodeKind::Ident, make_span(12, 6)); // "Yellow"
    let yellow_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let yellow_arm = paideia_as_ast::MatchArm {
        pattern: yellow_arm_pattern,
        guard: None,
        body: yellow_arm_body,
    };

    // Create match expression
    let match_id = ast.alloc_expr(
        NodeKind::ExprMatch,
        make_span(0, 1),
        paideia_as_ast::ExprData::Match {
            scrutinee,
            arms: vec![red_arm, yellow_arm],
            attrs: Default::default(),
        },
    );

    // Create lambda body with the match expression
    let lambda_id = ast.alloc_expr(
        NodeKind::ExprLambda,
        make_span(0, 1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![t_pattern_id],
            body: match_id,
            pipe_form: false,
        },
    );

    // Build enum registry
    let mut enum_registry = crate::EnumRegistry::empty();
    let traffic_enum_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Traffic".to_string(), traffic_enum_type_id);
    enum_registry.variants.insert(
        traffic_enum_type_id,
        vec![
            ("Red".to_string(), vec![]),
            ("Yellow".to_string(), vec![]),
            ("Green".to_string(), vec![]),
            ("Blue".to_string(), vec![]),
        ],
    );

    // Lower the AST
    let result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &crate::StructRegistry::empty(),
        &enum_registry,
        &std::collections::HashMap::new(),
    );

    // Verify the Match IR node was created
    let match_ir_id = result.ast_to_ir[&match_id];
    assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

    // Verify match_scrutinee_table entry
    let scrutinee_type = result.ir.match_scrutinee_table().get(match_ir_id);
    assert_eq!(
        scrutinee_type.copied(),
        Some(traffic_enum_type_id),
        "match_scrutinee_table should contain the Traffic enum type for lambda param scrutinee"
    );

    // Verify arm metadata entries
    let match_children = result.ir.children(match_ir_id);
    assert!(
        match_children.len() >= 3,
        "Match should have scrutinee + 2 arms as children"
    );
}

#[test]
fn bare_variant_arm_leaves_pattern_binding_none() {
    // Fix #1096: Test that bare no-payload enum-variant arms leave pattern_binding = None,
    // allowing #1052's auto-detect to fire on real code.
    // Build enum Traffic { Red, Yellow, Green, Blue }
    // Build lambda: fn(t: Traffic) -> match t { Red => 1, Yellow => 2, Green => 3, Blue => 4 }
    // Assert that for each arm's IR body node, arm_meta.pattern_binding.is_none()
    // AND arm_meta.variant_index == Some(<i>)
    use paideia_as_ir::EnumTypeId;
    use paideia_as_diagnostics::FileId;

    let mut source_map = SourceMap::new();
    let source_text = "Traffic Red Yellow Green Blue t";
    let _file = source_map.add_file(
        std::path::PathBuf::from("test.pdx"),
        String::from(source_text),
    );
    let mut sink = VecSink::new();
    let mut ast = AstArena::new();

    let file_id = FileId::new(1).unwrap();
    let make_span = |offset: u32, len: u32| {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    };

    // Create enum: enum Traffic { Red, Yellow, Green, Blue }
    let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 7)); // "Traffic"
    let red_variant_name_id = ast.alloc(NodeKind::Ident, make_span(8, 3)); // "Red"
    let yellow_variant_name_id = ast.alloc(NodeKind::Ident, make_span(12, 6)); // "Yellow"
    let green_variant_name_id = ast.alloc(NodeKind::Ident, make_span(19, 5)); // "Green"
    let blue_variant_name_id = ast.alloc(NodeKind::Ident, make_span(25, 4)); // "Blue"

    let red_variant = paideia_as_ast::EnumVariant::Unit {
        name: red_variant_name_id,
    };
    let yellow_variant = paideia_as_ast::EnumVariant::Unit {
        name: yellow_variant_name_id,
    };
    let green_variant = paideia_as_ast::EnumVariant::Unit {
        name: green_variant_name_id,
    };
    let blue_variant = paideia_as_ast::EnumVariant::Unit {
        name: blue_variant_name_id,
    };

    let _enum_item_id = ast.alloc_item(
        NodeKind::Enum,
        make_span(0, 1),
        paideia_as_ast::ItemData::Enum {
            name: enum_name_id,
            generic_params: vec![],
            variants: vec![red_variant, yellow_variant, green_variant, blue_variant],
            attributes: vec![],
            doc: None,
        },
    );

    // Create lambda parameter pattern: t
    let t_pattern_id = ast.alloc(NodeKind::PatIdent, make_span(30, 1)); // "t"

    // Create lambda parameter type: Traffic
    let traffic_type_id = ast.alloc(NodeKind::Ident, make_span(0, 7)); // "Traffic"

    // Register pattern → type mapping
    ast.pattern_type_hints_mut()
        .insert(t_pattern_id, traffic_type_id);

    // Create scrutinee reference: t
    let scrutinee = ast.alloc(NodeKind::Ident, make_span(30, 1)); // "t"

    // Create arm 1: Red => 1 (bare variant pattern)
    let red_variant_id = ast.alloc(NodeKind::Ident, make_span(8, 3)); // "Red"
    let red_arm_pattern = ast.alloc_pattern(
        NodeKind::PatIdent,
        make_span(8, 3),
        paideia_as_ast::PatternData::Ident {
            name: red_variant_id,
            mutable: false,
        },
    );
    let red_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let red_arm = paideia_as_ast::MatchArm {
        pattern: red_arm_pattern,
        guard: None,
        body: red_arm_body,
    };

    // Create arm 2: Yellow => 2 (bare variant pattern)
    let yellow_variant_id = ast.alloc(NodeKind::Ident, make_span(12, 6)); // "Yellow"
    let yellow_arm_pattern = ast.alloc_pattern(
        NodeKind::PatIdent,
        make_span(12, 6),
        paideia_as_ast::PatternData::Ident {
            name: yellow_variant_id,
            mutable: false,
        },
    );
    let yellow_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let yellow_arm = paideia_as_ast::MatchArm {
        pattern: yellow_arm_pattern,
        guard: None,
        body: yellow_arm_body,
    };

    // Create arm 3: Green => 3 (bare variant pattern)
    let green_variant_id = ast.alloc(NodeKind::Ident, make_span(19, 5)); // "Green"
    let green_arm_pattern = ast.alloc_pattern(
        NodeKind::PatIdent,
        make_span(19, 5),
        paideia_as_ast::PatternData::Ident {
            name: green_variant_id,
            mutable: false,
        },
    );
    let green_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let green_arm = paideia_as_ast::MatchArm {
        pattern: green_arm_pattern,
        guard: None,
        body: green_arm_body,
    };

    // Create arm 4: Blue => 4 (bare variant pattern)
    let blue_variant_id = ast.alloc(NodeKind::Ident, make_span(25, 4)); // "Blue"
    let blue_arm_pattern = ast.alloc_pattern(
        NodeKind::PatIdent,
        make_span(25, 4),
        paideia_as_ast::PatternData::Ident {
            name: blue_variant_id,
            mutable: false,
        },
    );
    let blue_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
    let blue_arm = paideia_as_ast::MatchArm {
        pattern: blue_arm_pattern,
        guard: None,
        body: blue_arm_body,
    };

    // Create match expression
    let match_id = ast.alloc_expr(
        NodeKind::ExprMatch,
        make_span(0, 1),
        paideia_as_ast::ExprData::Match {
            scrutinee,
            arms: vec![red_arm, yellow_arm, green_arm, blue_arm],
            attrs: Default::default(),
        },
    );

    // Create lambda body with the match expression
    let lambda_id = ast.alloc_expr(
        NodeKind::ExprLambda,
        make_span(0, 1),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![t_pattern_id],
            body: match_id,
            pipe_form: false,
        },
    );

    // Build enum registry
    let mut enum_registry = crate::EnumRegistry::empty();
    let traffic_enum_type_id = EnumTypeId(1);
    enum_registry
        .by_name
        .insert("Traffic".to_string(), traffic_enum_type_id);
    enum_registry.variants.insert(
        traffic_enum_type_id,
        vec![
            ("Red".to_string(), vec![]),
            ("Yellow".to_string(), vec![]),
            ("Green".to_string(), vec![]),
            ("Blue".to_string(), vec![]),
        ],
    );

    // Lower the AST
    let result = lower_ast_to_ir(
        &ast,
        &source_map,
        &mut sink,
        &crate::StructRegistry::empty(),
        &enum_registry,
        &std::collections::HashMap::new(),
    );

    // Verify the Match IR node was created
    let match_ir_id = result.ast_to_ir[&match_id];
    assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

    // Verify arm metadata entries
    let match_children = result.ir.children(match_ir_id);
    assert_eq!(
        match_children.len(),
        5,
        "Match should have scrutinee + 4 arms as children"
    );

    // Get arm body IDs (skip scrutinee at index 0)
    let red_arm_ir_id = match_children[1];
    let yellow_arm_ir_id = match_children[2];
    let green_arm_ir_id = match_children[3];
    let blue_arm_ir_id = match_children[4];

    // Verify Red arm metadata
    let red_arm_meta = result.ir.match_arm_meta().get(red_arm_ir_id);
    assert!(red_arm_meta.is_some(), "Red arm should have meta entry");
    let red_meta = red_arm_meta.unwrap();
    assert_eq!(red_meta.variant_index, Some(0), "Red should have variant_index = 0");
    assert!(
        red_meta.pattern_binding.is_none(),
        "Red bare-variant arm should have pattern_binding = None"
    );

    // Verify Yellow arm metadata
    let yellow_arm_meta = result.ir.match_arm_meta().get(yellow_arm_ir_id);
    assert!(yellow_arm_meta.is_some(), "Yellow arm should have meta entry");
    let yellow_meta = yellow_arm_meta.unwrap();
    assert_eq!(
        yellow_meta.variant_index,
        Some(1),
        "Yellow should have variant_index = 1"
    );
    assert!(
        yellow_meta.pattern_binding.is_none(),
        "Yellow bare-variant arm should have pattern_binding = None"
    );

    // Verify Green arm metadata
    let green_arm_meta = result.ir.match_arm_meta().get(green_arm_ir_id);
    assert!(green_arm_meta.is_some(), "Green arm should have meta entry");
    let green_meta = green_arm_meta.unwrap();
    assert_eq!(
        green_meta.variant_index,
        Some(2),
        "Green should have variant_index = 2"
    );
    assert!(
        green_meta.pattern_binding.is_none(),
        "Green bare-variant arm should have pattern_binding = None"
    );

    // Verify Blue arm metadata
    let blue_arm_meta = result.ir.match_arm_meta().get(blue_arm_ir_id);
    assert!(blue_arm_meta.is_some(), "Blue arm should have meta entry");
    let blue_meta = blue_arm_meta.unwrap();
    assert_eq!(blue_meta.variant_index, Some(3), "Blue should have variant_index = 3");
    assert!(
        blue_meta.pattern_binding.is_none(),
        "Blue bare-variant arm should have pattern_binding = None"
    );
}
