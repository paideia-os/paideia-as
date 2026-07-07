//! PA-r16-007 (#1067): ChecksumOps::ipv4_checksum recipe lowering.
//!
//! Tests that ChecksumOps::ipv4_checksum elaborates to the 21-instruction
//! RFC 1071 fold implementation with proper label registration.
//!
//! Recipe: RFC 1071 one's-complement folding checksum:
//! - SysVRegs: RDI = hdr pointer, RSI = length (bytes)
//! - Result in low-16 bits of RAX
//! - Three labels: loop_start, odd_check, fold

use paideia_as_ir::{InstrMode, IrArena, instruction::{Mnemonic, Operand, IntWidth, Cond}};

#[test]
fn checksum_ops_ipv4_checksum_lowers_to_21_instructions() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "ChecksumOps",
        "ipv4_checksum",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(
        result.is_some(),
        "ChecksumOps::ipv4_checksum should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("ipv4_checksum lowering should succeed");

    // Verify instruction count is 26 (double-fold RFC 1071 pattern)
    assert_eq!(
        recipe.instructions.len(),
        26,
        "ipv4_checksum should lower to exactly 26 instructions (double-fold pattern)"
    );

    // Verify arg_convention is SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "ipv4_checksum should use SysVRegs convention"
    );

    // Verify labels
    assert_eq!(recipe.labels.len(), 3, "ipv4_checksum should have 3 labels");
    assert_eq!(recipe.labels[0], ("loop_start", 5));
    assert_eq!(recipe.labels[1], ("odd_check", 11));
    assert_eq!(recipe.labels[2], ("fold", 16));
}

#[test]
fn checksum_ops_ipv4_checksum_instruction_sequence_is_correct() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "ChecksumOps",
        "ipv4_checksum",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("ipv4_checksum lowering should succeed");

    // Verify critical instruction mnemonics at key indices
    assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Xor, "inst[0] should be Xor");
    assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Mov, "inst[1] should be Mov");
    assert_eq!(recipe.instructions[2].mnemonic, Mnemonic::Shr, "inst[2] should be Shr");
    assert_eq!(recipe.instructions[3].mnemonic, Mnemonic::Test, "inst[3] should be Test");

    // Verify Jcc instructions with correct conditions
    match recipe.instructions[4].mnemonic {
        Mnemonic::Jcc(Cond::Zero) => {},
        _ => panic!("inst[4] should be Jcc(Zero)"),
    }

    // Verify loop_start label points to movzx
    assert_eq!(
        recipe.instructions[5].mnemonic, Mnemonic::Movzx,
        "inst[5] (loop_start label) should be Movzx"
    );

    assert_eq!(recipe.instructions[6].mnemonic, Mnemonic::Add, "inst[6] should be Add");

    // Verify Adc with W64 width
    match recipe.instructions[7].mnemonic {
        Mnemonic::Adc { width: IntWidth::W64 } => {},
        _ => panic!("inst[7] should be Adc with W64 width"),
    }

    assert_eq!(recipe.instructions[8].mnemonic, Mnemonic::Add, "inst[8] should be Add");
    assert_eq!(recipe.instructions[9].mnemonic, Mnemonic::Dec, "inst[9] should be Dec");

    // Verify loop back with Jcc(NonZero)
    match recipe.instructions[10].mnemonic {
        Mnemonic::Jcc(Cond::NonZero) => {},
        _ => panic!("inst[10] should be Jcc(NonZero)"),
    }

    // Verify odd_check label points to test
    assert_eq!(
        recipe.instructions[11].mnemonic, Mnemonic::Test,
        "inst[11] (odd_check label) should be Test"
    );

    // Verify conditional jump to fold
    match recipe.instructions[12].mnemonic {
        Mnemonic::Jcc(Cond::Zero) => {},
        _ => panic!("inst[12] should be Jcc(Zero)"),
    }

    // Verify odd-byte processing
    assert_eq!(recipe.instructions[13].mnemonic, Mnemonic::Movzx, "inst[13] should be Movzx");
    assert_eq!(recipe.instructions[14].mnemonic, Mnemonic::Add, "inst[14] should be Add");

    // Verify Adc after odd-byte addition
    match recipe.instructions[15].mnemonic {
        Mnemonic::Adc { width: IntWidth::W64 } => {},
        _ => panic!("inst[15] should be Adc with W64 width"),
    }

    // Verify fold label points to mov
    assert_eq!(
        recipe.instructions[16].mnemonic, Mnemonic::Mov,
        "inst[16] (fold label) should be Mov"
    );

    // Double-fold RFC 1071 pattern (indices 16-25):
    // 16: mov rdx, rax   ; first fold pass
    // 17: shr rdx, 16
    // 18: and rax, 0xffff
    // 19: add rax, rdx
    // 20: mov rdx, rax   ; second fold pass (handles overflow from first)
    // 21: shr rdx, 16
    // 22: and rax, 0xffff
    // 23: add rax, rdx
    // 24: not rax
    // 25: and rax, 0xffff (mask upper 1-bits from Not)
    assert_eq!(recipe.instructions[17].mnemonic, Mnemonic::Shr, "inst[17] should be Shr (first fold)");
    assert_eq!(recipe.instructions[18].mnemonic, Mnemonic::And, "inst[18] should be And (mask low16)");
    assert_eq!(recipe.instructions[19].mnemonic, Mnemonic::Add, "inst[19] should be Add (first fold)");
    assert_eq!(recipe.instructions[20].mnemonic, Mnemonic::Mov, "inst[20] should be Mov (second fold copy)");
    assert_eq!(recipe.instructions[21].mnemonic, Mnemonic::Shr, "inst[21] should be Shr (second fold)");
    assert_eq!(recipe.instructions[22].mnemonic, Mnemonic::And, "inst[22] should be And (mask low16)");
    assert_eq!(recipe.instructions[23].mnemonic, Mnemonic::Add, "inst[23] should be Add (second fold)");
    assert_eq!(recipe.instructions[24].mnemonic, Mnemonic::Not, "inst[24] should be Not (one's complement)");
    assert_eq!(recipe.instructions[25].mnemonic, Mnemonic::And, "inst[25] (final) should be And (mask Not result)");
}

