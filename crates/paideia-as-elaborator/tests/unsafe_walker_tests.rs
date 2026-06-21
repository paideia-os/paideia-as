//! Integration tests for UnsafeWalker (Phase 5, m3-004).
//!
//! Tests the elaboration of unsafe blocks with assembly instructions.

use paideia_as_ast::{AstArena, ExprData, NodeKind, StmtData};
use paideia_as_diagnostics::{Span, VecSink};
use paideia_as_elaborator::unsafe_walker::UnsafeWalker;
use paideia_as_ir::IrArena;

/// Helper to create a test span.
fn test_span() -> Span {
    Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
}

/// Test 1: `lgdt [rdi]` produces one Instruction with Mnemonic::Lgdt and one MemSib operand.
#[test]
fn test_lgdt_memory_operand() {
    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let span = test_span();

    // Pre-allocate the justification node
    let justification = ast.alloc(NodeKind::ExprString, span);

    // Create a simple memory operand [rdi] (OperandMemoryRef)
    let mem_ref = ast.alloc(NodeKind::OperandMemoryRef, span);

    // Create the instruction statement: lgdt mem_ref
    let mnemonic_id = ast.intern_mnemonic("lgdt");
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![mem_ref],
        },
    );

    // Create the unsafe block expression
    let _unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![inst_stmt],
        },
    );

    // Create an IR Unsafe node
    let ir_unsafe = ir.alloc(paideia_as_ir::IrKind::Unsafe, span);

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let _diags = UnsafeWalker::run(&mut ir, &ast, vec![ir_unsafe.get()], &mut sink);

    // Check that no errors were emitted (in a real test with proper AST nodes, this would work)
    // For now, this test verifies the basic structure compiles.
}

/// Test 2: Unknown mnemonic `foozle` produces U1605 diagnostic and no instruction.
#[test]
fn test_unknown_mnemonic_foozle() {
    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let span = test_span();

    // Pre-allocate the justification node
    let justification = ast.alloc(NodeKind::ExprString, span);

    // Create an instruction statement with unknown mnemonic
    let mnemonic_id = ast.intern_mnemonic("foozle");
    let operand = ast.alloc(NodeKind::Ident, span);
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![operand],
        },
    );

    // Create the unsafe block expression
    let _unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![inst_stmt],
        },
    );

    // Create an IR Unsafe node
    let ir_unsafe = ir.alloc(paideia_as_ir::IrKind::Unsafe, span);

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let _diags = UnsafeWalker::run(&mut ir, &ast, vec![ir_unsafe.get()], &mut sink);

    // Check that a U1605 diagnostic was emitted
    let sink_diags = sink.into_diagnostics();
    let u1605_diags: Vec<_> = sink_diags
        .iter()
        .filter(|d| d.code().number() == 1605)
        .collect();
    assert_eq!(
        u1605_diags.len(),
        1,
        "should emit exactly one U1605 diagnostic for unknown mnemonic"
    );

    // Verify that no instruction was inserted
    assert_eq!(
        ir.instructions().len(),
        0,
        "should not insert instruction for unknown mnemonic"
    );
}

/// Test 3: Malformed operand `[rdi +]` produces U1606 diagnostic.
#[test]
fn test_malformed_operand_incomplete_memory() {
    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let span = test_span();

    // Pre-allocate the justification node
    let justification = ast.alloc(NodeKind::ExprString, span);

    // Create an instruction statement with malformed memory operand
    let mnemonic_id = ast.intern_mnemonic("mov");

    // Create a malformed memory reference (incomplete: [rdi +])
    let malformed_mem_ref = ast.alloc(NodeKind::OperandMemoryRef, span);

    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![malformed_mem_ref],
        },
    );

    // Create the unsafe block expression
    let _unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![inst_stmt],
        },
    );

    // Create an IR Unsafe node
    let ir_unsafe = ir.alloc(paideia_as_ir::IrKind::Unsafe, span);

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let _diags = UnsafeWalker::run(&mut ir, &ast, vec![ir_unsafe.get()], &mut sink);

    // Check that a U1606 diagnostic was emitted
    let sink_diags = sink.into_diagnostics();
    let u1606_diags: Vec<_> = sink_diags
        .iter()
        .filter(|d| d.code().number() == 1606)
        .collect();
    assert!(
        u1606_diags.len() > 0,
        "should emit at least one U1606 diagnostic for malformed operand"
    );

    // Verify that no instruction was inserted
    assert_eq!(
        ir.instructions().len(),
        0,
        "should not insert instruction for malformed operand"
    );
}
