//! PA-r16-007-followup (#1057) round-trip: calls to MmioOps::mmio_read_u32
//! and MmioOps::mmio_write_u32 elaborate to MOV instructions with absolute displacement.
//!
//! This test verifies the stdlib lowering path for MmioOps methods.
//! - mmio_read_u32(addr) → mov eax, dword [addr]
//! - mmio_write_u32(addr, val) → mov dword [addr], imm

use paideia_as_ir::{InstrMode, IrArena, IrNodeId, instruction::{Mnemonic, Operand, IntWidth}};
use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};

#[test]
fn mmio_read_u32_lowers_to_mov_eax_mem_disp32() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "MmioOps::mmio_read_u32 should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("mmio_read_u32 lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "mmio_read_u32 should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized {
            width: IntWidth::W32
        },
        "mmio_read_u32 should lower to MovSized W32 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        2,
        "MovSized should have exactly two operands"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify first operand is Reg(RAX)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            assert_eq!(*reg, paideia_as_ir::abi::RAX, "first operand should be RAX");
        }
        _ => panic!("expected Reg(RAX) operand"),
    }

    // Verify second operand is MemDisp { 0x1000 }
    match &inst.operands[1] {
        Operand::MemDisp { disp } => {
            assert_eq!(*disp, 0x1000, "displacement should be 0x1000");
        }
        _ => panic!("expected MemDisp operand"),
    }
}

#[test]
fn mmio_write_u32_lowers_to_mov_mem_disp32_imm32() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);
    arena.literal_values_mut().insert(val_id, 0x12345678);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_write_u32",
        InstrMode::Mode64,
        &[addr_id, val_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "MmioOps::mmio_write_u32 should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("mmio_write_u32 lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "mmio_write_u32 should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized {
            width: IntWidth::W32
        },
        "mmio_write_u32 should lower to MovSized W32 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        2,
        "MovSized should have exactly two operands"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify first operand is MemDisp { 0x1000 }
    match &inst.operands[0] {
        Operand::MemDisp { disp } => {
            assert_eq!(*disp, 0x1000, "displacement should be 0x1000");
        }
        _ => panic!("expected MemDisp operand"),
    }

    // Verify second operand is Imm64(0x12345678)
    match &inst.operands[1] {
        Operand::Imm64(val) => {
            assert_eq!(*val, 0x12345678, "immediate should be 0x12345678");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

#[test]
fn mmio_read_u32_with_large_addr() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    // Large address within i32 range
    arena.literal_values_mut().insert(addr_id, 0x7FFFFFFF);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("mmio_read_u32 should succeed with large addr");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[1] {
        Operand::MemDisp { disp } => {
            assert_eq!(*disp, 0x7FFFFFFF, "max i32 displacement should work");
        }
        _ => panic!("expected MemDisp"),
    }
}

#[test]
fn mmio_read_u32_non_literal_returns_error() {
    let arena = IrArena::new();
    let missing_id = IrNodeId::new(999).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[missing_id],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal arg");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "MmioOps::mmio_read_u32");
        }
    }
}

#[test]
fn mmio_write_u32_non_literal_val_returns_error() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let missing_val = IrNodeId::new(999).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_write_u32",
        InstrMode::Mode64,
        &[addr_id, missing_val],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal val");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 1);
            assert_eq!(method, "MmioOps::mmio_write_u32");
        }
    }
}

#[test]
fn mmio_read_u32_encodes_to_correct_bytes() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("mmio_read_u32 lowering should succeed");
    assert_eq!(recipe.instructions.len(), 1);

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _output = encode_instruction(&recipe.instructions[0], &mut buf, &mut stats)
        .expect("encoding should succeed");

    // Expected bytes for: mov eax, dword [0x1000]
    // Opcode: 8B 04 25 (mov r32, r/m32 with SIB byte 04 25 for [disp32])
    // Displacement: 0x1000 in little-endian = 00 10 00 00
    // Full sequence: 8B 04 25 00 10 00 00
    let expected = vec![0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00];
    assert_eq!(
        buf.bytes, expected,
        "mmio_read_u32 should encode to mov eax, dword [0x1000]"
    );
}

#[test]
fn mmio_write_u32_encodes_to_correct_bytes() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);
    arena.literal_values_mut().insert(val_id, 0x12345678);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_write_u32",
        InstrMode::Mode64,
        &[addr_id, val_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("mmio_write_u32 lowering should succeed");
    assert_eq!(recipe.instructions.len(), 1);

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _output = encode_instruction(&recipe.instructions[0], &mut buf, &mut stats)
        .expect("encoding should succeed");

    // Expected bytes for: mov dword [0x1000], 0x12345678
    // Opcode: C7 04 25 (mov r/m32, imm32 with SIB byte 04 25 for [disp32])
    // Displacement: 0x1000 in little-endian = 00 10 00 00
    // Immediate: 0x12345678 in little-endian = 78 56 34 12
    // Full sequence: C7 04 25 00 10 00 00 78 56 34 12
    let expected = vec![0xC7, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12];
    assert_eq!(
        buf.bytes, expected,
        "mmio_write_u32 should encode to mov dword [0x1000], 0x12345678"
    );
}

// PA-v0.21-013 (#1289): u8 / u16 / u64 volatile lowering.
//
// The recipe layer mirrors the u32 form with a different IntWidth; the
// encoder path is exactly what `MovSized{W8/W16/W64}` already exercises.
// We assert shape here and add one representative byte-exact check per
// width to catch a mis-wiring of the width parameter.

use paideia_as_ir::instruction::IntWidth as MmioWidth;

