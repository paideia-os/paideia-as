//! PA-r16-007-followup (#1056) round-trip: calls to PerCpuOps::percpu_inc
//! and PerCpuOps::percpu_add elaborate to GS-prefixed lock instructions.
//! PA-R18-M2-003 (paideia-os#767): PerCpuOps::read_u64 / write_u64 / cmpxchg64
//! SysVRegs recipes for runtime-offset per-CPU control-block access.
//!
//! This test verifies the stdlib lowering path for PerCpuOps methods.
//! - percpu_inc(offset)                → lock inc qword [gs:offset]
//! - percpu_add(offset, val)           → lock add qword [gs:offset], imm
//! - read_u64(off)                     → mov rax, [gs:rdi + 0]
//! - write_u64(off, val)               → mov [gs:rdi + 0], rsi
//! - cmpxchg64(off, expected, new)     → mov rax, rsi; lock cmpxchg [gs:rdi+0], rdx

use paideia_as_ir::{InstrMode, IrArena, IrNodeId, abi, instruction::{Mnemonic, Operand, Scale, SegPrefix, IntWidth}};
use paideia_as_elaborator::stdlib_lowering::{ArgConvention, LoweringRecipe};

#[test]
fn percpu_inc_lowers_to_gs_lock_inc() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_inc",
        InstrMode::Mode64,
        &[offset_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "PerCpuOps::percpu_inc should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("percpu_inc lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "percpu_inc should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::LockInc {
            width: IntWidth::W64
        },
        "percpu_inc should lower to LockInc W64 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        1,
        "LockInc should have exactly one operand"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify the operand is MemSeg { Gs, MemDisp { 0x1000 } }
    match &inst.operands[0] {
        Operand::MemSeg { seg, inner } => {
            assert_eq!(*seg, SegPrefix::Gs, "segment should be Gs");
            match inner.as_ref() {
                Operand::MemDisp { disp } => {
                    assert_eq!(*disp, 0x1000, "displacement should be 0x1000");
                }
                _ => panic!("expected MemDisp inner operand"),
            }
        }
        _ => panic!("expected MemSeg operand"),
    }
}

#[test]
fn percpu_add_lowers_to_gs_lock_add() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x2000);
    arena.literal_values_mut().insert(value_id, 5);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "PerCpuOps::percpu_add should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "percpu_add should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::LockAdd {
            width: IntWidth::W64
        },
        "percpu_add should lower to LockAdd W64 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        2,
        "LockAdd should have exactly two operands"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify the first operand is MemSeg { Gs, MemDisp { 0x2000 } }
    match &inst.operands[0] {
        Operand::MemSeg { seg, inner } => {
            assert_eq!(*seg, SegPrefix::Gs, "segment should be Gs");
            match inner.as_ref() {
                Operand::MemDisp { disp } => {
                    assert_eq!(*disp, 0x2000, "displacement should be 0x2000");
                }
                _ => panic!("expected MemDisp inner operand"),
            }
        }
        _ => panic!("expected MemSeg operand"),
    }

    // Verify the second operand is Imm64(5)
    match &inst.operands[1] {
        Operand::Imm64(val) => {
            assert_eq!(*val, 5, "immediate should be 5");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

#[test]
fn percpu_add_with_large_offset() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    // Large offset within i32 range
    arena.literal_values_mut().insert(offset_id, 0x7FFFFFFF);
    arena.literal_values_mut().insert(value_id, 100);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add should succeed with large offset");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[0] {
        Operand::MemSeg { seg: _, inner } => match inner.as_ref() {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, 0x7FFFFFFF, "max i32 displacement should work");
            }
            _ => panic!("expected MemDisp"),
        },
        _ => panic!("expected MemSeg"),
    }
}

#[test]
fn percpu_inc_non_literal_returns_error() {
    let arena = IrArena::new();
    let missing_id = IrNodeId::new(999).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_inc",
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
            assert_eq!(method, "PerCpuOps::percpu_inc");
        }
    }
}

#[test]
fn percpu_add_non_literal_offset_returns_error() {
    let mut arena = IrArena::new();
    let missing_offset = IrNodeId::new(999).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(value_id, 5);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[missing_offset, value_id],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal offset");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "PerCpuOps::percpu_add");
        }
    }
}

