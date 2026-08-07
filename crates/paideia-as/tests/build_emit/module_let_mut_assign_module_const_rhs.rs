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

    // paideia-as#1276 phase 3: 4-byte prologue + 7-byte load + 7-byte store +
    // 4-byte epilogue + 1-byte ret = 23 bytes. Load starts at offset 4, store at 11,
    // epilogue at 18, ret at 22.
    assert_eq!(
        actual.len(),
        23,
        "budget_reset must be exactly 23 bytes (prologue + load + store + epilogue + ret), got {}",
        actual.len()
    );

    // Prologue at [0..4]: 55 48 89 E5 (push rbp; mov rbp, rsp)
    assert_eq!(&actual[0..4], &[0x55, 0x48, 0x89, 0xE5],
        "budget_reset must start with frame prologue [55 48 89 E5]");
    // Load at [4..11]: 48 8b 05 ?? ?? ?? ??
    assert_eq!(&actual[4..7], &[0x48, 0x8b, 0x05],
        "budget_reset must have RIP-relative load bytes [48 8b 05] at offset 4 for BUDGET_DEFAULT");
    // Store at [11..18]: 48 89 05 ?? ?? ?? ??
    assert_eq!(&actual[11..14], &[0x48, 0x89, 0x05],
        "budget_reset must have RIP-relative store bytes [48 89 05] at offset 11 for current_budget");
    // Epilogue + ret at [18..23]
    assert_eq!(&actual[18..23], &[0x48, 0x89, 0xEC, 0x5D, 0xC3],
        "budget_reset must end with epilogue+ret [48 89 EC 5D C3]");

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

    // paideia-as#1276 phase 3: 4-byte prologue shifted the load's disp32
    // slot from +3 to +7 (prologue + 3 bytes of mov head).
    let load_reloc_key = (budget_reset_off + 7, "BUDGET_DEFAULT".to_string());
    let load_reloc = relocs_found
        .get(&load_reloc_key)
        .expect("Must have R_X86_64_PC32 relocation at budget_reset+7 targeting BUDGET_DEFAULT");

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

    // paideia-as#1276 phase 3: store's disp32 slot shifts from +10 to +14
    // (+4 for prologue).
    let store_reloc_key = (budget_reset_off + 14, "current_budget".to_string());
    let store_reloc = relocs_found
        .get(&store_reloc_key)
        .expect("Must have R_X86_64_PC32 relocation at budget_reset+14 targeting current_budget");

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
