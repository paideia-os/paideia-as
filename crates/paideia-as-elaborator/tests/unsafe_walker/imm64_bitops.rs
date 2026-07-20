//! Integration tests for imm64 bitwise operation expansion (PA-R13-010).
//!
//! Tests verify that or/and/xor with 64-bit immediates that don't fit in i32
//! are correctly expanded to:
//!   movabs r11, imm64
//!   <op>   dst, r11
//!
//! The expansion only triggers when:
//! - Mnemonic ∈ {Or, And, Xor}
//! - Operands = [Reg(dst), Imm64(v)]
//! - Mode = Mode64
//! - needs_expansion(v) = true (signed 32-bit round-trip fails)
//!
//! Collision guard: if dst == r11, emit diagnostic E1710 and refuse expansion.

use paideia_as_ast::{AstArena, ExprData, NodeKind, StmtData};
use paideia_as_diagnostics::{SourceMap, Span, VecSink};
use paideia_as_elaborator::{LocalBindingTable, unsafe_walker::UnsafeWalker};
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{InstrMode, IrArena};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::common::test_span;

/// Helper: Parse and elaborate an unsafe instruction with a big immediate.
/// The source text format is "MNEMONIC DST_REG, 0xFFFFFFFF00000000" for big immediates.
/// Returns the generated instructions and diagnostics.
fn elaborate_bitop_with_imm64(
    mnemonic_str: &str,
    dst_reg_name: &str,
) -> (Vec<Instruction>, Vec<paideia_as_diagnostics::Diagnostic>) {
    let mut ast = AstArena::new();
    let mut ir = IrArena::new();

    // Create source text with a big immediate that requires expansion
    // For all tests, use 0xFFFFFFFF00000000 which needs expansion
    let imm_hex = "0xFFFFFFFF00000000";
    let source = format!("{} {}, {}", mnemonic_str, dst_reg_name, imm_hex);

    let file_id = paideia_as_diagnostics::FileId::new(1).unwrap();
    let stmt_span = Span::new(file_id, 0, source.len() as u32);

    // Calculate span for destination register
    // Format: "MNEMONIC " = mnemonic_str.len() + 1 (for space)
    let dst_start = (mnemonic_str.len() + 1) as u32;
    let dst_len = dst_reg_name.len() as u32;
    let dst_span = Span::new(file_id, dst_start, dst_len);

    // Immediate operand span: starts after ", " from dst
    let imm_start = (mnemonic_str.len() + 1 + dst_reg_name.len() + 2) as u32;
    let imm_len = imm_hex.len() as u32;
    let imm_span = Span::new(file_id, imm_start, imm_len);

    let justification = ast.alloc(NodeKind::ExprString, stmt_span);

    // Destination register operand
    let dst_ident = ast.alloc(NodeKind::Ident, dst_span);
    let dst_operand = ast.alloc_expr(
        NodeKind::OperandRegister,
        dst_span,
        ExprData::OperandRegister { reg: dst_ident },
    );

    // Immediate operand - placeholder node with span pointing to hex in source
    let imm_placeholder = ast.alloc(NodeKind::Placeholder, imm_span);
    let imm_operand = ast.alloc_expr(
        NodeKind::ExprLiteral,
        imm_span,
        ExprData::Literal {
            lit: imm_placeholder,
        },
    );

    // Create instruction statement
    let mnemonic_id = ast.intern_mnemonic(mnemonic_str);
    let inst_stmt = ast.alloc_stmt(
        NodeKind::StmtInstruction,
        stmt_span,
        StmtData::Instruction {
            mnemonic: mnemonic_id,
            operands: vec![dst_operand, imm_operand],
},
    );

    // Create unsafe block
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

    // Create source map and add the source text
    let mut source_map = SourceMap::new();
    let _ = source_map.add_file(PathBuf::from("test.pdx"), source);

    // Run UnsafeWalker
    let mut sink = VecSink::new();
    let record_layouts = HashMap::new();
    let local_bindings = LocalBindingTable::new();
    let mut next_emission_order = 1u32;
    let (_unsafe_labels, _label_to_instr, _first_instrs, mut diags) = UnsafeWalker::run(
        &mut ir,
        &ast,
        vec![ir_unsafe.get()],
        &source_map,
        &mut sink,
        &record_layouts,
        &local_bindings,
        InstrMode::Mode64,
        &HashSet::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut next_emission_order,
    );

    // Also collect diagnostics from the sink itself
    diags.extend(sink.diagnostics().to_vec());

    // Extract instructions from IR, sorting by node ID to maintain order
    let mut instructions: Vec<(paideia_as_ir::IrNodeId, Instruction)> = ir
        .instructions()
        .entries()
        .iter()
        .map(|(id, inst)| (*id, inst.clone()))
        .collect();

    // Sort by node ID to maintain insertion order
    instructions.sort_by_key(|(id, _)| *id);
    let instructions: Vec<Instruction> = instructions.into_iter().map(|(_, inst)| inst).collect();

    (instructions, diags)
}

// ===== Expansion detection tests =====

#[test]
fn or_rax_imm64_0xffffffff00000000_triggers_expansion() {
    // 0xFFFFFFFF00000000 does not round-trip through i32 → needs expansion
    assert!(paideia_as_elaborator::imm64_expand::needs_expansion(
        0xFFFFFFFF00000000u64 as i64
    ));
}

#[test]
fn and_rdi_imm64_0x8000000000000000_triggers_expansion() {
    // 0x8000000000000000 (min i64) does not fit in i32 → needs expansion
    assert!(paideia_as_elaborator::imm64_expand::needs_expansion(
        0x8000000000000000u64 as i64
    ));
}

