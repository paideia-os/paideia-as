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

    // Adversarial-verify of #1094 + paideia-as#1276 phase 3:
    //   4-byte prologue (55 48 89 E5)
    //   7-byte store (48 89 3d ?? ?? ?? ??)
    //   3-byte `mov rax, rdi` (48 89 f8)
    //   4-byte epilogue (48 89 EC 5D)
    //   1-byte ret (C3)
    // = 19 bytes exact. A loose `>= 8` bound would not have caught a double-
    // emitted Store, which is exactly the class of regression this fixture
    // guards against.
    assert_eq!(
        actual.len(),
        19,
        "bump must be exactly 19 bytes (prologue + mov [rip+counter], rdi + mov rax, rdi + epilogue + ret) \
         — got {} bytes: {:02X?}; a longer length suggests the Store was emitted more than once",
        actual.len(),
        actual
    );

    // Prologue at [0..4]: 55 48 89 E5
    assert_eq!(&actual[..4], &[0x55, 0x48, 0x89, 0xE5], "bytes 0..4 must be push rbp; mov rbp,rsp");
    // Store at [4..11]: 48 89 3d ?? ?? ?? ??
    assert_eq!(actual[4], 0x48, "Byte 4 must be REX.W of store");
    assert_eq!(actual[5], 0x89, "Byte 5 must be mov opcode");
    assert_eq!(actual[6], 0x3d, "Byte 6 must be ModR/M for [rip+disp32]");
    // Epilogue + ret at [14..19]: 48 89 EC 5D C3
    assert_eq!(&actual[14..19], &[0x48, 0x89, 0xEC, 0x5D, 0xC3],
        "bytes 14..19 must be mov rsp,rbp; pop rbp; ret");
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
    // paideia-as#1276 phase 3: 4-byte frame prologue precedes the mov, so
    // the disp32 slot is now at prologue(4) + mov head (3) = +7 relative to
    // the function symbol (was +3 pre-#1276).
    let reloc_offset = bump_off + 7;

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
