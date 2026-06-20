//! Constant pooling pass regression tests.
//!
//! Phase-4-m1-010: PoolConstants is now a real rewrite pass.
//! This pass detects repeated immediate operands via InstructionSideTable,
//! interns them into the ConstantPoolTable, and emits O1509 diagnostics.

mod common;

use common::create_instruction_node;
use common::create_test_arena;
use paideia_as_ir::instruction::{Mnemonic, Operand};
use paideia_as_ir::opt::{OptDiagSink, OptPass, PoolConstantsPass};

/// Test that pool-constants pass noop on empty arena.
#[test]
fn pool_constants_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = PoolConstantsPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger any rewrite.
    assert!(
        !changed,
        "Empty arena should produce no changes from pool-constants pass"
    );
    assert_eq!(sink.diagnostics.len(), 0);
}

/// Test that pool-constants pass is registered and callable.
#[test]
fn pool_constants_pass_registered() {
    let pass = PoolConstantsPass;
    assert_eq!(
        pass.name(),
        "pool-constants",
        "PoolConstants pass should have canonical name"
    );
}

/// Test that pool-constants correctly detects and interns repeated immediates.
#[test]
fn pool_constants_detects_and_interns_repeated_imm() {
    let (mut arena, func) = create_test_arena();

    // Create instructions with 2 repeated immediates
    let repeated_value = 0x1111_1111_1111_1111i64;
    let unique_value = 0x2222_2222_2222_2222i64;

    let _mov1 = create_instruction_node(
        &mut arena,
        Mnemonic::Mov,
        vec![Operand::Imm64(repeated_value)],
    );
    let _mov2 = create_instruction_node(
        &mut arena,
        Mnemonic::Mov,
        vec![Operand::Imm64(repeated_value)],
    );
    let _add = create_instruction_node(
        &mut arena,
        Mnemonic::Add,
        vec![Operand::Imm64(unique_value)],
    );

    let mut sink = OptDiagSink::new();
    let pass = PoolConstantsPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(
        changed,
        "PoolConstantsPass should detect and intern repeated immediates"
    );
    assert_eq!(sink.diagnostics.len(), 1);
    assert_eq!(sink.diagnostics[0].pass, "pool-constants");
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1509 rewrote 1 sites"),
        "Diagnostic should report 1 pooled constant"
    );

    // Verify the pool was populated with the repeated value
    let pool = arena.constant_pool();
    assert_eq!(pool.len(), 1, "Pool should contain 1 unique repeated value");
}

/// Test that pool-constants emits O1509 diagnostic with correct count.
#[test]
fn pool_constants_emits_o1509_with_correct_count() {
    let (mut arena, func) = create_test_arena();

    // Create instructions with 3 different repeated immediates
    let value1 = 0x1111_1111_1111_1111i64;
    let value2 = 0x2222_2222_2222_2222i64;
    let value3 = 0x3333_3333_3333_3333i64;

    let _mov1 = create_instruction_node(&mut arena, Mnemonic::Mov, vec![Operand::Imm64(value1)]);
    let _add1 = create_instruction_node(&mut arena, Mnemonic::Add, vec![Operand::Imm64(value1)]);

    let _mov2 = create_instruction_node(&mut arena, Mnemonic::Mov, vec![Operand::Imm64(value2)]);
    let _sub1 = create_instruction_node(&mut arena, Mnemonic::Sub, vec![Operand::Imm64(value2)]);

    let _mov3 = create_instruction_node(&mut arena, Mnemonic::Mov, vec![Operand::Imm64(value3)]);
    let _lea1 = create_instruction_node(&mut arena, Mnemonic::Lea, vec![Operand::Imm64(value3)]);

    let mut sink = OptDiagSink::new();
    let pass = PoolConstantsPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(changed);
    assert_eq!(sink.diagnostics.len(), 1);
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1509 rewrote 3 sites"),
        "Diagnostic should report 3 pooled constants"
    );

    // Verify the pool was populated with all 3 unique repeated values
    let pool = arena.constant_pool();
    assert_eq!(
        pool.len(),
        3,
        "Pool should contain 3 unique repeated values"
    );
}
