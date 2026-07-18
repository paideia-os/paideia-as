//! paideia-as v0.20 dynamic-emit + PQ signing demo — WASM i32.add with signature
//!
//! This example extends the basic `wasm_add.rs` example by adding a post-emit
//! signing and verification step. It demonstrates the composition pattern that
//! paideia-os Phase 13 (WASM jail) will use:
//!
//!   emit_instruction → sign_runtime_buffer → (later) verify_runtime_buffer + execute
//!
//! The example:
//!   1. Decodes WASM i32.add opcodes (same as wasm_add.rs)
//!   2. Lowers to x86_64 instructions (same as wasm_add.rs)
//!   3. Emits bytecode via emit_instruction (same as wasm_add.rs)
//!   4. Signs the emitted buffer with a hybrid keypair (NEW)
//!   5. Verifies the signature round-trip (NEW)
//!
//! Run via: cargo run -p paideia-as-emit --example wasm_add_signed

use paideia_as_emit::{emit_instruction, CodeBuffer, EmitError};
use paideia_as_runtime::{Instruction, InstrMode, Mnemonic, Operand, RegId, Scale};
use paideia_pq_sign::{Hybrid, Signer, sign_runtime_buffer, verify_runtime_buffer};
use rand_core::OsRng;

// --- WASM opcode surface (tiny subset, one shape per opcode we handle) ---

/// Minimal WASM opcode set for this demo.
#[derive(Debug, Clone)]
enum WasmOp {
    /// local.get <index> (opcode 0x20)
    LocalGet(u32),
    /// i32.add (opcode 0x6A)
    I32Add,
    /// end (opcode 0x0B)
    End,
}

/// Decode a WASM function body byte stream into a sequence of WasmOp.
///
/// This is a minimal, hand-rolled decoder for demonstration purposes.
/// A production engine would use a real WASM parser (e.g., wasmparser).
fn decode_body(bytes: &[u8]) -> Result<Vec<WasmOp>, &'static str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x20 => {
                // local.get <leb128 u32>. Single-byte encoding is sufficient
                // for indices < 128; the example never uses indices ≥ 128.
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

// --- Lowering table (WASM → x86_64 Instruction) ---

