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
    assert_eq!(text.len(), 24, ".text must be 24 bytes (three 8-byte fns)");

    let expected = vec![
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00, 0xc3,
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00, 0xc3,
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00, 0xc3,
    ];
    assert_eq!(text, expected, ".text bytes must have three mov+ret pairs in order");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    for symbol in file.symbols() {
        if let Ok(name) = symbol.name() {
            match name {
                "fa" => {
                    assert_eq!(symbol.address(), 0);
                    assert_eq!(symbol.size(), 8);
                }
                "fb" => {
                    assert_eq!(symbol.address(), 8);
                    assert_eq!(symbol.size(), 8);
                }
                "fc" => {
                    assert_eq!(symbol.address(), 16);
                    assert_eq!(symbol.size(), 8);
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
    assert_eq!(
        offsets,
        vec![3, 11, 19],
        "#1143: three sibling stores must emit relocs at instruction-local disp32 offsets within .text"
    );
}
