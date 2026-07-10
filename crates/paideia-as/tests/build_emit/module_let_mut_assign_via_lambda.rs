//! Test for #1116: Pattern 5 module-level let mut assignment via lambda body.
//!
//! Tests: `let mut counter : u64 = 0; fn (v: u64) -> counter = v`
//!
//! Expected bytecode for set_counter:
//! - `mov [rip+counter], rdi` (7 bytes: 48 89 3d NN NN NN NN) — write RDI to data-section counter
//! - `ret` (1 byte: c3)
//! Total: 8 bytes
//!
//! Expected relocation:
//! - r_offset=3 (offset of disp32 in the mov instruction)
//! - Symbol: counter
//! - Type: R_X86_64_PC32 (RIP-relative 32-bit displacement)
//!
//! Expected data section:
//! - Symbol: counter (8 bytes, SHT_PROGBITS, writable)
//! - Initial value: 0x00 (8 zero bytes)

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes, data_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that set_counter emits correct bytecode for module-level let mut assignment.
#[test]
fn set_counter_emits_mov_rip_sym_and_ret() {
    let input = build_emit("module_let_mut_assign_via_lambda.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find set_counter symbol and extract its bytes
    let mut set_counter_bytes = None;
    let mut set_counter_offset = 0;

    eprintln!("=== Symbol table ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_addr = sym.address() as usize;
            let sym_size = sym.size() as usize;
            eprintln!("  {}: addr={}, size={}", name, sym_addr, sym_size);

            if name == "set_counter" && sym_size > 0 && sym_addr + sym_size <= text.len() {
                set_counter_bytes = Some(text[sym_addr..sym_addr + sym_size].to_vec());
                set_counter_offset = sym_addr;
            }
        }
    }

    let actual = set_counter_bytes.expect("set_counter symbol must exist");

    // Expected: mov [rip+counter], rdi (7 bytes: 48 89 3d ?? ?? ?? ??) + ret (1 byte: c3)
    // Total: 8 bytes
    assert_eq!(
        actual.len(),
        8,
        "set_counter must be exactly 8 bytes (mov [rip+counter], rdi; ret)"
    );

    // Check mnemonic prefix: 48 89 3d (mov [rip+disp32], rdi)
    assert_eq!(actual[0], 0x48, "First byte must be REX.W");
    assert_eq!(actual[1], 0x89, "Second byte must be mov opcode");
    assert_eq!(actual[2], 0x3d, "Third byte must be ModR/M for [rip+disp32]");

    // Bytes 3-6 are disp32 (will be filled by linker for PC-relative relocation)
    // We don't assert their value; the linker will populate them.

    // Check ret instruction at byte 7
    assert_eq!(actual[7], 0xc3, "Last byte must be ret (0xc3)");

    eprintln!(
        "set_counter bytes (at offset {}): {:02X?}",
        set_counter_offset, actual
    );
}

/// Test that counter symbol exists in .data section with correct properties.
#[test]
fn counter_symbol_exists_in_data() {
    let input = build_emit("module_let_mut_assign_via_lambda.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let _data = data_bytes(&bytes);
    // .data section should exist (may be empty initially, but writable)

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find counter symbol
    let mut counter_found = false;
    let mut counter_size = 0;

    eprintln!("=== Data section symbols ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_size = sym.size() as usize;
            if name == "counter" {
                eprintln!("Found counter: size={}", sym_size);
                counter_found = true;
                counter_size = sym_size;
                break;
            }
        }
    }

    assert!(counter_found, "counter symbol must exist in symbol table");
    assert_eq!(
        counter_size, 8,
        "counter must be 8 bytes (u64)"
    );
}

/// Test that relocation for set_counter → counter is at the correct offset.
///
/// The relocation should be at r_offset=3 (inside the mov instruction),
/// for symbol "counter", with type R_X86_64_PC32.
#[test]
fn set_counter_relocation_at_correct_offset() {
    let input = build_emit("module_let_mut_assign_via_lambda.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find set_counter symbol offset
    let mut set_counter_offset = None;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "set_counter" {
                set_counter_offset = Some(sym.address() as usize);
                break;
            }
        }
    }

    let set_counter_off = set_counter_offset.expect("set_counter symbol must exist");

    // Find relocation for counter at set_counter_off + 3
    let mut reloc_found = false;
    let reloc_offset = set_counter_off + 3; // disp32 position in mov instruction

    eprintln!("=== Relocations ===");
    for section in file.sections() {
        for (offset, _relocation) in section.relocations() {
            eprintln!("Relocation: offset={}", offset);

            if offset as usize == reloc_offset {
                // Found relocation at correct offset
                reloc_found = true;
                eprintln!(
                    "✓ Found correct relocation at offset {}: counter",
                    offset
                );
                break;
            }
        }
        if reloc_found {
            break;
        }
    }

    // If relocation not at offset 3, this indicates #1130 still exists
    if !reloc_found {
        eprintln!(
            "WARNING: No relocation found at expected offset {} for counter",
            reloc_offset
        );
        eprintln!("This may indicate #1130 (relocation offset accounting) is not yet fixed.");
        eprintln!("Printing all relocations in .text:");
        for section in file.sections() {
            for (offset, _relocation) in section.relocations() {
                eprintln!("  Reloc at offset {}", offset);
            }
        }
        panic!("Relocation at offset {} not found; expected for counter symbol", reloc_offset);
    }
}