#[test]
fn checksum_ops_ipv4_checksum_label_refs_are_correct() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "ChecksumOps",
        "ipv4_checksum",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("ipv4_checksum lowering should succeed");

    // Verify Jcc(Zero) at index 4 jumps to odd_check
    let jcc_4 = &recipe.instructions[4];
    assert_eq!(jcc_4.operands.len(), 1, "Jcc[4] should have 1 operand");
    match &jcc_4.operands[0] {
        Operand::LabelRef { name, addend } => {
            assert_eq!(name, "odd_check", "Jcc[4] should jump to odd_check");
            assert_eq!(*addend, 0);
        }
        _ => panic!("Jcc[4] operand should be LabelRef"),
    }

    // Verify Jcc(NonZero) at index 10 jumps to loop_start
    let jcc_10 = &recipe.instructions[10];
    assert_eq!(jcc_10.operands.len(), 1, "Jcc[10] should have 1 operand");
    match &jcc_10.operands[0] {
        Operand::LabelRef { name, addend } => {
            assert_eq!(name, "loop_start", "Jcc[10] should jump to loop_start");
            assert_eq!(*addend, 0);
        }
        _ => panic!("Jcc[10] operand should be LabelRef"),
    }

    // Verify Jcc(Zero) at index 12 jumps to fold
    let jcc_12 = &recipe.instructions[12];
    assert_eq!(jcc_12.operands.len(), 1, "Jcc[12] should have 1 operand");
    match &jcc_12.operands[0] {
        Operand::LabelRef { name, addend } => {
            assert_eq!(name, "fold", "Jcc[12] should jump to fold");
            assert_eq!(*addend, 0);
        }
        _ => panic!("Jcc[12] operand should be LabelRef"),
    }
}

#[test]
fn checksum_ops_ipv4_checksum_movzx_encoding_hints_are_set() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "ChecksumOps",
        "ipv4_checksum",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("ipv4_checksum lowering should succeed");

    // Verify movzx at index 5 (word load) has encoding hint
    let movzx_5 = &recipe.instructions[5];
    assert_eq!(movzx_5.mnemonic, Mnemonic::Movzx);
    assert!(
        movzx_5.encoding_hint.is_some(),
        "movzx[5] should have encoding hint"
    );
    if let Some(hint) = movzx_5.encoding_hint {
        assert_eq!(hint.opcode, 0x0F, "movzx[5] opcode should be 0x0F");
        assert_eq!(hint.operand_size, 2, "movzx[5] operand_size should be 2 (word)");
    }

    // Verify movzx at index 13 (byte load) has encoding hint
    let movzx_13 = &recipe.instructions[13];
    assert_eq!(movzx_13.mnemonic, Mnemonic::Movzx);
    assert!(
        movzx_13.encoding_hint.is_some(),
        "movzx[13] should have encoding hint"
    );
    if let Some(hint) = movzx_13.encoding_hint {
        assert_eq!(hint.opcode, 0x0F, "movzx[13] opcode should be 0x0F");
        assert_eq!(hint.operand_size, 1, "movzx[13] operand_size should be 1 (byte)");
    }
}

#[test]
fn checksum_ops_ipv4_checksum_uses_sysvregs_convention() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "ChecksumOps",
        "ipv4_checksum",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("ipv4_checksum lowering should succeed");

    // Verify arg_convention is SysVRegs (not Literal)
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "ipv4_checksum should use SysVRegs convention"
    );

    // Verify that recipe references RDI and RSI (SysV arg registers)
    let uses_rdi = recipe.instructions.iter().any(|inst| {
        inst.operands.iter().any(|op| match op {
            Operand::Reg(reg_id) => {
                // RDI = RegId(7), RSI = RegId(6)
                reg_id.0 == 7
            }
            Operand::MemSib { base, .. } => base.0 == 7,
            _ => false,
        })
    });

    let uses_rsi = recipe.instructions.iter().any(|inst| {
        inst.operands.iter().any(|op| match op {
            Operand::Reg(reg_id) => reg_id.0 == 6,
            _ => false,
        })
    });

    assert!(uses_rdi, "ipv4_checksum should reference RDI");
    assert!(uses_rsi, "ipv4_checksum should reference RSI");
}

#[test]
fn checksum_ops_ipv4_checksum_unknown_trait_returns_none() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "UnknownTrait",
        "ipv4_checksum",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_none(), "Unknown trait should return None");
}

#[test]
fn checksum_ops_unknown_method_returns_none() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "ChecksumOps",
        "unknown_method",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_none(), "Unknown method should return None");
}
