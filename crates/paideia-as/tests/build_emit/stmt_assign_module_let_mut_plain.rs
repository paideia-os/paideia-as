//! Test for #1094: Assign statement in plain action block (non-unsafe).
//!
//! Tests: `let mut counter : u64 = 0; fn (v: u64) -> { counter = v; v }`
//!
//! Expected bytecode for bump:
//! - `mov [rip+counter], rdi` (7 bytes: 48 89 3d NN NN NN NN) — write RDI to data-section counter
//! - `mov rax, rdi` (3 bytes: 48 89 c8) — return v
//! - `ret` (1 byte: c3)
//! Total: 11 bytes
//!
//! Expected relocation:
//! - r_offset=3 (offset of disp32 in the mov instruction)
//! - Symbol: counter
//! - Type: R_X86_64_PC32 (RIP-relative 32-bit displacement)

use object::{Object, ObjectSection, ObjectSymbol, RelocationKind, RelocationTarget};

use crate::common::elf::{assert_elf64_magic, text_bytes, data_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test that bump emits correct bytecode for plain action block assignment.
#[test]
fn bump_emits_mov_rip_sym_and_ret() {
    let input = build_emit("stmt_assign_module_let_mut_plain.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find bump symbol and extract its bytes
    let mut bump_bytes = None;
    let mut bump_offset = 0;

    eprintln!("=== Symbol table ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_addr = sym.address() as usize;
            let sym_size = sym.size() as usize;
            eprintln!("  {}: addr={}, size={}", name, sym_addr, sym_size);

            if name == "bump" && sym_size > 0 && sym_addr + sym_size <= text.len() {
                bump_bytes = Some(text[sym_addr..sym_addr + sym_size].to_vec());
                bump_offset = sym_addr;
            }
        }
    }

    let actual = bump_bytes.expect("bump symbol must exist");

    eprintln!(
        "bump bytes (at offset {}): {:02X?}",
        bump_offset, actual
    );

    // Adversarial-verify of #1094: tightened from a weak `>= 8` lower bound to an
    // exact byte count. Post-#1161, final Var expressions are moved to RAX for return,
    // so the expected shape is `mov [rip+counter], rdi` (7 bytes) + `mov rax, rdi` (3 bytes) + `ret` (1 byte) = 11 bytes.
    // A loose `>= 8` bound would not have caught a double-emitted Store (e.g. two
    // copies of the 7-byte mov), which is exactly the class of regression this
    // fixture exists to guard against (see dispatch_store's now-removed dead
    // mark_store_emitted() call).
    assert_eq!(
        actual.len(),
        11,
        "bump must be exactly 11 bytes (mov [rip+counter], rdi; mov rax, rdi; ret) — got {} bytes: {:02X?}; \
         a longer length suggests the Store was emitted more than once",
        actual.len(),
        actual
    );

    // Check mnemonic prefix for first instruction: 48 89 3d (mov [rip+disp32], rdi)
    assert_eq!(actual[0], 0x48, "First byte must be REX.W");
    assert_eq!(actual[1], 0x89, "Second byte must be mov opcode");
    assert_eq!(actual[2], 0x3d, "Third byte must be ModR/M for [rip+disp32]");

    // Post-#1161: The function must end in ret (0xc3) at exactly offset 10 — after the
    // mov rax, rdi (3 bytes at offset 7-9). Not folded into, or displaced by, a sibling
    // function's bytes (see #1140).
    assert_eq!(
        actual[10], 0xc3,
        "bump's 11th byte (offset 10) must be ret (0xc3); got {:02X}",
        actual[10]
    );
}

/// Test that counter symbol exists in .data section with correct properties.
#[test]
fn counter_symbol_exists_in_data_plain() {
    let input = build_emit("stmt_assign_module_let_mut_plain.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let _data = data_bytes(&bytes);
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut counter_syms = Vec::new();

    eprintln!("=== Data section symbols ===");
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "counter" {
                eprintln!(
                    "Found counter: size={}, section_index={:?}",
                    sym.size(),
                    sym.section_index()
                );
                counter_syms.push(sym);
            }
        }
    }

    assert_eq!(
        counter_syms.len(),
        1,
        "expected exactly one 'counter' symbol, found {}",
        counter_syms.len()
    );
    let counter_sym = &counter_syms[0];
    assert_eq!(counter_sym.size(), 8, "counter must be 8 bytes (u64)");
    assert!(counter_sym.is_definition(), "counter must be a defined symbol, not undefined");

    // Confirm it actually lives in the .data section
    let section_index = counter_sym
        .section_index()
        .expect("counter symbol must have a section index");
    let section = file
        .section_by_index(section_index)
        .expect("counter's section index should resolve");
    assert_eq!(
        section.name().unwrap_or(""),
        ".data",
        "counter symbol must live in .data section"
    );
}

/// Test that relocation for bump → counter is at the correct offset.
#[test]
fn bump_relocation_at_correct_offset() {
    let input = build_emit("stmt_assign_module_let_mut_plain.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find bump symbol offset
    let mut bump_offset = None;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "bump" {
                bump_offset = Some(sym.address() as usize);
                break;
            }
        }
    }

    let bump_off = bump_offset.expect("bump symbol must exist");
    let reloc_offset = bump_off + 3; // disp32 position in mov instruction

    // .text must be large enough that the disp32 field (4 bytes starting at
    // the relocation offset) fits entirely inside it
    assert!(
        reloc_offset + 4 <= text.len(),
        "#1130 regression: disp32 field [{}, {}) falls outside .text (len {})",
        reloc_offset,
        reloc_offset + 4,
        text.len()
    );

    // Find the relocation at bump_off + 3
    let mut matched = None;

    eprintln!("=== Relocations ===");
    for section in file.sections() {
        for (offset, relocation) in section.relocations() {
            eprintln!("Relocation: offset={} kind={:?} addend={}", offset, relocation.kind(), relocation.addend());
            if offset as usize == reloc_offset {
                matched = Some(relocation);
                break;
            }
        }
        if matched.is_some() {
            break;
        }
    }

    let relocation = matched.unwrap_or_else(|| {
        panic!(
            "#1094 regression: no relocation found at offset {} (disp32 of bump's mov); \
             the assignment did not emit properly",
            reloc_offset
        );
    });

    // Type: R_X86_64_PC32
    assert_eq!(
        relocation.kind(),
        RelocationKind::Relative,
        "relocation at offset {} must be R_X86_64_PC32 (RelocationKind::Relative)",
        reloc_offset
    );
    assert_eq!(
        relocation.size(),
        32,
        "relocation at offset {} must be a 32-bit displacement",
        reloc_offset
    );

    // Addend: -4 for PC32
    assert_eq!(
        relocation.addend(),
        -4,
        "relocation at offset {} must have addend -4 (standard PC32 disp)",
        reloc_offset
    );

    // Symbol: must be "counter"
    match relocation.target() {
        RelocationTarget::Symbol(sym_idx) => {
            let sym = file
                .symbol_by_index(sym_idx)
                .expect("relocation target symbol index must resolve");
            assert_eq!(
                sym.name().unwrap_or(""),
                "counter",
                "relocation at offset {} must target symbol 'counter'",
                reloc_offset
            );
        }
        other => panic!(
            "relocation at offset {} must target a symbol, got {:?}",
            reloc_offset, other
        ),
    }

    eprintln!(
        "✓ Verified relocation at offset {}: kind={:?}, size=32, addend=-4, symbol=counter",
        reloc_offset,
        relocation.kind()
    );
}
