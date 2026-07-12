//! Issue #1176: module-level Object constant as call argument.
//!
//! Fixture: `call_site(v) = take2(v, SHIFT)` where SHIFT is a module-level constant.
//! Before #1176, passing SHIFT as a non-first argument would fire T0521 because
//! module constants live in arena.symbols(), not state.local_bindings.
//!
//! After #1176, the elaborator recognizes module-level Object constants and emits
//! a RIP-relative load to fetch the constant value into the argument register.
//!
//! Expected bytecode for call_site:
//! - `mov rsi, [rip+SHIFT]` (7 bytes: 48 8b 35 ?? ?? ?? ??) — load SHIFT into RSI (arg 1)
//! - `call take2` (5 bytes: e8 ?? ?? ?? ??)
//! - `ret` (1 byte: c3)

use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that module_const_arg_call builds without T0521.
#[test]
fn module_const_arg_call_no_t0521() {
    let out = run_build(build_emit("module_const_arg_call.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr.contains("T0521"),
        "T0521 must not fire for module-level Object constant as call arg; stderr:\n{}",
        out.stderr
    );
}

/// Test that call_site emits RIP-relative load for the module constant.
#[test]
fn call_site_emits_mov_rip_sym_for_constant() {
    let out = run_build(build_emit("module_const_arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find call_site symbol and extract its bytes
    let mut call_site_bytes = None;
    let mut call_site_offset = 0;

    eprintln!("=== Symbol table ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_addr = sym.address() as usize;
            let sym_size = sym.size() as usize;
            eprintln!("  {}: addr={}, size={}", name, sym_addr, sym_size);

            if name == "call_site" && sym_size > 0 && sym_addr + sym_size <= text.len() {
                call_site_bytes = Some(text[sym_addr..sym_addr + sym_size].to_vec());
                call_site_offset = sym_addr;
            }
        }
    }

    let actual = call_site_bytes.expect("call_site symbol must exist");
    eprintln!(
        "call_site bytes (at offset {}): {:02X?}",
        call_site_offset, actual
    );

    // Expected: mov rsi, [rip+SHIFT] (7 bytes: 48 8b 35 ?? ?? ?? ??)
    //         + call take2 (5 bytes: e8 ?? ?? ?? ??)
    //         + ret (1 byte: c3)
    // Total: 13 bytes
    assert!(
        actual.len() >= 13,
        "call_site must be at least 13 bytes (mov [rip+SHIFT]; call take2; ret), got {}",
        actual.len()
    );

    // Check for RIP-relative load bytes: 48 8b 35 (mov rsi, [rip+disp32])
    // This pattern is unique to the RIP-relative load for arg 1 (RSI).
    let has_rip_load = actual.windows(3).any(|w| w == [0x48, 0x8b, 0x35]);
    assert!(
        has_rip_load,
        "call_site must contain RIP-relative load bytes [48 8b 35] for the constant; bytes:\n{:02X?}",
        actual
    );

    // Check for CALL opcode (0xe8)
    let has_call = actual.iter().any(|&b| b == 0xe8);
    assert!(
        has_call,
        ".text must contain a CALL (0xe8) for take2()"
    );

    // Check for RET opcode (0xc3)
    let has_ret = actual.iter().any(|&b| b == 0xc3);
    assert!(has_ret, ".text must contain RET (0xc3)");
}

/// Test that a relocation targets SHIFT.
#[test]
fn shift_symbol_has_relocation() {
    let out = run_build(build_emit("module_const_arg_call.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find SHIFT symbol
    let mut shift_found = false;
    eprintln!("=== Checking for SHIFT symbol ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "SHIFT" {
                eprintln!("Found SHIFT symbol");
                shift_found = true;
                break;
            }
        }
    }
    assert!(shift_found, "SHIFT symbol must exist in symbol table");

    // Check that at least one relocation targets SHIFT
    // (The RIP-relative load should have a PC32 relocation to SHIFT)
    let mut shift_relocation_found = false;
    eprintln!("=== Checking relocations ===");
    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let RelocationTarget::Symbol(sym_idx) = relocation.target() {
                if let Ok(sym) = file.symbol_by_index(sym_idx) {
                    if let Ok(name) = sym.name() {
                        eprintln!("Relocation to symbol: {}", name);
                        if name == "SHIFT" {
                            shift_relocation_found = true;
                        }
                    }
                }
            }
        }
    }
    assert!(
        shift_relocation_found,
        "At least one relocation must target SHIFT symbol"
    );
}
