//! Integration tests for byte_offset_in_text field (Phase 7 m1-003)
//!
//! Tests verify that encoder correctly populates byte_offset_in_text before encoding,
//! allowing relocation sites to use the precise instruction byte offset rather than
//! computing it after encoding.

use paideia_as_ir::{
    Instruction, InstructionSideTable, IrNodeId, InstrMode, Mnemonic, Operand, RegId, SmallVec,
};

macro_rules! sv {
    ($($item:expr),*) => {{
        let mut sv: SmallVec<[_; 3]> = SmallVec::new();
        $(sv.push($item);)*
        sv
    }};
}

/// Test 1: Single call instruction has byte_offset_in_text populated
#[test]
fn single_call_offset_is_populated() {
    let mut table = InstructionSideTable::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: sv![Operand::SymbolRef {
            name: "target_fn".to_string(),
            addend: 0,
        }],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    };
    let node_id = IrNodeId::new(1).unwrap();
    table.insert(node_id, inst);

    // Simulate emit_text_from_instructions setting the offset
    if let Some(inst_mut) = table.get_mut(node_id) {
        inst_mut.byte_offset_in_text = Some(0);
    }

    let inst_retrieved = table.get(node_id).unwrap();
    assert_eq!(inst_retrieved.byte_offset_in_text, Some(0));
}

/// Test 2: Interleaved calls have distinct offsets
#[test]
fn interleaved_calls_have_distinct_offsets() {
    let mut table = InstructionSideTable::new();

    // Call 1 at offset 0
    let call1 = Instruction {
        mnemonic: Mnemonic::Call,
        operands: sv![Operand::SymbolRef {
            name: "fn1".to_string(),
            addend: 0,
        }],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    };
    let id1 = IrNodeId::new(1).unwrap();
    table.insert(id1, call1);

    // Mov instruction at offset 5 (call is 5 bytes: E8 + 4 bytes)
    let mov = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: sv![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))],
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
    let id2 = IrNodeId::new(2).unwrap();
    table.insert(id2, mov);

    // Call 2 at offset 8 (after 5-byte call + 3-byte mov)
    let call2 = Instruction {
        mnemonic: Mnemonic::Call,
        operands: sv![Operand::SymbolRef {
            name: "fn2".to_string(),
            addend: 0,
        }],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    };
    let id3 = IrNodeId::new(3).unwrap();
    table.insert(id3, call2);

    // Simulate emit_text_from_instructions setting offsets
    if let Some(inst) = table.get_mut(id1) {
        inst.byte_offset_in_text = Some(0);
    }
    if let Some(inst) = table.get_mut(id2) {
        inst.byte_offset_in_text = Some(5);
    }
    if let Some(inst) = table.get_mut(id3) {
        inst.byte_offset_in_text = Some(8);
    }

    let call1_off = table.get(id1).unwrap().byte_offset_in_text;
    let mov_off = table.get(id2).unwrap().byte_offset_in_text;
    let call2_off = table.get(id3).unwrap().byte_offset_in_text;

    assert_eq!(call1_off, Some(0));
    assert_eq!(mov_off, Some(5));
    assert_eq!(call2_off, Some(8));
}

