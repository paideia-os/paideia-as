//! Integration tests for WASM i32.add dynamic-emit lowering.
//!
//! These tests verify that the WASM → x86_64 lowering produces correct byte sequences,
//! handles errors appropriately, and maintains the emit_instruction contract.
//!
//! The 6 canary tests cover:
//! 1. i32.add lowering produces exact byte sequence
//! 2. local.get(0) at depth 0 targets RAX
//! 3. Full function body produces expected 14-byte stream
//! 4. iced-x86 round-trip on full body
//! 5. Decode error on invalid opcode
//! 6. emit_instruction refuses unresolved relocation

use paideia_as_emit::{emit_instruction, CodeBuffer, EmitError, Instruction, Mnemonic, Operand};
use paideia_as_runtime::{InstrMode, RegId, Scale};
use smallvec::SmallVec;

// --- Shared fixture helpers (duplicated from examples/wasm_add.rs for test isolation) ---

#[derive(Debug, Clone)]
enum WasmOp {
    LocalGet(u32),
    I32Add,
    End,
}

fn decode_body(bytes: &[u8]) -> Result<Vec<WasmOp>, &'static str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x20 => {
                let idx = bytes.get(i + 1).copied().ok_or("truncated local.get")? as u32;
                out.push(WasmOp::LocalGet(idx));
                i += 2;
            }
            0x6A => {
                out.push(WasmOp::I32Add);
                i += 1;
            }
            0x0B => {
                out.push(WasmOp::End);
                i += 1;
            }
            _ => return Err("unsupported opcode"),
        }
    }
    Ok(out)
}

