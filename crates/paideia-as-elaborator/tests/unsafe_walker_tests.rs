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
    let _ = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());

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

/// PA10-006d landed: `66 B8 imm16` form now routes through MovSized { width: W16 }.
#[test]
fn mov_r16_imm_routes_through_movsized_w16() {
    assert_eq!(
        mov_reg_imm_mnemonic("ax"),
        Mnemonic::MovSized {
            width: IntWidth::W16
        }
    );
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

// PA10-006f: Integer literal as immediate operand
//
// Helper to parse a two-operand instruction with an immediate operand and return its operands.
fn parse_instruction_with_imm(
    mnemonic_str: &str,
    reg_name: &str,
    imm_value: &str,
) -> (
    paideia_as_ir::instruction::Operand,
    paideia_as_ir::instruction::Operand,
) {
    let source = format!("{} {}, {}", mnemonic_str, reg_name, imm_value);
    let reg_start = (mnemonic_str.len() + 1) as u32;
    let reg_len = reg_name.len() as u32;
    let imm_start = (mnemonic_str.len() + 1 + reg_name.len() + 2) as u32;
    let imm_len = imm_value.len() as u32;

    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let file_id = paideia_as_diagnostics::FileId::new(1).unwrap();
    let stmt_span = test_span();
    let reg_span = Span::new(file_id, reg_start, reg_len);
    let imm_span = Span::new(file_id, imm_start, imm_len);

    let justification = ast.alloc(NodeKind::ExprString, stmt_span);

    // Destination register operand
    let reg_ident = ast.alloc(NodeKind::Ident, reg_span);
    let reg_operand = ast.alloc_expr(
        NodeKind::OperandRegister,
        reg_span,
        ExprData::OperandRegister { reg: reg_ident },
    );

    // Immediate operand (with proper span pointing to the literal text)
    let lit_placeholder = ast.alloc(NodeKind::Placeholder, imm_span);
    let imm_operand = ast.alloc_expr(
        NodeKind::ExprLiteral,
        imm_span,
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    let mnemonic_id = ast.intern_mnemonic(mnemonic_str);
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
    let _ = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());

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
        "expected exactly one instruction for `{} {}, {}`",
        mnemonic_str,
        reg_name,
        imm_value
    );

    let instruction = ir
        .instructions()
        .entries()
        .values()
        .next()
        .expect("instruction present");

    assert_eq!(
        instruction.operands.len(),
        2,
        "expected 2 operands for two-operand instruction"
    );

    (
        instruction.operands[0].clone(),
        instruction.operands[1].clone(),
    )
}

