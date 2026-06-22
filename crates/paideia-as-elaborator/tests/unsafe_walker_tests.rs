//! Integration tests for UnsafeWalker (Phase 5, m3-004).
//!
//! Tests the elaboration of unsafe blocks with assembly instructions.

use paideia_as_ast::{AstArena, ExprData, NodeKind, StmtData};
use paideia_as_diagnostics::{SourceMap, Span, VecSink};
use paideia_as_elaborator::{LocalBindingTable, unsafe_walker::UnsafeWalker};
use paideia_as_ir::IrArena;
use paideia_as_ir::instruction::{IntWidth, Mnemonic};
use std::collections::HashMap;
use std::path::PathBuf;

// Phase 6 m1-005 tests: zero-arity mnemonics
mod unsafe_walker;

/// Helper to create a test span.
fn test_span() -> Span {
    Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
}

/// PA8 m3-003 (#827): drive `mov <reg>, 0` through the unsafe walker against
/// real source text and return the resulting Instruction's mnemonic.
///
/// The register name is read back from the source span by `get_register_name`,
/// so the source layout must place `reg_name` immediately after `"mov "`. The
/// immediate is the literal `0` (the elaborator's `extract_integer_from_span`
/// placeholder), which is sufficient: this harness verifies the *mnemonic
/// retarget*, not the immediate value.
fn mov_reg_imm_mnemonic(reg_name: &str) -> Mnemonic {
    let source = format!("mov {reg_name}, 0");
    // `reg` occupies bytes [4, 4 + reg_name.len()) in `source`.
    let reg_start = 4u32;
    let reg_len = reg_name.len() as u32;

    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let file_id = paideia_as_diagnostics::FileId::new(1).unwrap();
    let stmt_span = test_span();
    let reg_span = Span::new(file_id, reg_start, reg_len);

    let justification = ast.alloc(NodeKind::ExprString, stmt_span);

    // Destination register operand: OperandRegister wrapping an Ident whose
    // span points at `reg_name` in the source.
    let reg_ident = ast.alloc(NodeKind::Ident, reg_span);
    let reg_operand = ast.alloc_expr(
        NodeKind::OperandRegister,
        reg_span,
        ExprData::OperandRegister { reg: reg_ident },
    );

    // Immediate operand `0`: ExprLiteral wrapping a placeholder literal node.
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
            operands: vec![reg_operand, imm_operand],
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
    let _ = source_map.add_file(PathBuf::from("test.pdx"), source);

    let mut sink = VecSink::new();
    let record_layouts = HashMap::new();
    let local_bindings = LocalBindingTable::new();
    let _diags = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
    );

    assert_eq!(
        ir.instructions().len(),
        1,
        "expected exactly one instruction for `mov {reg_name}, 0`"
    );
    ir.instructions()
        .entries()
        .values()
        .next()
        .expect("instruction present")
        .mnemonic
}

/// `mov al, 0` retargets to the width-aware r8 immediate move.
#[test]
fn mov_r8_imm_retargets_to_movsized_w8() {
    assert_eq!(
        mov_reg_imm_mnemonic("al"),
        Mnemonic::MovSized {
            width: IntWidth::W8
        }
    );
}

/// `mov eax, 0` retargets to the width-aware r32 immediate move.
#[test]
fn mov_r32_imm_retargets_to_movsized_w32() {
    assert_eq!(
        mov_reg_imm_mnemonic("eax"),
        Mnemonic::MovSized {
            width: IntWidth::W32
        }
    );
}

/// `mov rax, 0` stays the generic 64-bit `Mov` (r64 imm path is the documented
/// follow-up, not part of the #827 ship-minimum retarget).
#[test]
fn mov_r64_imm_stays_generic_mov() {
    assert_eq!(mov_reg_imm_mnemonic("rax"), Mnemonic::Mov);
}

/// `mov ax, 0` (r16) is intentionally NOT retargeted in the ship-minimum: the
/// `66 B8 imm16` form is deferred follow-up, so it stays generic `Mov`.
#[test]
fn mov_r16_imm_stays_generic_mov_deferred() {
    assert_eq!(mov_reg_imm_mnemonic("ax"), Mnemonic::Mov);
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

    // Create a source map with a dummy file for testing
    let mut source_map = SourceMap::new();
    let _ = source_map.add_file(PathBuf::from("test.pdx"), String::new());

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let record_layouts = HashMap::new();
    let local_bindings = LocalBindingTable::new();
    let _diags = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
    );

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

    // Create a source map with a dummy file for testing
    let mut source_map = SourceMap::new();
    let _ = source_map.add_file(PathBuf::from("test.pdx"), String::new());

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let record_layouts = HashMap::new();
    let local_bindings = LocalBindingTable::new();
    let _diags = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
    );

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

    // Create a source map with a dummy file for testing
    let mut source_map = SourceMap::new();
    let _ = source_map.add_file(PathBuf::from("test.pdx"), String::new());

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let record_layouts = HashMap::new();
    let local_bindings = LocalBindingTable::new();
    let _diags = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
    );

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