#[test]
fn percpu_add_non_literal_value_returns_error() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let missing_value = IrNodeId::new(999).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, missing_value],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal value");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 1);
            assert_eq!(method, "PerCpuOps::percpu_add");
        }
    }
}

#[test]
fn percpu_inc_offset_out_of_i32_range_returns_error() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    // Value larger than i32::MAX
    arena.literal_values_mut().insert(offset_id, i32::MAX as i64 + 1);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_inc",
        InstrMode::Mode64,
        &[offset_id],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on offset out of range");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "PerCpuOps::percpu_inc");
        }
    }
}

#[test]
fn percpu_add_with_negative_offset() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    // Negative offset (still within i32 range)
    arena.literal_values_mut().insert(offset_id, -256);
    arena.literal_values_mut().insert(value_id, 10);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add should succeed with negative offset");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[0] {
        Operand::MemSeg { seg: _, inner } => match inner.as_ref() {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, -256, "negative displacement should work");
            }
            _ => panic!("expected MemDisp"),
        },
        _ => panic!("expected MemSeg"),
    }
}

#[test]
fn percpu_add_with_negative_immediate() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x1000);
    // Negative immediate value
    arena.literal_values_mut().insert(value_id, -1);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add should succeed with negative immediate");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[1] {
        Operand::Imm64(val) => {
            assert_eq!(*val, -1, "negative immediate should work");
        }
        _ => panic!("expected Imm64"),
    }
}

// ============================================================================
// R18-M2-003 (paideia-os#767): PerCpuOps read_u64 / write_u64 / cmpxchg64
// ============================================================================

fn expect_gs_mem_sib_rdi_disp0(op: &Operand) {
    match op {
        Operand::MemSeg { seg, inner } => {
            assert_eq!(*seg, SegPrefix::Gs, "segment should be Gs");
            match inner.as_ref() {
                Operand::MemSib { base, index, scale, disp } => {
                    assert_eq!(*base, abi::RDI, "base register should be RDI (SysV arg-0 = off)");
                    assert!(index.is_none(), "index should be None (no scaled index)");
                    assert_eq!(*scale, Scale::X1, "scale should be X1");
                    assert_eq!(*disp, 0, "displacement should be 0 (RDI carries the offset)");
                }
                other => panic!("expected MemSib inner operand, got {:?}", other),
            }
        }
        other => panic!("expected MemSeg operand, got {:?}", other),
    }
}

fn expect_sysv_regs_recipe(recipe: &LoweringRecipe, n_instructions: usize) {
    assert_eq!(
        recipe.arg_convention,
        ArgConvention::SysVRegs,
        "PerCpuOps runtime-offset accessors use SysVRegs arg convention"
    );
    assert_eq!(
        recipe.instructions.len(),
        n_instructions,
        "recipe should have exactly {} instruction(s)",
        n_instructions
    );
    assert!(
        recipe.labels.is_empty(),
        "recipe should not declare local labels"
    );
}

#[test]
fn read_u64_lowers_to_mov_gs_rdi_disp0() {
    // Args come from arg-marshalling (RDI = off), not literal extraction.
    // For SysVRegs recipes, arg_ids values do not need to be literal nodes;
    // the recipe uses fixed SysV registers regardless.
    let arena = IrArena::new();
    let off_id = IrNodeId::new(1).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "read_u64",
        InstrMode::Mode64,
        &[off_id],
        &arena,
    );

    let recipe = result
        .expect("read_u64 must be a registered stdlib method")
        .expect("read_u64 lowering should succeed for any arg (SysVRegs)");
    expect_sysv_regs_recipe(&recipe, 1);

    let inst = &recipe.instructions[0];
    assert_eq!(inst.mnemonic, Mnemonic::Mov, "should be Mov (W64 default)");
    assert_eq!(inst.mode, InstrMode::Mode64);
    assert_eq!(inst.operands.len(), 2, "mov has two operands");

    // op 0 = RAX (SysV return)
    match &inst.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RAX, "dst should be RAX (SysV return)"),
        other => panic!("expected Reg(RAX), got {:?}", other),
    }
    // op 1 = [gs:rdi + 0]
    expect_gs_mem_sib_rdi_disp0(&inst.operands[1]);
}

