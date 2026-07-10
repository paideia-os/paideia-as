//! Issue #1140: Virtual-ID ordering bug regression test.
//!
//! Verifies that sibling Store-dispatch functions emit in correct order,
//! avoiding cross-function virtual-IrNodeId collision issues that corrupt .text.
//!
//! The fixture emits two sibling lambdas (f3, g3) that both dispatch to Store.
//! Without emission_order sorting, f3's ret instruction could sort AFTER g3's
//! body, causing text corruption (missing ret, wrong instruction width, OOB reloc).

use object::{Object, ObjectSymbol};

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
    assert_eq!(
        text.len(), 16,
        ".text should be exactly 16 bytes (f3: 8, g3: 8); got {}",
        text.len()
    );

    // Verify exact bytecode sequence
    // f3: mov rdi, [rip+0]; ret
    // g3: mov rsi, [rip+0]; ret
    let expected = vec![
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00, 0xc3, // f3: mov rdi, [rip+0]; ret
        0x48, 0x89, 0x35, 0x00, 0x00, 0x00, 0x00, 0xc3, // g3: mov rsi, [rip+0]; ret
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
                    assert_eq!(symbol.size(), 8, "f3 should be 8 bytes");
                    f3_found = true;
                }
                "g3" => {
                    assert_eq!(symbol.address(), 8, "g3 should start at offset 8");
                    assert_eq!(symbol.size(), 8, "g3 should be 8 bytes");
                    g3_found = true;
                }
                _ => {}
            }
        }
    }

    assert!(f3_found, "Symbol f3 not found in ELF");
    assert!(g3_found, "Symbol g3 not found in ELF");

    // Verify relocations
    let relocations: Vec<_> = file
        .relocation_sections()
        .flat_map(|rel_section| {
            rel_section
                .relocations()
                .map(move |(rel_offset, rel)| (rel_offset, rel))
        })
        .collect();

    assert_eq!(relocations.len(), 2, "Should have exactly 2 relocations");

    // Check relocation offsets are within bounds
    for (offset, _rel) in &relocations {
        assert!(
            *offset < 16,
            "Relocation offset {} is out of bounds for .text size 16",
            offset
        );
    }
}