fn recipe_for(method: &str, addr: i64) -> paideia_as_elaborator::stdlib_lowering::LoweringRecipe {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, addr);
    paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        method,
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    )
    .unwrap_or_else(|| panic!("MmioOps::{} must be registered", method))
    .unwrap_or_else(|e| panic!("MmioOps::{} lowering must succeed, got {:?}", method, e))
}

fn write_recipe_for(
    method: &str,
    addr: i64,
    val: i64,
) -> paideia_as_elaborator::stdlib_lowering::LoweringRecipe {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, addr);
    arena.literal_values_mut().insert(val_id, val);
    paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        method,
        InstrMode::Mode64,
        &[addr_id, val_id],
        &arena,
    )
    .unwrap_or_else(|| panic!("MmioOps::{} must be registered", method))
    .unwrap_or_else(|e| panic!("MmioOps::{} lowering must succeed, got {:?}", method, e))
}

fn assert_mov_sized_shape(
    recipe: &paideia_as_elaborator::stdlib_lowering::LoweringRecipe,
    expected_width: MmioWidth,
) {
    assert_eq!(recipe.instructions.len(), 1);
    match recipe.instructions[0].mnemonic {
        Mnemonic::MovSized { width } => assert_eq!(width, expected_width),
        other => panic!("expected MovSized, got {:?}", other),
    }
    assert_eq!(recipe.instructions[0].operands.len(), 2);
}

#[test]
fn mmio_read_u8_lowers_to_mov_sized_w8() {
    let recipe = recipe_for("mmio_read_u8", 0x1000);
    assert_mov_sized_shape(&recipe, MmioWidth::W8);
}

#[test]
fn mmio_read_u16_lowers_to_mov_sized_w16() {
    let recipe = recipe_for("mmio_read_u16", 0x1000);
    assert_mov_sized_shape(&recipe, MmioWidth::W16);
}

#[test]
fn mmio_read_u64_lowers_to_mov_sized_w64() {
    let recipe = recipe_for("mmio_read_u64", 0x1000);
    assert_mov_sized_shape(&recipe, MmioWidth::W64);
}

#[test]
fn mmio_write_u8_lowers_to_mov_sized_w8() {
    let recipe = write_recipe_for("mmio_write_u8", 0x1000, 0x42);
    assert_mov_sized_shape(&recipe, MmioWidth::W8);
}

#[test]
fn mmio_write_u16_lowers_to_mov_sized_w16() {
    let recipe = write_recipe_for("mmio_write_u16", 0x1000, 0x1234);
    assert_mov_sized_shape(&recipe, MmioWidth::W16);
}

#[test]
fn mmio_write_u64_lowers_to_mov_sized_w64() {
    // W64 requires the immediate to fit in i32 sign-extended (per the
    // encoder's guard); use a small value.
    let recipe = write_recipe_for("mmio_write_u64", 0x1000, 0x7FFFFFFF);
    assert_mov_sized_shape(&recipe, MmioWidth::W64);
}

#[test]
fn mmio_read_u8_non_literal_addr_returns_error() {
    let arena = IrArena::new();
    let missing_id = IrNodeId::new(999).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u8",
        InstrMode::Mode64,
        &[missing_id],
        &arena,
    );
    let err = result
        .expect("recipe must be registered")
        .expect_err("must error on non-literal addr");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "MmioOps::mmio_read_u8");
        }
    }
}

#[test]
fn mmio_two_adjacent_reads_emit_two_distinct_movs() {
    // #1289 acceptance criterion: two adjacent MmioOps reads of the same
    // address emit two distinct `mov` instructions (not one CSE-collapsed).
    // The recipe is a raw MovSized mnemonic — no CSE runs over encoded
    // instructions — so emitting the recipe twice must produce two
    // byte-identical instructions in the buffer.
    let recipe1 = recipe_for("mmio_read_u32", 0x1000);
    let recipe2 = recipe_for("mmio_read_u32", 0x1000);

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _ = encode_instruction(&recipe1.instructions[0], &mut buf, &mut stats).unwrap();
    let after_first = buf.bytes.len();
    let _ = encode_instruction(&recipe2.instructions[0], &mut buf, &mut stats).unwrap();

    // Two calls → two independent encodings; each produces the same 7-byte
    // sequence, so the buffer length is exactly 2 * 7 = 14 bytes.
    assert_eq!(after_first, 7, "first mmio_read_u32 = 7 bytes");
    assert_eq!(buf.bytes.len(), 14, "two adjacent reads emit two 7-byte MOVs");
    assert_eq!(
        &buf.bytes[0..7],
        &buf.bytes[7..14],
        "the two encodings are byte-identical"
    );
}

#[test]
fn mmio_read_u16_encodes_to_66_prefix_form() {
    // Byte-exact spot-check for the W16 width: mov ax, [0x1000] should
    // start with the 0x66 operand-size prefix.
    let recipe = recipe_for("mmio_read_u16", 0x1000);
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _ = encode_instruction(&recipe.instructions[0], &mut buf, &mut stats).unwrap();
    assert_eq!(
        buf.bytes[0], 0x66,
        "W16 mov to memory carries the 0x66 operand-size prefix"
    );
}

#[test]
fn mmio_read_u64_encodes_to_rex_w_form() {
    // Byte-exact spot-check for the W64 width: mov rax, [0x1000] should
    // start with 0x48 (REX.W).
    let recipe = recipe_for("mmio_read_u64", 0x1000);
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _ = encode_instruction(&recipe.instructions[0], &mut buf, &mut stats).unwrap();
    assert_eq!(
        buf.bytes[0], 0x48,
        "W64 mov to memory carries the REX.W prefix"
    );
}
