//! Tests for symbol memory operand parsing (v15-m6-001a).
//!
//! Verifies that UnsafeWalker correctly handles memory operands with symbol references
//! and optional integer addends: [symbol], [symbol + N], [symbol - N].

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::{SourceMap, Span, VecSink};
use paideia_as_elaborator::{LocalBindingTable, unsafe_walker::UnsafeWalker};
use paideia_as_ir::instruction::Operand;
use paideia_as_ir::{InstrMode, IrArena};
use std::collections::HashMap;
use std::path::PathBuf;

/// Helper to create a test span.
fn test_span() -> Span {
    Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
}

/// Build an AST node for an identifier at specific source position.
fn build_ident_at(ast: &mut AstArena, source_pos: u32, source_len: u32) -> NodeId {
    let span = Span::new(
        paideia_as_diagnostics::FileId::new(1).unwrap(),
        source_pos,
        source_len,
    );
    ast.alloc(NodeKind::Ident, span)
}

/// Build an AST node for an integer literal at specific source position.
fn build_intlit_at(ast: &mut AstArena, source_pos: u32, source_len: u32) -> NodeId {
    let span = Span::new(
        paideia_as_diagnostics::FileId::new(1).unwrap(),
        source_pos,
        source_len,
    );
    let placeholder = ast.alloc(NodeKind::Placeholder, span);
    ast.alloc_expr(
        NodeKind::ExprLiteral,
        span,
        ExprData::Literal { lit: placeholder },
    )
}

/// Build an AST node for a binary infix expression.
fn build_infix(
    ast: &mut AstArena,
    lhs: NodeId,
    op_pos: u32,
    op: &str,
    rhs: NodeId,
    total_span: Span,
) -> NodeId {
    // Create operator node with appropriate span pointing at the actual operator in source
    let op_span = Span::new(total_span.file(), op_pos, op.len() as u32);
    let op_node = ast.alloc(NodeKind::Placeholder, op_span);

    ast.alloc_expr(
        NodeKind::ExprInfix,
        total_span,
        ExprData::Infix {
            op: op_node,
            lhs,
            rhs,
        },
    )
}

/// Build a memory reference operand (OperandMemoryRef).
fn build_memref(ast: &mut AstArena, addr: NodeId, span: Span) -> NodeId {
    ast.alloc_expr(
        NodeKind::OperandMemoryRef,
        span,
        ExprData::OperandMemoryRef { addr },
    )
}

/// Helper to run a single `mov [symbol], 0` instruction through UnsafeWalker
/// and return the resulting operand.
fn mov_symbol_memory_operand(source: &str, ast: &mut AstArena, memref_operand: NodeId) -> Operand {
    let mut ir = IrArena::new();

    let stmt_span = test_span();

    let justification = ast.alloc(NodeKind::ExprString, stmt_span);

    // Immediate operand `0`: ExprLiteral
    let lit_placeholder = ast.alloc(NodeKind::Placeholder, stmt_span);
    let imm_operand = ast.alloc_expr(
        NodeKind::ExprLiteral,
        stmt_span,
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    let mnemonic_id = ast.intern_mnemonic("mov");
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        stmt_span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![memref_operand, imm_operand],
        },
    );

    let _unsafe_expr = ast.alloc_expr(
        NodeKind::ExprUnsafe,
        stmt_span,
        ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification,
            block: vec![inst_stmt],
        },
    );

    let ir_unsafe = ir.alloc(paideia_as_ir::IrKind::Unsafe, stmt_span);

    let mut source_map = SourceMap::new();
    let _ = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());

    let mut sink = VecSink::new();
    let record_layouts = HashMap::new();
    let local_bindings = LocalBindingTable::new();
    let (_unsafe_labels, _label_to_instr, _first_instrs, _diags) = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
        InstrMode::Mode64,
    );

    assert_eq!(
        ir.instructions().len(),
        1,
        "expected exactly one instruction"
    );
    ir.instructions()
        .entries()
        .values()
        .next()
        .expect("instruction present")
        .operands[0]
        .clone()
}

/// Test: [pml4] → SymbolRef { name: "pml4", addend: 0 }
#[test]
fn bare_symbol_operand() {
    let mut ast = AstArena::new();

    // Source: `mov [pml4], 0`
    // Position of "pml4": 5-8
    let pml4_ident = build_ident_at(&mut ast, 5, 4);
    let memref = build_memref(&mut ast, pml4_ident, test_span());

    let source = "mov [pml4], 0";
    let operand = mov_symbol_memory_operand(source, &mut ast, memref);

    match operand {
        Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "pml4");
            assert_eq!(addend, 0);
        }
        other => panic!("Expected SymbolRef, got {:?}", other),
    }
}

/// Test: [pml4 + 4] → SymbolRef { name: "pml4", addend: 4 }
#[test]
fn symbol_plus_intlit_operand() {
    let mut ast = AstArena::new();

    // Source: `mov [pml4 + 4], 0`
    // Position of "pml4": 5-8
    // Position of "+": 10
    // Position of "4": 12
    let pml4_ident = build_ident_at(&mut ast, 5, 4);
    let intlit_4 = build_intlit_at(&mut ast, 12, 1);

    let span = Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 5, 8);
    let infix = build_infix(&mut ast, pml4_ident, 10, "+", intlit_4, span);
    let memref = build_memref(&mut ast, infix, test_span());

    let source = "mov [pml4 + 4], 0";
    let operand = mov_symbol_memory_operand(source, &mut ast, memref);

    match operand {
        Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "pml4");
            assert_eq!(addend, 4);
        }
        other => panic!("Expected SymbolRef with addend 4, got {:?}", other),
    }
}

/// Test: [pdpt - 16] → SymbolRef { name: "pdpt", addend: -16 }
#[test]
fn symbol_minus_intlit_operand() {
    let mut ast = AstArena::new();

    // Source: `mov [pdpt - 16], 0`
    // Position of "pdpt": 5-8
    // Position of "-": 10
    // Position of "16": 12-13
    let pdpt_ident = build_ident_at(&mut ast, 5, 4);
    let intlit_16 = build_intlit_at(&mut ast, 12, 2);

    let span = Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 5, 8);
    let infix = build_infix(&mut ast, pdpt_ident, 10, "-", intlit_16, span);
    let memref = build_memref(&mut ast, infix, test_span());

    let source = "mov [pdpt - 16], 0";
    let operand = mov_symbol_memory_operand(source, &mut ast, memref);

    match operand {
        Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "pdpt");
            assert_eq!(addend, -16);
        }
        other => panic!("Expected SymbolRef with addend -16, got {:?}", other),
    }
}

/// Test: Commutative form [8 + pml4] → SymbolRef { name: "pml4", addend: 8 }
#[test]
fn intlit_plus_symbol_operand() {
    let mut ast = AstArena::new();

    // Source: `mov [8 + pml4], 0`
    // Position of "8": 5
    // Position of "+": 7
    // Position of "pml4": 9-12
    let intlit_8 = build_intlit_at(&mut ast, 5, 1);
    let pml4_ident = build_ident_at(&mut ast, 9, 4);

    let span = Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 5, 8);
    let infix = build_infix(&mut ast, intlit_8, 7, "+", pml4_ident, span);
    let memref = build_memref(&mut ast, infix, test_span());

    let source = "mov [8 + pml4], 0";
    let operand = mov_symbol_memory_operand(source, &mut ast, memref);

    match operand {
        Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "pml4");
            assert_eq!(addend, 8);
        }
        other => panic!("Expected SymbolRef with addend 8, got {:?}", other),
    }
}
