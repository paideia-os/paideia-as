//! Issue #1179: module-level let mut assignment with module-level const as RHS.
//!
//! Fixture: `current_budget = BUDGET_DEFAULT` where both are module-level symbols,
//! inside a lambda body.
//!
//! Before #1179, assigning a module-level const to a module-level let-mut would fire T0540
//! because the RHS Var arm only checked local_bindings, missing module-level Objects.
//!
//! After #1179, the elaborator recognizes module-level Object constants and emits
//! a RIP-relative load to fetch the constant value into RAX, then stores RAX to the target.
//!
//! Expected bytecode for budget_reset:
//! - `mov rax, [rip+BUDGET_DEFAULT]` (7 bytes: 48 8b 05 ?? ?? ?? ??) — load constant into RAX
//! - `mov [rip+current_budget], rax` (7 bytes: 48 89 05 ?? ?? ?? ??) — store RAX to let-mut
//! - `ret` (1 byte: c3)
//! Total: 15 bytes

use object::{Object, ObjectSection, ObjectSymbol, RelocationKind, RelocationTarget};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that module_let_mut_assign_module_const_rhs builds without T0540.
#[test]
fn budget_reset_no_t0540() {
    let out = run_build(build_emit("module_let_mut_assign_module_const_rhs.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr.contains("T0540"),
        "T0540 must not fire for module-level const as var_assign RHS; stderr:\n{}",
        out.stderr
    );
}

/// Test that budget_reset emits load-store-ret sequence with RIP-relative instructions.
#[test]
fn budget_reset_emits_load_store_ret_sequence() {
    let out = run_build(build_emit("module_let_mut_assign_module_const_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find budget_reset symbol and extract its bytes
    let mut budget_reset_bytes = None;

    eprintln!("=== Symbol table ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_addr = sym.address() as usize;
            let sym_size = sym.size() as usize;
            eprintln!("  {}: addr={}, size={}", name, sym_addr, sym_size);

            if name == "budget_reset" && sym_size > 0 && sym_addr + sym_size <= text.len() {
                budget_reset_bytes = Some(text[sym_addr..sym_addr + sym_size].to_vec());
            }
        }
    }

    let actual = budget_reset_bytes.expect("budget_reset symbol must exist");

    // Expected: mov rax, [rip+BUDGET_DEFAULT] (7 bytes: 48 8b 05 ?? ?? ?? ??)
    //         + mov [rip+current_budget], rax (7 bytes: 48 89 05 ?? ?? ?? ??)
    //         + ret (1 byte: c3)
    // Total: 15 bytes exactly
    assert_eq!(
        actual.len(),
        15,
        "budget_reset must be exactly 15 bytes (load; store; ret), got {}",
        actual.len()
    );

    // Check for RIP-relative load at offset 0: 48 8b 05 (mov rax, [rip+disp32])
    assert_eq!(
        &actual[0..3],
        &[0x48, 0x8b, 0x05],
        "budget_reset must start with RIP-relative load bytes [48 8b 05] for BUDGET_DEFAULT"
    );

    // Check for RIP-relative store at offset 7: 48 89 05 (mov [rip+disp32], rax)
    assert_eq!(
        &actual[7..10],
        &[0x48, 0x89, 0x05],
        "budget_reset must have RIP-relative store bytes [48 89 05] at offset 7 for current_budget"
    );

    // Check for RET opcode (0xc3) at byte 14
    assert_eq!(actual[14], 0xc3, "Last byte must be ret (0xc3)");

    eprintln!("budget_reset bytes: {:02X?}", actual);
}

/// Test that budget_reset has two relocations: one for BUDGET_DEFAULT load, one for current_budget store.
#[test]
fn budget_reset_has_two_relocations() {
    let out = run_build(build_emit("module_let_mut_assign_module_const_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find budget_reset symbol offset
    let mut budget_reset_offset = None;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "budget_reset" {
                budget_reset_offset = Some(sym.address() as usize);
                break;
            }
        }
    }

    let budget_reset_off = budget_reset_offset.expect("budget_reset symbol must exist");

    // Expected relocations:
    // - Offset: budget_reset_off + 3 (disp32 field of the load instruction)
    //   - Symbol: BUDGET_DEFAULT
    //   - Kind: RelocationKind::Relative (R_X86_64_PC32)
    //   - Size: 32
    //   - Addend: -4
    // - Offset: budget_reset_off + 10 (disp32 field of the store instruction)
    //   - Symbol: current_budget
    //   - Kind: RelocationKind::Relative (R_X86_64_PC32)
    //   - Size: 32
    //   - Addend: -4

    let mut relocs_found = std::collections::HashMap::new();

    eprintln!("=== Relocations ===");
    for section in file.sections() {
        for (offset, relocation) in section.relocations() {
            eprintln!("Relocation: offset={} kind={:?} addend={}", offset, relocation.kind(), relocation.addend());
            if let RelocationTarget::Symbol(sym_idx) = relocation.target() {
                if let Ok(sym) = file.symbol_by_index(sym_idx) {
                    if let Ok(name) = sym.name() {
                        let key = (offset as usize, name.to_string());
                        relocs_found.insert(key, relocation);
                    }
                }
            }
        }
    }

    // Check load relocation at budget_reset_off + 3 → BUDGET_DEFAULT
    let load_reloc_key = (budget_reset_off + 3, "BUDGET_DEFAULT".to_string());
    let load_reloc = relocs_found
        .get(&load_reloc_key)
        .expect("Must have R_X86_64_PC32 relocation at budget_reset+3 targeting BUDGET_DEFAULT");

    assert_eq!(
        load_reloc.kind(),
        RelocationKind::Relative,
        "Load relocation must be R_X86_64_PC32 (RelocationKind::Relative)"
    );
    assert_eq!(
        load_reloc.size(),
        32,
        "Load relocation must be a 32-bit displacement"
    );
    assert_eq!(
        load_reloc.addend(),
        -4,
        "Load relocation must have addend -4"
    );

    // Check store relocation at budget_reset_off + 10 → current_budget
    let store_reloc_key = (budget_reset_off + 10, "current_budget".to_string());
    let store_reloc = relocs_found
        .get(&store_reloc_key)
        .expect("Must have R_X86_64_PC32 relocation at budget_reset+10 targeting current_budget");

    assert_eq!(
        store_reloc.kind(),
        RelocationKind::Relative,
        "Store relocation must be R_X86_64_PC32 (RelocationKind::Relative)"
    );
    assert_eq!(
        store_reloc.size(),
        32,
        "Store relocation must be a 32-bit displacement"
    );
    assert_eq!(
        store_reloc.addend(),
        -4,
        "Store relocation must have addend -4"
    );

    eprintln!("✓ Verified both relocations: load BUDGET_DEFAULT at +3, store current_budget at +10");
}
