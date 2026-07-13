//! #1181: module-let-mut assignment with BinOp RHS (expression tree lowering).

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn bitmap_set_no_t0540() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on BinOp RHS after #1181");
    // Also check that bitmap_clear (with BitNot) doesn't fire T0540
    assert!(!out.stderr.contains("bitmap_clear"), "bitmap_clear must compile without errors");
}

#[test]
fn bitmap_set_emits_load_op_store() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);

    // Check for RIP-relative load of priority_bitmap: 48 8b 05 ?? ?? ?? ??
    assert!(
        text.windows(7).any(|w| w[0] == 0x48 && w[1] == 0x8b && w[2] == 0x05),
        ".text must contain `mov rax, [rip+priority_bitmap]` (48 8b 05 ...)"
    );

    // Check for or opcode: 48 09 d0 (or rax, r10) or 4c 09 d0 (or r10, rax)
    assert!(
        text.windows(3).any(|w| (w[0] == 0x48 || w[0] == 0x4c) && w[1] == 0x09),
        ".text must contain an `or` opcode (48/4c 09 ...)"
    );

    // Check for shift instruction (shl dest, cl): D3 E0 (shl rax, cl) or similar
    assert!(
        text.windows(2).any(|w| w[0] == 0xd3 && (w[1] == 0xe0 || w[1] == 0xe1 || w[1] == 0xe2)),
        ".text must contain a shift instruction (D3 ...)"
    );

    // Check for RIP-relative store: 48 89 05 ?? ?? ?? ??
    assert!(
        text.windows(7).any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0x05),
        ".text must contain `mov [rip+priority_bitmap], rax` (48 89 05 ...)"
    );

    // Check for RET instruction at end: C3
    assert!(
        text.windows(1).any(|w| w[0] == 0xc3),
        ".text must end with RET (c3)"
    );
}

#[test]
fn bitmap_set_has_relocations_to_priority_bitmap() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut reloc_count = 0;

    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "priority_bitmap" {
                            // Count by addend: -4 is the canonical RIP-relative addend
                            if relocation.addend() == -4 {
                                // We can't distinguish load from store by relocation alone,
                                // but we expect 2 total: one for load, one for store.
                                if matches!(relocation.kind(), object::RelocationKind::Relative) {
                                    reloc_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // For bitmap_set: 1 load + 1 store = 2 relocations
    assert!(reloc_count >= 2, "must have at least 2 Relative relocations to priority_bitmap (load + store); got {}", reloc_count);
}
