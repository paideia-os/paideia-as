//! #1143: reloc offsets stay within .text for three sibling Store-dispatch functions.

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn three_fn_store_dispatch_ordering_reloc_offsets() {
    let out = run_build(build_emit("three_fn_store_dispatch_ordering.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    // paideia-as#1276 phase 3: each 8-byte fn grows by 8 (prologue+epilogue) → 16 bytes each.
    assert_eq!(text.len(), 48, ".text must be 48 bytes (three 16-byte fns with prologue+epilogue)");

    let one_fn = |disp: u8| -> Vec<u8> {
        vec![
            0x55, 0x48, 0x89, 0xE5,                                 // push rbp; mov rbp, rsp
            0x48, 0x89, disp, 0x00, 0x00, 0x00, 0x00,                // mov [rip+disp], rdi/rsi/rdx
            0x48, 0x89, 0xEC, 0x5D, 0xC3,                            // mov rsp,rbp; pop rbp; ret
        ]
    };
    let mut expected = Vec::new();
    expected.extend(one_fn(0x3d));
    expected.extend(one_fn(0x3d));
    expected.extend(one_fn(0x3d));
    assert_eq!(text, expected, ".text bytes must have three prologue+mov+epilogue+ret sequences in order");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    for symbol in file.symbols() {
        if let Ok(name) = symbol.name() {
            match name {
                "fa" => {
                    assert_eq!(symbol.address(), 0);
                    assert_eq!(symbol.size(), 16);
                }
                "fb" => {
                    assert_eq!(symbol.address(), 16);
                    assert_eq!(symbol.size(), 16);
                }
                "fc" => {
                    assert_eq!(symbol.address(), 32);
                    assert_eq!(symbol.size(), 16);
                }
                _ => {}
            }
        }
    }

    let mut offsets: Vec<u64> = Vec::new();
    for section in file.sections() {
        for (offset, _rel) in section.relocations() {
            offsets.push(offset);
        }
    }
    offsets.sort();
    // Each disp32 slot sits at (fn_base + prologue(4) + mov head(3)).
    assert_eq!(
        offsets,
        vec![7, 23, 39],
        "#1143: three sibling stores must emit relocs at instruction-local disp32 offsets within .text"
    );
}
