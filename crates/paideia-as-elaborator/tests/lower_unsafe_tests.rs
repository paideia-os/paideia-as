//! Tests for lowering ExprData::Unsafe to IR with RawInstruction children.
//!
//! Phase 8 m4-001: verify that lower.rs walks the unsafe block's statement
//! list and emits each statement as an IR child of the Unsafe node.

use paideia_as_ast::{AstArena, ExprData, NodeKind, StmtData};
use paideia_as_diagnostics::Span;
use paideia_as_elaborator::lower::lower_ast_to_ir;
use paideia_as_ir::{IrKind, InstrMode};

fn test_span() -> Span {
    Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
}

/// Test that ExprData::Unsafe block body is lowered into IR children.
#[test]
fn unsafe_block_lowers_body_to_children() {
    let mut ast = AstArena::new();
    let span = test_span();

    // Create a simple mnemonic
    let mnemonic_id = ast.intern_mnemonic("cli");

    // Create an instruction: cli (no operands)
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![],
        },
    );

    // Create justification
    let justification = ast.alloc(NodeKind::ExprString, span);

    // Create unsafe expression with the instruction in its block
    let unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![inst_stmt],
        },
    );

    // Lower to IR
    let result = lower_ast_to_ir(&ast);

    // Find the Unsafe IR node
    let unsafe_ir_id = result.ast_to_ir[&unsafe_expr];
    let unsafe_ir = result.ir.get(unsafe_ir_id).expect("unsafe IR node exists");

    // Verify the IR node is IrKind::Unsafe
    assert_eq!(unsafe_ir.kind, IrKind::Unsafe);

    // Verify it has children (the instruction)
    let children = result.ir.children(unsafe_ir_id);
    assert_eq!(
        children.len(),
        1,
        "Unsafe should have one child (the instruction)"
    );

    // Verify the child is a RawInstruction
    let inst_ir = result
        .ir
        .get(children[0])
        .expect("instruction IR node exists");
    assert_eq!(inst_ir.kind, IrKind::RawInstruction);
}

/// Test unsafe block with multiple instructions lowers all to children.
#[test]
fn unsafe_block_with_three_stmts_lowers_all() {
    let mut ast = AstArena::new();
    let span = test_span();

    // Create three instructions: cli, hlt, nop
    let cli_mnem = ast.intern_mnemonic("cli");
    let hlt_mnem = ast.intern_mnemonic("hlt");
    let nop_mnem = ast.intern_mnemonic("nop");

    let cli_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: cli_mnem,
            operands: vec![],
        },
    );

    let hlt_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: hlt_mnem,
            operands: vec![],
        },
    );

    let nop_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: nop_mnem,
            operands: vec![],
        },
    );

    // Create unsafe expression with three instructions
    let justification = ast.alloc(NodeKind::ExprString, span);
    let unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![cli_stmt, hlt_stmt, nop_stmt],
        },
    );

    // Lower to IR
    let result = lower_ast_to_ir(&ast);

    // Find the Unsafe IR node
    let unsafe_ir_id = result.ast_to_ir[&unsafe_expr];
    let children = result.ir.children(unsafe_ir_id);

    // Verify all three instructions are children
    assert_eq!(
        children.len(),
        3,
        "Unsafe should have three children (three instructions)"
    );

    // Verify each child is a RawInstruction
    for (i, &child_id) in children.iter().enumerate() {
        let child_ir = result.ir.get(child_id).expect("instruction IR node exists");
        assert_eq!(
            child_ir.kind,
            IrKind::RawInstruction,
            "Child {} should be RawInstruction",
            i
        );
    }
}

/// Test that non-instruction statements in an unsafe block are still lowered.
#[test]
fn unsafe_block_with_mixed_stmts_lowers_all() {
    let mut ast = AstArena::new();
    let span = test_span();

    // Create: let x = 0; cli; nop
    let lit = ast.alloc(NodeKind::Placeholder, span);
    let lit_expr = ast.alloc_expr(NodeKind::ExprLiteral, span, ExprData::Literal { lit });

    let name_id = ast.alloc(NodeKind::Ident, span);
    let let_stmt = ast.alloc_stmt(
        NodeKind::StmtLet,
        span,
        StmtData::Let {
            name: name_id,
            ty: None,
            value: lit_expr,
            mutable: false,
        },
    );

    let cli_mnem = ast.intern_mnemonic("cli");
    let cli_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: cli_mnem,
            operands: vec![],
        },
    );

    let nop_mnem = ast.intern_mnemonic("nop");
    let nop_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: nop_mnem,
            operands: vec![],
        },
    );

    // Create unsafe expression
    let justification = ast.alloc(NodeKind::ExprString, span);
    let unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![let_stmt, cli_stmt, nop_stmt],
        },
    );

    // Lower to IR
    let result = lower_ast_to_ir(&ast);

    // Find the Unsafe IR node
    let unsafe_ir_id = result.ast_to_ir[&unsafe_expr];
    let children = result.ir.children(unsafe_ir_id);

    // Verify all three statements are children
    assert_eq!(
        children.len(),
        3,
        "Unsafe should have three children (let, cli, nop)"
    );

    // Verify kinds: Let, RawInstruction, RawInstruction
    let child0 = result.ir.get(children[0]).expect("child 0 exists");
    assert_eq!(child0.kind, IrKind::Let, "Child 0 should be Let");

    let child1 = result.ir.get(children[1]).expect("child 1 exists");
    assert_eq!(
        child1.kind,
        IrKind::RawInstruction,
        "Child 1 should be RawInstruction"
    );

    let child2 = result.ir.get(children[2]).expect("child 2 exists");
    assert_eq!(
        child2.kind,
        IrKind::RawInstruction,
        "Child 2 should be RawInstruction"
    );
}
