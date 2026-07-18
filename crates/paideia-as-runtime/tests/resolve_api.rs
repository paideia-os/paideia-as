//! Integration tests for the symbol resolution API (resolve_symbols).
//!
//! Eight AC canaries + four discipline canaries, mirroring #1020's test structure.

#![allow(unused_mut)]

use paideia_as_runtime::{
    resolve_symbols, Instruction, LabelMap, Mnemonic, Operand, RegId, ResolveError,
    ResolvePolicy, Scale, SymbolTable,
};
use smallvec::SmallVec;

// Helper: create a simple instruction with no encoding hint.
fn make_instruction(mnemonic: Mnemonic, operands: SmallVec<[Operand; 3]>) -> Instruction {
    Instruction {
        mnemonic,
        operands,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: paideia_as_runtime::InstrMode::Mode64,
        emission_order: 1,
    }
}

// ── AC Canaries (8) ──────────────────────────────────────────────────────

/// R-1: SymbolRef becomes Imm64 with no addend.
#[test]
fn resolve_symbolref_call_becomes_imm64() {
    let mut symbols = SymbolTable::new();
    symbols.insert("printf", 0xDEAD_BEEF);

    let mut labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::SymbolRef {
        name: "printf".to_string(),
        addend: 0,
    });

    let mut instructions = [make_instruction(Mnemonic::Call, ops)];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());

    assert_eq!(
        instructions[0].operands[0],
        Operand::Imm64(0xDEAD_BEEF as i64)
    );
}

/// R-2: SymbolRef with addend is correctly combined.
#[test]
fn resolve_symbolref_addend_applied() {
    let mut symbols = SymbolTable::new();
    symbols.insert("table", 0x1000);

    let mut labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::SymbolRef {
        name: "table".to_string(),
        addend: 8,
    });

    let mut instructions = [make_instruction(Mnemonic::Call, ops)];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());

    assert_eq!(instructions[0].operands[0], Operand::Imm64(0x1008 as i64));
}

/// R-3: LabelRef backward references resolve to correct offset.
#[test]
fn resolve_labelref_backward() {
    let symbols = SymbolTable::new();
    let mut labels = LabelMap::new();
    labels.insert("top", 0).unwrap();

    let mut ops = SmallVec::new();
    ops.push(Operand::LabelRef {
        name: "top".to_string(),
        addend: 0,
    });

    let mut instructions = [
        make_instruction(Mnemonic::Nop, SmallVec::new()),
        make_instruction(Mnemonic::Nop, SmallVec::new()),
        make_instruction(Mnemonic::Jmp, ops),
    ];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());

    // Nop is 1 byte, so offset of instruction[2] is 0 + 1 + 1 = 2, but label[0] = 0.
    assert_eq!(instructions[2].operands[0], Operand::Imm64(0));
}

/// R-4: LabelRef forward references resolve to correct offset.
#[test]
fn resolve_labelref_forward() {
    let symbols = SymbolTable::new();
    let mut labels = LabelMap::new();
    labels.insert("end", 2).unwrap();

    let mut ops = SmallVec::new();
    ops.push(Operand::LabelRef {
        name: "end".to_string(),
        addend: 0,
    });

    let mut instructions = [
        make_instruction(Mnemonic::Jmp, ops),
        make_instruction(Mnemonic::Nop, SmallVec::new()),
        make_instruction(Mnemonic::Nop, SmallVec::new()),
    ];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());

    // Jmp is 5 bytes, Nop is 1 byte each, so offset[2] = 5 + 1 = 6.
    assert_eq!(instructions[0].operands[0], Operand::Imm64(6));
}

/// R-5: MemRipRelSym becomes MemRipRel with resolved displacement.
#[test]
fn resolve_memripsym_to_memriprel() {
    let mut symbols = SymbolTable::new();
    symbols.insert("data", 0x100);

    let mut labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::Reg(RegId(0)));
    ops.push(Operand::MemRipRelSym {
        name: "data".to_string(),
        addend: 0,
    });

    let mut instructions = [make_instruction(Mnemonic::Mov, ops)];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());

    assert_eq!(instructions[0].operands[1], Operand::MemRipRel { disp: 0x100 });
}

/// R-6: MemSymIndexed becomes MemDispIndexed with resolved displacement.
#[test]
fn resolve_memsymindexed_to_memdispindexed() {
    let mut symbols = SymbolTable::new();
    symbols.insert("tbl", 0x1000);

    let mut labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::Reg(RegId(0)));
    ops.push(Operand::MemSymIndexed {
        name: "tbl".to_string(),
        addend: 0,
        index: RegId(3),
        scale: Scale::X8,
    });

    let mut instructions = [make_instruction(Mnemonic::Mov, ops)];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());

    assert_eq!(
        instructions[0].operands[1],
        Operand::MemDispIndexed {
            disp: 0x1000,
            index: RegId(3),
            scale: Scale::X8,
        }
    );
}