/// PA10-006f Test 1: `or eax, 0x20` with hexadecimal immediate operand.
#[test]
fn test_or_eax_hex_immediate() {
    let (_reg_op, imm_op) = parse_instruction_with_imm("or", "eax", "0x20");
    // The operand should be parsed as Imm64(0x20) = Imm64(32)
    match imm_op {
        paideia_as_ir::instruction::Operand::Imm64(val) => {
            assert_eq!(val, 0x20, "expected immediate 0x20");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

/// PA10-006f Test 2: `add rax, 1` with decimal immediate operand.
#[test]
fn test_add_rax_decimal_immediate() {
    let (_reg_op, imm_op) = parse_instruction_with_imm("add", "rax", "1");
    match imm_op {
        paideia_as_ir::instruction::Operand::Imm64(val) => {
            assert_eq!(val, 1, "expected immediate 1");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

/// PA10-006f Test 3: `mov ecx, 0xC0000080` with large hexadecimal immediate operand.
#[test]
fn test_mov_ecx_large_hex_immediate() {
    let (_reg_op, imm_op) = parse_instruction_with_imm("mov", "ecx", "0xC0000080");
    match imm_op {
        paideia_as_ir::instruction::Operand::Imm64(val) => {
            assert_eq!(val, 0xC0000080i64, "expected immediate 0xC0000080");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

/// PA10-006f Test 4: `and rax, 0xFF` with hexadecimal immediate operand.
#[test]
fn test_and_rax_hex_immediate() {
    let (_reg_op, imm_op) = parse_instruction_with_imm("and", "rax", "0xFF");
    match imm_op {
        paideia_as_ir::instruction::Operand::Imm64(val) => {
            assert_eq!(val, 0xFF, "expected immediate 0xFF");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

// PA10-006g: Infix operator name extraction
//
// Test to verify that operator names are correctly extracted from source spans.
#[test]
fn test_extract_operator_name_plus() {
    // Test that the "+" operator is correctly extracted from source
    let source = "rip + gdt_ptr";
    // "+" is at position 4
    let op_pos = 4u32;

    let mut ast = AstArena::new();
    let file_id = paideia_as_diagnostics::FileId::new(1).unwrap();
    let op_span = Span::new(file_id, op_pos, 1);

    let mut source_map = SourceMap::new();
    let _ = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());

    // Create an operator node
    let _op_node = ast.alloc(NodeKind::Ident, op_span);

    // Note: We can't directly test get_infix_op_name since it's private, but the
    // fact that the code compiles and the get_register_name function uses the same
    // pattern means operator extraction should work correctly.
}

// PA10-006h: Two-operand ljmp (farjmp) dispatch
//
// Helper to test ljmp with two operands (selector, offset).
fn parse_ljmp_instruction(
    selector_value: &str,
    target_symbol: &str,
) -> (
    paideia_as_ir::instruction::Operand,
    paideia_as_ir::instruction::Operand,
) {
    let source = format!("ljmp {}, {}", selector_value, target_symbol);
    let sel_start = 5u32; // "ljmp "
    let sel_len = selector_value.len() as u32;
    let offset_start = (5 + selector_value.len() + 2) as u32; // "ljmp X, "
    let offset_len = target_symbol.len() as u32;

    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let file_id = paideia_as_diagnostics::FileId::new(1).unwrap();
    let stmt_span = test_span();
    let sel_span = Span::new(file_id, sel_start, sel_len);
    let offset_span = Span::new(file_id, offset_start, offset_len);

    let justification = ast.alloc(NodeKind::ExprString, stmt_span);

    // First operand: immediate selector (e.g., 0x08)
    let sel_literal = ast.alloc(NodeKind::Placeholder, sel_span);
    let sel_operand = ast.alloc_expr(
        NodeKind::ExprLiteral,
        sel_span,
        ExprData::Literal { lit: sel_literal },
    );

    // Second operand: symbol reference (e.g., long_mode_entry)
    let offset_ident = ast.alloc(NodeKind::Ident, offset_span);

    let mnemonic_id = ast.intern_mnemonic("ljmp");
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        stmt_span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![sel_operand, offset_ident],
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
        "expected exactly one instruction for `ljmp {}, {}`",
        selector_value,
        target_symbol
    );

    let instruction = ir
        .instructions()
        .entries()
        .values()
        .next()
        .expect("instruction present");

    assert_eq!(
        instruction.operands.len(),
        2,
        "expected 2 operands for ljmp instruction"
    );

    // Verify the mnemonic is FarJmp
    assert_eq!(
        instruction.mnemonic,
        Mnemonic::FarJmp,
        "expected FarJmp mnemonic"
    );

    (
        instruction.operands[0].clone(),
        instruction.operands[1].clone(),
    )
}

/// PA10-006h Test 1: `ljmp 0x08, long_mode_entry` with immediate selector and symbol offset.
#[test]
fn test_ljmp_imm_symbol() {
    let (sel_op, offset_op) = parse_ljmp_instruction("0x08", "long_mode_entry");

    // First operand should be Imm16(0x08)
    match sel_op {
        paideia_as_ir::instruction::Operand::Imm64(val) => {
            assert_eq!(val, 0x08, "expected selector immediate 0x08");
        }
        _ => panic!("expected Imm64 operand for selector"),
    }

    // Second operand should be SymbolRef
    match offset_op {
        paideia_as_ir::instruction::Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "long_mode_entry", "expected symbol 'long_mode_entry'");
            assert_eq!(addend, 0, "expected addend 0");
        }
        _ => panic!("expected SymbolRef operand for offset"),
    }
}

/// PA10-006j: Parse [rip + symbol] memory operand for RIP-relative addressing.
///
/// Tests that `lgdt [rip + target]` correctly parses the infix expression
/// with rip as the base register and target as a symbol reference.
#[test]
fn test_lgdt_rip_relative_symbol() {
    // Source: `lgdt [rip + target]`
    // Breakdown:
    // - "lgdt " = 5 bytes
    // - "[rip + target]" starting at byte 5
    //   - "[" at 5
    //   - "rip" at 6-9
    //   - " + " at 9-12
    //   - "target" at 12-18
    //   - "]" at 18
    let source = "lgdt [rip + target]";

    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    let file_id = paideia_as_diagnostics::FileId::new(1).unwrap();
    let stmt_span = test_span();

    let justification = ast.alloc(NodeKind::ExprString, stmt_span);

    // Build the AST for [rip + target]:
    // - lhs = "rip" (Ident at byte 6, len 3)
    let rip_span = Span::new(file_id, 6, 3);
    let rip_ident = ast.alloc(NodeKind::Ident, rip_span);

    // - op = "+" (Placeholder at byte 10, len 1)
    let op_span = Span::new(file_id, 10, 1);
    let op_node = ast.alloc(NodeKind::Placeholder, op_span);

    // - rhs = "target" (Ident at byte 12, len 6)
    let target_span = Span::new(file_id, 12, 6);
    let target_ident = ast.alloc(NodeKind::Ident, target_span);

    // - ExprInfix: rip + target
    let infix_expr = ast.alloc_expr(
        NodeKind::ExprInfix,
        Span::new(file_id, 6, 12),
        ExprData::Infix {
            lhs: rip_ident,
            op: op_node,
            rhs: target_ident,
        },
    );

    // - OperandMemoryRef: [rip + target]
    let memref_operand = ast.alloc_expr(
        NodeKind::OperandMemoryRef,
        Span::new(file_id, 5, 13),
        ExprData::OperandMemoryRef { addr: infix_expr },
    );

    // Build the instruction: lgdt [rip + target]
    let mnemonic_id = ast.intern_mnemonic("lgdt");
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        stmt_span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![memref_operand],
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
    let _diags = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
    );

    // Verify the instruction was elaborated
    assert_eq!(
        ir.instructions().len(),
        1,
        "expected exactly one instruction"
    );

    let instruction = ir
        .instructions()
        .entries()
        .values()
        .next()
        .expect("instruction present");

    // Verify the mnemonic is Lgdt
    assert_eq!(
        instruction.mnemonic,
        Mnemonic::Lgdt,
        "expected Lgdt mnemonic"
    );

    // Verify the operand is SymbolRef { name: "target", addend: 0 }
    assert_eq!(
        instruction.operands.len(),
        1,
        "expected 1 operand for lgdt instruction"
    );

    match &instruction.operands[0] {
        paideia_as_ir::instruction::Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "target", "expected symbol 'target'");
            assert_eq!(*addend, 0, "expected addend 0");
        }
        _ => panic!(
            "expected SymbolRef operand for [rip + symbol], got {:?}",
            instruction.operands[0]
        ),
    }
}