/// Lower a single WASM opcode to zero or more x86_64 instructions.
///
/// Evaluation-stack model: WASM's operand stack is materialized in registers.
/// We use a simplified two-depth model:
/// - RAX = top of stack (evaluation result)
/// - RBX = second on stack
///
/// Args are passed on the stack per SysV calling convention:
/// - arg0 at [rsp + 16]
/// - arg1 at [rsp + 24]
/// (displacement assumes no local variables; a real engine would compute this
/// from the function signature and local declarations.)
fn lower(op: &WasmOp, stack_depth: usize) -> Vec<Instruction> {
    match op {
        WasmOp::LocalGet(idx) => {
            // Load the local variable at index `idx` from the stack frame.
            // Target register depends on current stack depth: if we're at depth 0,
            // this becomes the top (RAX); if depth > 0, the next slot (RBX).
            let dst = if stack_depth == 0 {
                RegId(0) // RAX
            } else {
                RegId(3) // RBX
            };
            let disp = 16 + (*idx as i32) * 8; // byte offset from RSP
            vec![instr(
                Mnemonic::Mov,
                &[
                    Operand::Reg(dst),
                    Operand::MemSib {
                        base: RegId(4), // RSP
                        index: None,
                        scale: Scale::X1,
                        disp,
                    },
                ],
            )]
        }
        WasmOp::I32Add => {
            // Pop two i32s, push their sum. In our register model: RAX += RBX.
            vec![instr(
                Mnemonic::Add,
                &[Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
            )]
        }
        WasmOp::End => {
            // End of function body: return to caller.
            vec![instr(Mnemonic::Ret, &[])]
        }
    }
}

/// Helper to construct Instruction structs with boilerplate defaults.
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

// --- Emit driver ---

/// Emit x86_64 bytes for a sequence of WASM operations.
fn emit_body(ops: &[WasmOp]) -> Result<Vec<u8>, EmitError> {
    let mut buf = CodeBuffer::new();
    let mut depth = 0usize;
    for op in ops {
        for ins in lower(op, depth) {
            emit_instruction(&mut buf, ins)?;
        }
        // Update stack depth for next iteration
        depth = match op {
            WasmOp::LocalGet(_) => depth + 1, // push → depth increases
            WasmOp::I32Add => depth.saturating_sub(1), // pop 2, push 1 → net -1
            WasmOp::End => depth, // no change to stack depth
        };
    }
    Ok(buf.bytes)
}

// --- Roundtrip verifier (iced-x86 disassembler) ---

/// Verify emitted bytes round-trip through iced-x86 decoder.
fn verify_with_iced(bytes: &[u8]) -> Result<Vec<String>, String> {
    use iced_x86::{Decoder, DecoderOptions};

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let mut decoded = Vec::new();

    for instruction in &mut decoder {
        decoded.push(instruction);
    }

    if decoded.is_empty() {
        return Err("no instructions decoded".to_string());
    }

    let mut decoded_mnemonics = Vec::new();
    let mut offset = 0u32;
    for instruction in &decoded {
        let mnemonic = instruction.mnemonic();
        decoded_mnemonics.push(format!(
            "[{:02x}] {:?} ({} bytes)",
            offset, mnemonic, instruction.len()
        ));
        offset += instruction.len() as u32;
    }

    Ok(decoded_mnemonics)
}

// --- Main ---

fn main() {
    let function_body: &[u8] = &[0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B];

    println!("paideia-as v0.20 dynamic-emit + PQ signing demo");
    println!("================================================\n");

    // Decode WASM bytes
    println!("Input WASM byte stream (function body of `(func (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)`):");
    print!("  ");
    for byte in function_body {
        print!("{:02x} ", byte);
    }
    println!("\n");

    let ops = match decode_body(function_body) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("Error decoding WASM: {}", e);
            return;
        }
    };

    println!("Decoded WASM ops:");
    for (i, op) in ops.iter().enumerate() {
        match op {
            WasmOp::LocalGet(idx) => {
                let dst_reg = if i == 0 { "rax" } else { "rbx" };
                let frame_offset = 16 + (*idx as i32) * 8;
                println!(
                    "  [{}] local.get {}    -> mov {}, [rsp+{}]        (arg{})",
                    i, idx, dst_reg, frame_offset, idx
                );
            }
            WasmOp::I32Add => println!("  [{}] i32.add        -> add rax, rbx", i),
            WasmOp::End => println!("  [{}] end            -> ret", i),
        }
    }
    println!();

    // Emit bytes
    let emitted = match emit_body(&ops) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Error emitting: {:?}", e);
            return;
        }
    };

    println!("Emitted x86_64 bytes (via emit_instruction):");
    // Print hexdump with inline disassembly comments
    let mut offset = 0;
    for chunk in emitted.chunks(16) {
        print!("  ");
        for &byte in chunk {
            print!("{:02x} ", byte);
        }
        // Pad to align comments
        let printed = chunk.len() * 3;
        let padding = if printed < 48 { 48 - printed } else { 0 };
        print!("{}", " ".repeat(padding));

        // Disassemble this chunk on the same line
        if let Ok(decoded) = verify_with_iced(&emitted[offset..]) {
            if !decoded.is_empty() {
                print!("; {}", decoded[0].split(']').nth(1).unwrap_or("").trim());
            }
        }
        println!();
        offset += chunk.len();
    }
    println!("\nTotal: {} bytes.\n", emitted.len());

    // Round-trip verification (iced-x86)
    println!("Verification: iced-x86 round-trip");
    match verify_with_iced(&emitted) {
        Ok(decoded_mnemonics) => {
            for line in &decoded_mnemonics {
                println!("  {}", line);
            }
            println!("  OK — all instructions decoded successfully.\n");
        }
        Err(e) => {
            eprintln!("  ERROR: {}\n", e);
        }
    }

    // NEW: PQ signature round-trip
    println!("PQ Signature round-trip");
    println!("=======================\n");

    // Generate a keypair
    let (pk, sk) = Hybrid::keygen(&mut OsRng);
    println!("Generated hybrid keypair:");
    println!("  Public key length: {} bytes", pk.to_bytes().len());
    println!("  Secret key length: {} bytes", sk.to_bytes().len());
    println!();

    // Sign the emitted buffer
    let sig = sign_runtime_buffer(&sk, &emitted);
    println!("Signed the emitted buffer:");
    println!("  Signature length: {} bytes", sig.to_bytes().len());
    println!();

    // Verify the signature
    let verified = verify_runtime_buffer(&pk, &emitted, &sig);
    println!("Verification result:");
    println!("  verified: {}", verified);

    if verified {
        println!("\nSUCCESS: Signature verified over emitted bytecode.");
    } else {
        eprintln!("\nERROR: Signature verification failed!");
    }
}
