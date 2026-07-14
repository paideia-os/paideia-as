//! Issue #1194 corrective 1 — byte-level verification for let-RHS BitNot.
//!
//! Tests that let-bindings with bitwise NOT (~expr) on the RHS correctly emit
//! the `not` instruction. The bug was that BitNot fell into the #1138 catch-all,
//! recording the binding but emitting zero bytes.
//!
//! Fixture: f(0x37) with mask = ~15:
//!   mask = ~15 = 0xFFFFFFFFFFFFFFF0 (in 64-bit)
//!   f(0x37) = 0x37 & 0xFFFFFFFFFFFFFFF0 = 0x30 = 48

use object::{Object, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Issue #1194: Verify that `let mask = ~15` emits the `not` instruction.
///
/// The expected bytecode for f() should contain a `not` instruction.
/// x86-64 encoding: `48 F7 D<reg>` where <reg> is the destination register.
/// Acceptable variations: D1 (rcx), D2 (rdx), D0 (r8), D1 (r9).
#[test]
fn let_with_bitnot_rhs_emits_not() {
    let out = run_build(build_emit("let_with_bitnot_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist in ELF");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Build a map of symbol name -> raw bytes.
    let mut symbol_bytes: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();

    eprintln!("=== Symbol table debug ===");
    eprintln!(".text section: {} bytes", text.len());
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_addr = sym.address() as usize;
            let sym_size = sym.size() as usize;
            eprintln!("  {}: addr={}, size={}", name, sym_addr, sym_size);

            if sym_size > 0 && sym_addr + sym_size <= text.len() {
                let func_bytes = text[sym_addr..sym_addr + sym_size].to_vec();
                symbol_bytes.insert(name.to_string(), func_bytes);
            }
        }
    }

    // Verify that f() contains a `not` instruction.
    // x86-64 encoding: 48 F7 D<reg> (3 bytes)
    // Acceptable patterns: 48 F7 D0, 48 F7 D1, 48 F7 D2, 49 F7 D0, 49 F7 D1
    let patterns = vec![
        vec![0x48, 0xF7, 0xD1], // not rcx
        vec![0x48, 0xF7, 0xD2], // not rdx
        vec![0x49, 0xF7, 0xD0], // not r8
        vec![0x49, 0xF7, 0xD1], // not r9
    ];

    if let Some(f_bytes) = symbol_bytes.get("f") {
        let mut found_not = false;
        for pattern in &patterns {
            if f_bytes.windows(3).any(|w| w == pattern.as_slice()) {
                found_not = true;
                break;
            }
        }

        if !found_not {
            eprintln!(
                "ERROR: `not` instruction not found in f(). Bytes: {:02X?}",
                f_bytes
            );
            panic!(
                "Function f() does not contain expected `not` instruction (patterns: {:?})",
                patterns
            );
        }
    } else {
        panic!("Function f() not found in symbol table");
    }
}