/// R-7: Unknown symbol returns UnknownSymbol error with correct indices.
#[test]
fn unknown_symbol_reports_error() {
    let symbols = SymbolTable::new();
    let labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::SymbolRef {
        name: "missing".to_string(),
        addend: 0,
    });

    let mut instructions = [make_instruction(Mnemonic::Call, ops)];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);

    match result {
        Err(ResolveError::UnknownSymbol { instr_index, operand_index, name }) => {
            assert_eq!(instr_index, 0);
            assert_eq!(operand_index, 0);
            assert_eq!(name, "missing");
        }
        _ => panic!("Expected UnknownSymbol error"),
    }
}

/// R-8: Unknown label returns UnknownLabel error with correct indices.
#[test]
fn unknown_label_reports_error() {
    let symbols = SymbolTable::new();
    let labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::LabelRef {
        name: "noSuchLabel".to_string(),
        addend: 0,
    });

    let mut instructions = [make_instruction(Mnemonic::Jmp, ops)];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);

    match result {
        Err(ResolveError::UnknownLabel { instr_index, operand_index, name }) => {
            assert_eq!(instr_index, 0);
            assert_eq!(operand_index, 0);
            assert_eq!(name, "noSuchLabel");
        }
        _ => panic!("Expected UnknownLabel error"),
    }
}

// ── Discipline Canaries (4) ──────────────────────────────────────────────

/// R-9: Resolve then emit roundtrip — resolved instructions pass emit pre-flight.
#[test]
fn resolve_then_emit_roundtrip() {
    let mut symbols = SymbolTable::new();
    symbols.insert("foo", 0x1000);

    let labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::SymbolRef {
        name: "foo".to_string(),
        addend: 0,
    });

    let mut instructions = [
        make_instruction(Mnemonic::Nop, SmallVec::new()),
        make_instruction(Mnemonic::Call, ops),
        make_instruction(Mnemonic::Ret, SmallVec::new()),
    ];

    let resolve_result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(resolve_result.is_ok());

    // Post-resolve, verify no instruction carries a reloc operand.
    for ins in &instructions {
        for op in &ins.operands {
            assert!(!matches!(
                op,
                Operand::SymbolRef { .. }
                    | Operand::LabelRef { .. }
                    | Operand::MemRipRelSym { .. }
                    | Operand::MemSymIndexed { .. }
            ));
        }
    }
}

/// R-10: Resolved Call target is correctly set (verifiable by inspection).
#[test]
fn resolved_call_target_correct() {
    let mut symbols = SymbolTable::new();
    symbols.insert("foo", 0x1000);

    let labels = LabelMap::new();

    let mut ops = SmallVec::new();
    ops.push(Operand::SymbolRef {
        name: "foo".to_string(),
        addend: 0,
    });

    let mut instructions = [
        make_instruction(Mnemonic::Nop, SmallVec::new()),
        make_instruction(Mnemonic::Call, ops),
        make_instruction(Mnemonic::Ret, SmallVec::new()),
    ];

    resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64).unwrap();

    // After resolution, the Call operand should be Imm64(0x1000).
    assert_eq!(instructions[1].operands[0], Operand::Imm64(0x1000));
}

/// R-11: Partial mutation on error — prior instructions are mutated, later are not.
#[test]
fn resolve_partial_mutation_on_error() {
    let mut symbols = SymbolTable::new();
    symbols.insert("known", 0x1);

    let labels = LabelMap::new();

    let mut ops1 = SmallVec::new();
    ops1.push(Operand::SymbolRef {
        name: "known".to_string(),
        addend: 0,
    });

    let mut ops2 = SmallVec::new();
    ops2.push(Operand::SymbolRef {
        name: "unknown".to_string(),
        addend: 0,
    });

    let mut instructions = [
        make_instruction(Mnemonic::Nop, SmallVec::new()),
        make_instruction(Mnemonic::Call, ops1),
        make_instruction(Mnemonic::Call, ops2),
    ];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);

    // Error on the unknown symbol.
    assert!(matches!(
        result,
        Err(ResolveError::UnknownSymbol {
            instr_index: 2,
            ..
        })
    ));

    // Instruction[1] was mutated before the error.
    assert_eq!(instructions[1].operands[0], Operand::Imm64(0x1));

    // Instruction[2] still has the unresolved SymbolRef.
    if let Operand::SymbolRef { name, .. } = &instructions[2].operands[0] {
        assert_eq!(name, "unknown");
    } else {
        panic!("Expected SymbolRef operand");
    }
}

/// R-12: Empty slice is handled gracefully.
#[test]
fn resolve_empty_slice_ok() {
    let symbols = SymbolTable::new();
    let labels = LabelMap::new();

    let mut instructions: [Instruction; 0] = [];

    let result = resolve_symbols(&mut instructions, &symbols, &labels, ResolvePolicy::AbsoluteImm64);
    assert!(result.is_ok());
}