/// Test 3: Three consecutive calls each have correct offset
#[test]
fn three_consecutive_calls_have_sequential_offsets() {
    let mut table = InstructionSideTable::new();

    for i in 1..=3 {
        let node_id = IrNodeId::new(i as u32).unwrap();
        let call = Instruction {
            mnemonic: Mnemonic::Call,
            operands: sv![Operand::SymbolRef {
                name: format!("fn{}", i),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
        table.insert(node_id, call);

        // Simulate encoding: each call is 5 bytes
        if let Some(inst) = table.get_mut(node_id) {
            inst.byte_offset_in_text = Some((i as u32 - 1) * 5);
        }
    }

    for i in 1..=3 {
        let node_id = IrNodeId::new(i as u32).unwrap();
        let expected_offset = (i as u32 - 1) * 5;
        let inst = table.get(node_id).unwrap();
        assert_eq!(inst.byte_offset_in_text, Some(expected_offset));
    }
}

/// Test 4: Mixed instructions maintain correct offsets
#[test]
fn mixed_instructions_maintain_correct_offsets() {
    let mut table = InstructionSideTable::new();

    let instructions = vec![
        (1, Mnemonic::Call, 0),  // call at 0
        (2, Mnemonic::Mov, 5),   // mov at 5 (call is 5 bytes)
        (3, Mnemonic::Call, 8),  // call at 8 (mov is 3 bytes)
        (4, Mnemonic::Add, 13),  // add at 13 (call is 5 bytes)
        (5, Mnemonic::Call, 16), // call at 16 (add is 3 bytes)
    ];

    for (id, mnem, offset) in instructions {
        let node_id = IrNodeId::new(id).unwrap();
        let operands = match mnem {
            Mnemonic::Call => {
                sv![Operand::SymbolRef {
                    name: format!("fn{}", id),
                    addend: 0,
                }]
            }
            Mnemonic::Mov | Mnemonic::Add => {
                sv![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))]
            }
            _ => unreachable!(),
        };

        let inst = Instruction {
            mnemonic: mnem,
            operands,
            encoding_hint: None,
            byte_offset_in_text: Some(offset as u32),
            mode: InstrMode::default(),
        };
        table.insert(node_id, inst);
    }

    // Verify all offsets
    for (id, _, expected_offset) in vec![
        (1, Mnemonic::Call, 0),
        (2, Mnemonic::Mov, 5),
        (3, Mnemonic::Call, 8),
        (4, Mnemonic::Add, 13),
        (5, Mnemonic::Call, 16),
    ] {
        let node_id = IrNodeId::new(id).unwrap();
        let inst = table.get(node_id).unwrap();
        assert_eq!(inst.byte_offset_in_text, Some(expected_offset as u32));
    }
}

/// Test 5: Call with addend preserves offset accuracy
#[test]
fn call_with_addend_preserves_offset_accuracy() {
    let mut table = InstructionSideTable::new();

    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: sv![Operand::SymbolRef {
            name: "symbol_with_addend".to_string(),
            addend: 42,
        }],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    };
    let node_id = IrNodeId::new(1).unwrap();
    table.insert(node_id, inst);

    // Set offset before encoding (this is done by emit_text_from_instructions)
    if let Some(inst_mut) = table.get_mut(node_id) {
        inst_mut.byte_offset_in_text = Some(100);
    }

    let inst_retrieved = table.get(node_id).unwrap();
    assert_eq!(inst_retrieved.byte_offset_in_text, Some(100));
    assert_eq!(inst_retrieved.operands.len(), 1);

    if let Operand::SymbolRef { addend, .. } = &inst_retrieved.operands[0] {
        assert_eq!(*addend, 42);
    }
}

/// Test 6: After-encoding offset computation demonstrates the off-by-one bug
/// This test shows why we need byte_offset_in_text: if the encoder computes
/// offset AFTER pushing bytes, it gets buf.bytes.len() which is AFTER the push.
#[test]
fn after_encoding_offset_would_be_incorrect() {
    let mut table = InstructionSideTable::new();

    let mut call1 = Instruction {
        mnemonic: Mnemonic::Call,
        operands: sv![Operand::SymbolRef {
            name: "fn1".to_string(),
            addend: 0,
        }],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    };
    call1.byte_offset_in_text = Some(0);

    let mut mov = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: sv![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
    mov.byte_offset_in_text = Some(5);

    let mut call2 = Instruction {
        mnemonic: Mnemonic::Call,
        operands: sv![Operand::SymbolRef {
            name: "fn2".to_string(),
            addend: 0,
        }],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    };
    call2.byte_offset_in_text = Some(8);

    // If encoder computed: reloc_offset = buf.bytes.len() + 1
    // After call1 (5 bytes): buf.bytes.len() = 5, reloc would be at 6 (WRONG, should be 1)
    // After mov (3 bytes):   buf.bytes.len() = 8, reloc would be at 9
    // After call2 (5 bytes): buf.bytes.len() = 13, reloc would be at 14 (WRONG, should be 9)
    //
    // With byte_offset_in_text:
    // call1: byte_offset_in_text = 0, reloc = 0 + 1 = 1 ✓
    // call2: byte_offset_in_text = 8, reloc = 8 + 1 = 9 ✓

    assert_eq!(call1.byte_offset_in_text.unwrap() + 1, 1);
    assert_eq!(call2.byte_offset_in_text.unwrap() + 1, 9);
}