#[test]
fn xor_r15_imm64_0x7fffffffffffffff_triggers_expansion() {
    // 0x7FFFFFFFFFFFFFFF (max i64) does not fit in i32 → needs expansion
    assert!(paideia_as_elaborator::imm64_expand::needs_expansion(
        0x7FFFFFFFFFFFFFFFu64 as i64
    ));
}

#[test]
fn or_rax_imm32_negative_one_no_expansion() {
    // -1 (0xFFFFFFFFFFFFFFFF) fits in i32 (as -1) → no expansion needed
    assert!(!paideia_as_elaborator::imm64_expand::needs_expansion(-1i64));
}

#[test]
fn and_rax_imm32_0x7fffffff_no_expansion() {
    // 0x7FFFFFFF fits in i32 → no expansion
    assert!(!paideia_as_elaborator::imm64_expand::needs_expansion(0x7FFFFFFF_i64));
}

#[test]
fn xor_rbx_imm32_negative_one_no_expansion() {
    // -1 (0xFFFFFFFFFFFFFFFF) fits in i32 → no expansion
    assert!(!paideia_as_elaborator::imm64_expand::needs_expansion(-1i64));
}

// ===== Integration tests: elaborate through the full pipeline =====
// These tests drive the elaborator to verify that expansion actually fires,
// by parsing source text and checking the resulting IR instruction count.

#[test]
fn or_rax_0xffffffff00000000_elaborates_to_two_instructions() {
    // or rax, 0xFFFFFFFF00000000 should expand to movabs r11 + or rax, r11
    let (instructions, diags) = elaborate_bitop_with_imm64("or", "rax");

    // Expansion should produce exactly 2 instructions
    assert_eq!(
        instructions.len(),
        2,
        "or with big immediate should expand to 2 instructions, got {} diags: {:?}",
        instructions.len(),
        diags
    );

    // First should be movabs (moving to r11)
    assert_eq!(instructions[0].mnemonic, Mnemonic::Mov);
    assert_eq!(instructions[0].operands[0], Operand::Reg(RegId(11)));

    // Second should be or
    assert_eq!(instructions[1].mnemonic, Mnemonic::Or);
    assert_eq!(instructions[1].operands[1], Operand::Reg(RegId(11)));
}

#[test]
fn and_rdi_0x8000000000000000_elaborates_to_two_instructions() {
    // and rdi, 0x8000000000000000 should expand to movabs r11 + and rdi, r11
    let (instructions, diags) = elaborate_bitop_with_imm64("and", "rdi");

    assert_eq!(
        instructions.len(),
        2,
        "and with big immediate should expand to 2 instructions, got {} diags: {:?}",
        instructions.len(),
        diags
    );

    // First should be movabs
    assert_eq!(instructions[0].mnemonic, Mnemonic::Mov);
    assert_eq!(instructions[0].operands[0], Operand::Reg(RegId(11)));

    // Second should be and
    assert_eq!(instructions[1].mnemonic, Mnemonic::And);
    assert_eq!(instructions[1].operands[1], Operand::Reg(RegId(11)));
}

#[test]
fn xor_r15_0x7fffffffffffffff_elaborates_to_two_instructions() {
    // xor r15, 0x7FFFFFFFFFFFFFFF should expand to movabs r11 + xor r15, r11
    let (instructions, diags) = elaborate_bitop_with_imm64("xor", "r15");

    assert_eq!(
        instructions.len(),
        2,
        "xor with big immediate should expand to 2 instructions, got {} diags: {:?}",
        instructions.len(),
        diags
    );

    // First should be movabs
    assert_eq!(instructions[0].mnemonic, Mnemonic::Mov);
    assert_eq!(instructions[0].operands[0], Operand::Reg(RegId(11)));

    // Second should be xor
    assert_eq!(instructions[1].mnemonic, Mnemonic::Xor);
    assert_eq!(instructions[1].operands[1], Operand::Reg(RegId(11)));
}

// ===== Collision guard test (Issue 3) =====

#[test]
fn or_r11_imm64_collision_guard_returns_none() {
    // When dst == r11, expand_bitop_imm64 should return None
    let mut arena = IrArena::new();
    let span = test_span();
    let mut next_emission_order: u32 = 1;

    let result = paideia_as_elaborator::imm64_expand::expand_bitop_imm64(
        &mut arena,
        span,
        Mnemonic::Or,
        RegId(11),
        0xFFFFFFFF00000000u64 as i64,
        InstrMode::Mode64,
        &mut next_emission_order,
    );

    assert!(result.is_none(), "expand_bitop_imm64 should return None for dst=r11");
}

#[test]
fn or_r11_imm64_collision_emits_diagnostic() {
    // Elaborate or r11, 0xFFFFFFFF00000000 and verify diagnostic U1615 is emitted
    let (instructions, diags) = elaborate_bitop_with_imm64("or", "r11");

    // Collision should prevent expansion, resulting in:
    // - No instructions, AND
    // - Diagnostic error U1615 (unsafe-block discipline, r11 collision)
    assert!(
        instructions.is_empty(),
        "Expected no instructions due to r11 collision, got {} instructions",
        instructions.len()
    );

    let has_u1615 = diags.iter().any(|d| d.code().number() == 1615);
    assert!(
        has_u1615,
        "Expected U1615 diagnostic for r11 collision, got diags: {:?}",
        diags
    );
}
