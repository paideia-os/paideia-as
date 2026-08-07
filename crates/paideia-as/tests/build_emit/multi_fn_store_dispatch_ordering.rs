//! Issue #1140: Virtual-ID ordering bug regression test.
//!
//! Verifies that sibling Store-dispatch functions emit in correct order,
//! avoiding cross-function virtual-IrNodeId collision issues that corrupt .text.
//!
//! The fixture emits two sibling lambdas (f3, g3) that both dispatch to Store.
//! Without emission_order sorting, f3's ret instruction could sort AFTER g3's
//! body, causing text corruption (missing ret, wrong instruction width, OOB reloc).

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// #1140: Regression test for multi-function Store dispatch ordering.
///
/// Fixture: two sibling lambdas (f3, g3) both dispatch to Store.
/// Expected .text:
///   - f3 (8 bytes): `48 89 3d 00 00 00 00 c3` (mov rdi, [rip+0]; ret)
///   - g3 (8 bytes): `48 89 35 00 00 00 00 c3` (mov rsi, [rip+0]; ret)
///   - Total: exactly 16 bytes
/// Symbols:
///   - f3: st_value=0, st_size=8
///   - g3: st_value=8, st_size=8
/// Relocations:
///   - Two entries in .rela.text: r_offset ∈ {3, 11}, both < 16
///   - Both types R_X86_64_PC32
///   - Symbols: sink_a, sink_b respectively
#[test]
fn multi_fn_store_dispatch_ordering_text_and_symbols() {
    let out = run_build(build_emit("multi_fn_store_dispatch_ordering.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    // paideia-as#1276 phase 3: each fn grows 8 → 16 bytes (prologue 4 +
    // epilogue 4). Total .text: 32 bytes.
    assert_eq!(
        text.len(), 32,
        ".text should be exactly 32 bytes (f3: 16, g3: 16); got {}",
        text.len()
    );

    // Verify exact bytecode sequence with frame prologue/epilogue.
    // f3: push rbp; mov rbp,rsp; mov rdi, [rip+0]; mov rsp,rbp; pop rbp; ret
    // g3: push rbp; mov rbp,rsp; mov rsi, [rip+0]; mov rsp,rbp; pop rbp; ret
    let expected = vec![
        // f3 (16 bytes)
        0x55, 0x48, 0x89, 0xE5,                         // push rbp; mov rbp, rsp
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00,       // mov [rip+0], rdi
        0x48, 0x89, 0xEC, 0x5D, 0xc3,                   // mov rsp,rbp; pop rbp; ret
        // g3 (16 bytes)
        0x55, 0x48, 0x89, 0xE5,                         // push rbp; mov rbp, rsp
        0x48, 0x89, 0x35, 0x00, 0x00, 0x00, 0x00,       // mov [rip+0], rsi
        0x48, 0x89, 0xEC, 0x5D, 0xc3,                   // mov rsp,rbp; pop rbp; ret
    ];

    if text != expected {
        eprintln!(
            "MISMATCH: .text bytes do not match expected\n\
             Expected: {:02X?}\n\
             Got:      {:02X?}",
            expected, text
        );
        panic!(".text section mismatch: expected {:02X?}, got {:02X?}", expected, text);
    }

    // Verify symbols
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let mut f3_found = false;
    let mut g3_found = false;

    for symbol in file.symbols() {
        if let Ok(name) = symbol.name() {
            match name {
                "f3" => {
                    assert_eq!(symbol.address(), 0, "f3 should start at offset 0");
                    assert_eq!(symbol.size(), 16, "f3 should be 16 bytes (paideia-as#1276 phase 3: prologue+body+epilogue)");
                    f3_found = true;
                }
                "g3" => {
                    assert_eq!(symbol.address(), 16, "g3 should start at offset 16 (after f3's 16 bytes)");
                    assert_eq!(symbol.size(), 16, "g3 should be 16 bytes (paideia-as#1276 phase 3: prologue+body+epilogue)");
                    g3_found = true;
                }
                _ => {}
            }
        }
    }

    assert!(f3_found, "Symbol f3 not found in ELF");
    assert!(g3_found, "Symbol g3 not found in ELF");

    let mut relocations: Vec<(u64, object::Relocation)> = Vec::new();
    for section in file.sections() {
        for (offset, relocation) in section.relocations() {
            relocations.push((offset, relocation));
        }
    }
    relocations.sort_by_key(|(off, _)| *off);

    assert_eq!(relocations.len(), 2, "Should have exactly 2 relocations");
    let offsets: Vec<u64> = relocations.iter().map(|(o, _)| *o).collect();
    // paideia-as#1276 phase 3: each disp32 slot shifts by +4 (prologue) plus
    // the prior fn's added 8 bytes for the second reloc.
    // f3 disp32: prologue(4) + mov head (3) = 7
    // g3 disp32: g3 base (16) + prologue (4) + mov head (3) = 23
    assert_eq!(offsets, vec![7, 23], "#1143: reloc r_offsets must be instruction-local, within .text bounds");
}