#[test]
fn write_u64_lowers_to_mov_gs_rdi_disp0_rsi() {
    let arena = IrArena::new();
    let off_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "write_u64",
        InstrMode::Mode64,
        &[off_id, val_id],
        &arena,
    );

    let recipe = result
        .expect("write_u64 must be a registered stdlib method")
        .expect("write_u64 lowering should succeed for any args (SysVRegs)");
    expect_sysv_regs_recipe(&recipe, 1);

    let inst = &recipe.instructions[0];
    assert_eq!(inst.mnemonic, Mnemonic::Mov, "should be Mov (W64 default)");
    assert_eq!(inst.operands.len(), 2, "mov has two operands");

    // op 0 = [gs:rdi + 0]
    expect_gs_mem_sib_rdi_disp0(&inst.operands[0]);
    // op 1 = RSI (SysV arg-1)
    match &inst.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RSI, "src should be RSI (SysV arg-1 = val)"),
        other => panic!("expected Reg(RSI), got {:?}", other),
    }
}

#[test]
fn cmpxchg64_lowers_to_mov_rax_rsi_then_lock_cmpxchg() {
    let arena = IrArena::new();
    let off_id = IrNodeId::new(1).expect("valid node id");
    let expected_id = IrNodeId::new(2).expect("valid node id");
    let new_id = IrNodeId::new(3).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "cmpxchg64",
        InstrMode::Mode64,
        &[off_id, expected_id, new_id],
        &arena,
    );

    let recipe = result
        .expect("cmpxchg64 must be a registered stdlib method")
        .expect("cmpxchg64 lowering should succeed for any args (SysVRegs)");
    expect_sysv_regs_recipe(&recipe, 2);

    // Instruction 0: mov rax, rsi (load expected into cmpxchg comparand)
    let ldrax = &recipe.instructions[0];
    assert_eq!(ldrax.mnemonic, Mnemonic::Mov);
    assert_eq!(ldrax.operands.len(), 2);
    match (&ldrax.operands[0], &ldrax.operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RAX, "mov dst should be RAX");
            assert_eq!(*src, abi::RSI, "mov src should be RSI (expected)");
        }
        other => panic!("expected (Reg(RAX), Reg(RSI)), got {:?}", other),
    }

    // Instruction 1: lock cmpxchg [gs:rdi+0], rdx
    let cx = &recipe.instructions[1];
    assert_eq!(cx.mnemonic, Mnemonic::LockCmpxchg, "should be LockCmpxchg (W64)");
    assert_eq!(cx.operands.len(), 2);
    expect_gs_mem_sib_rdi_disp0(&cx.operands[0]);
    match &cx.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDX, "src should be RDX (new)"),
        other => panic!("expected Reg(RDX), got {:?}", other),
    }
}

#[test]
fn read_write_cmpxchg_do_not_use_literal_extraction() {
    // Because they are SysVRegs recipes, missing literal values must NOT
    // return NonLiteralArg errors — the recipe is emitted unconditionally.
    // This guards against a regression that would force literal-only offsets
    // and defeat the point of R18-M2-003 (runtime-computed CB offsets).
    let arena = IrArena::new(); // note: no literal values inserted
    let id = IrNodeId::new(1).expect("valid node id");

    for method in &["read_u64", "write_u64", "cmpxchg64"] {
        let n_args = match *method {
            "read_u64" => 1,
            "write_u64" => 2,
            "cmpxchg64" => 3,
            _ => unreachable!(),
        };
        let args: Vec<IrNodeId> = (1..=n_args)
            .map(|i| IrNodeId::new(i as u32).expect("valid node id"))
            .collect();
        let _ = id; // silence unused
        let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
            "PerCpuOps",
            method,
            InstrMode::Mode64,
            &args,
            &arena,
        )
        .unwrap_or_else(|| panic!("{} must be registered", method));

        // Must be Ok, never NonLiteralArg
        result.unwrap_or_else(|e| {
            panic!(
                "SysVRegs recipe {} must not require literals, got error: {:?}",
                method, e
            )
        });
    }
}
