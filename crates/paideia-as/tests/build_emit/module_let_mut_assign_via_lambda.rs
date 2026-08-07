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

use object::{Object, ObjectSection, ObjectSymbol, RelocationKind, RelocationTarget};

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

    // paideia-as#1276 phase 3: 4-byte prologue (55 48 89 E5) at head + 7-byte
    // mov + 4-byte epilogue (48 89 EC 5D) + 1-byte ret = 16 bytes.
    // The mov starts at offset 4; ret is at offset 15.
    assert_eq!(
        actual.len(),
        16,
        "set_counter must be exactly 16 bytes (prologue + mov [rip+counter], rdi + epilogue + ret)"
    );

    // Prologue prefix: 55 48 89 E5 (push rbp; mov rbp, rsp)
    assert_eq!(actual[0], 0x55, "Byte 0 must be `push rbp` (0x55)");
    assert_eq!(actual[1], 0x48, "Byte 1 must be REX.W (prefix of mov rbp,rsp)");
    assert_eq!(actual[2], 0x89, "Byte 2 must be MOV opcode");
    assert_eq!(actual[3], 0xE5, "Byte 3 must be ModR/M for mov rbp,rsp");

    // Store instruction at byte 4-10: 48 89 3d (mov [rip+disp32], rdi) + disp32
    assert_eq!(actual[4], 0x48, "Byte 4 must be REX.W of store");
    assert_eq!(actual[5], 0x89, "Byte 5 must be mov opcode");
    assert_eq!(actual[6], 0x3d, "Byte 6 must be ModR/M for [rip+disp32], rdi");

    // Bytes 7-10 are disp32 (linker-filled). Epilogue at 11-14, ret at 15.
    assert_eq!(actual[11], 0x48, "Byte 11 must be REX.W of `mov rsp, rbp`");
    assert_eq!(actual[12], 0x89, "Byte 12 must be MOV opcode");
    assert_eq!(actual[13], 0xEC, "Byte 13 must be ModR/M for `mov rsp, rbp`");
    assert_eq!(actual[14], 0x5D, "Byte 14 must be `pop rbp`");
    assert_eq!(actual[15], 0xc3, "Byte 15 must be `ret`");

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

    // Find ALL symbols named "counter" (not just the first) so a duplicate
    // (e.g. a leftover `data_<node_id>` entry aliasing the same offset, or a
    // second `counter` symbol from a second population pass — see #1134)
    // is caught rather than silently ignored by an early `break`.
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

    // Confirm it actually lives in the .data section (not .bss/.rodata/.text).
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

    // No dangling `data_<node_id>` symbol should exist alongside `counter`
    // (would indicate the populate_data_table / inline-scanner duplication
    // risk flagged in #1134 has resurfaced).
    let stray_data_syms: Vec<&str> = file
        .symbols()
        .filter_map(|s| s.name().ok())
        .filter(|n| n.starts_with("data_") && n[5..].chars().all(|c| c.is_ascii_digit()))
        .collect();
    assert!(
        stray_data_syms.is_empty(),
        "found stray data_<id> symbol(s) alongside 'counter': {:?}",
        stray_data_syms
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
    // paideia-as#1276 phase 3: 4-byte frame prologue precedes the mov, so
    // the disp32 slot is now at prologue(4) + mov head (3) = +7 relative to
    // the function symbol (was +3 pre-#1276).
    let reloc_offset = set_counter_off + 7;

    // .text must be large enough that the disp32 field (4 bytes starting at
    // the relocation offset) fits entirely inside it -- this is the actual
    // #1130 OOB check, not just "some relocation exists at this offset".
    assert!(
        reloc_offset + 4 <= text.len(),
        "#1130 regression: disp32 field [{}, {}) falls outside .text (len {})",
        reloc_offset,
        reloc_offset + 4,
        text.len()
    );

    // Find the relocation at set_counter_off + 3 and verify its symbol,
    // type, and addend -- not just that *a* relocation exists at that offset.
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
            "#1130 regression: no relocation found at offset {} (disp32 of set_counter's mov); \
             the OOB relocation offset bug is back",
            reloc_offset
        );
    });

    // Type: R_X86_64_PC32 decodes to RelocationKind::Relative with a 32-bit size in `object`.
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

    // Addend: standard PC32 addend is -4 (offset from end of the 4-byte
    // disp32 field back to the start, since RIP at execution time already
    // points past the whole instruction).
    assert_eq!(
        relocation.addend(),
        -4,
        "relocation at offset {} must have addend -4 (standard PC32 disp)",
        reloc_offset
    );

    // Symbol: must resolve to "counter", not some other symbol (e.g. a
    // stray data_<id> alias) that happens to share the offset.
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