fn lower(op: &WasmOp, stack_depth: usize) -> Vec<Instruction> {
    match op {
        WasmOp::LocalGet(idx) => {
            let dst = if stack_depth == 0 {
                RegId(0)
            } else {
                RegId(3)
            };
            let disp = 16 + (*idx as i32) * 8;
            vec![instr(
                Mnemonic::Mov,
                &[
                    Operand::Reg(dst),
                    Operand::MemSib {
                        base: RegId(4),
                        index: None,
                        scale: Scale::X1,
                        disp,
                    },
                ],
            )]
        }
        WasmOp::I32Add => {
            vec![instr(
                Mnemonic::Add,
                &[Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
            )]
        }
        WasmOp::End => vec![instr(Mnemonic::Ret, &[])],
    }
}

fn instr(mnemonic: Mnemonic, operands: &[Operand]) -> Instruction {
    Instruction {
        mnemonic,
        operands: operands.iter().cloned().collect(),
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        emission_order: 0,
    }
}

fn emit_body(ops: &[WasmOp]) -> Result<Vec<u8>, EmitError> {
    let mut buf = CodeBuffer::new();
    let mut depth = 0usize;
    for op in ops {
        for ins in lower(op, depth) {
            emit_instruction(&mut buf, ins)?;
        }
        depth = match op {
            WasmOp::LocalGet(_) => depth + 1,
            WasmOp::I32Add => depth.saturating_sub(1),
            WasmOp::End => depth,
        };
    }
    Ok(buf.bytes)
}

// --- Canary 1: i32.add produces expected 3-byte sequence ---

#[test]
fn lower_i32_add_produces_add_rax_rbx() {
    let mut buf = CodeBuffer::new();

    // Lower i32.add at stack depth 2 (both operands already on stack)
    let add_instrs = lower(&WasmOp::I32Add, 2);
    assert_eq!(add_instrs.len(), 1);

    let result = emit_instruction(&mut buf, add_instrs[0].clone());
    assert!(result.is_ok(), "i32.add should emit successfully");

    // ADD RAX, RBX → 48 01 D8 (3 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x01, 0xD8]);
}

// --- Canary 2: local.get(0) at depth 0 targets RAX ---

#[test]
fn lower_local_get_at_depth_zero_targets_rax() {
    let mut buf = CodeBuffer::new();

    // Lower local.get 0 at stack depth 0 (becomes RAX)
    let get_instrs = lower(&WasmOp::LocalGet(0), 0);
    assert_eq!(get_instrs.len(), 1);

    let result = emit_instruction(&mut buf, get_instrs[0].clone());
    assert!(result.is_ok(), "local.get 0 should emit successfully");

    // MOV RAX, [RSP + 0x10] → 48 8B 44 24 10 (5 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x44, 0x24, 0x10]);
}

// --- Canary 3: full function body produces expected 14-byte sequence ---

#[test]
fn emit_full_body_produces_expected_stream() {
    // Input: [0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B]
    // local.get 0, local.get 1, i32.add, end
    let function_body: &[u8] = &[0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B];

    let ops = decode_body(function_body).expect("decode should succeed");
    assert_eq!(ops.len(), 4);

    let emitted = emit_body(&ops).expect("emit should succeed");

    // Expected: mov rax, [rsp+16] (5) + mov rbx, [rsp+24] (5) + add rax, rbx (3) + ret (1) = 14 bytes
    let expected = vec![
        0x48, 0x8B, 0x44, 0x24, 0x10, // mov rax, [rsp+16]
        0x48, 0x8B, 0x5C, 0x24, 0x18, // mov rbx, [rsp+24]
        0x48, 0x01, 0xD8,             // add rax, rbx
        0xC3,                          // ret
    ];

    assert_eq!(emitted.len(), 14, "Full body should be 14 bytes");
    assert_eq!(emitted, expected);
}

// --- Canary 4: iced-x86 round-trip on full body ---

#[test]
fn emit_full_body_roundtrips_via_iced() {
    use iced_x86::{Decoder, DecoderOptions};

    let function_body: &[u8] = &[0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B];

    let ops = decode_body(function_body).expect("decode should succeed");
    let emitted = emit_body(&ops).expect("emit should succeed");

    // Decode emitted bytes using iced-x86
    let mut decoder = Decoder::new(64, &emitted, DecoderOptions::NONE);
    let mut mnemonics = Vec::new();
    for instruction in &mut decoder {
        mnemonics.push(instruction.mnemonic());
    }

    // Expect: Mov, Mov, Add, Ret
    use iced_x86::Mnemonic as IcedMnemonic;
    assert_eq!(mnemonics.len(), 4);
    assert_eq!(mnemonics[0], IcedMnemonic::Mov);
    assert_eq!(mnemonics[1], IcedMnemonic::Mov);
    assert_eq!(mnemonics[2], IcedMnemonic::Add);
    assert_eq!(mnemonics[3], IcedMnemonic::Ret);
}

// --- Canary 5: decode_body rejects truncated local.get ---

#[test]
fn decode_body_rejects_truncated_local_get() {
    // Truncated: local.get opcode (0x20) without index byte
    let truncated = [0x20];
    let result = decode_body(&truncated);

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "truncated local.get");
}

// --- Canary 6: emit_instruction refuses unresolved relocation ---

#[test]
fn emit_refuses_unsupported_opcode() {
    let mut buf = CodeBuffer::new();

    // Hand-build a CALL instruction with an unresolved SymbolRef operand.
    // This simulates what would happen if someone naively lowered a WASM `call $host_func`
    // without seizing the symbol-resolution API first.
    let call_with_symbol = Instruction {
        mnemonic: Mnemonic::Call,
        operands: {
            let mut ops = SmallVec::new();
            ops.push(Operand::SymbolRef {
                name: "host_func".to_string(),
                addend: 0,
            });
            ops
        },
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        emission_order: 0,
    };

    let result = emit_instruction(&mut buf, call_with_symbol);

    // Should fail with UnresolvedRelocation
    assert!(
        matches!(result, Err(EmitError::UnresolvedRelocation)),
        "emit_instruction should refuse SymbolRef"
    );

    // Buffer should be unchanged (rollback contract)
    assert_eq!(buf.as_slice().len(), 0, "buffer should be empty after error");
}
