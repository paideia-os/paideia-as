//! Issues #1188 (Var tail) and #1189 (module-qualified FieldAccess tail):
//! emit_block_body_arm was missing arms for these tail shapes.

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn var_tail_in_each_arm_emits_mov_rax_from_binding() {
    let input = build_emit("match_arm_var_tail.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let f_bytes = crate::common::elf::symbol_bytes(&bytes, "f")
        .expect("symbol 'f' should exist in .text");

    let seq = [0x48u8, 0x89, 0xf0];
    let count = f_bytes.windows(3).filter(|w| *w == seq).count();
    assert!(count >= 2,
        "expected >= 2 `mov rax, rsi` (48 89 f0) sequences in f, got {}: {:02X?}",
        count, f_bytes);
}

#[test]
fn module_field_read_tail_in_each_arm_emits_rip_load() {
    let input = build_emit("match_arm_module_field_read_tail.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let f_bytes = crate::common::elf::symbol_bytes(&bytes, "f")
        .expect("symbol 'f' should exist in .text");

    let seq = [0x48u8, 0x8b, 0x05];
    let count = f_bytes.windows(3).filter(|w| *w == seq).count();
    assert!(count >= 2,
        "expected >= 2 `mov rax, [rip+disp32]` (48 8b 05) sequences, got {}: {:02X?}",
        count, f_bytes);
}

#[test]
fn module_field_read_tail_reloc_present() {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
    let input = build_emit("match_arm_module_field_read_tail.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("parse ELF");
    let text = file.sections().find(|s| s.name().unwrap_or("") == ".text").expect(".text");
    let mut found = false;
    for (_offset, reloc) in text.relocations() {
        if let RelocationTarget::Symbol(idx) = reloc.target() {
            if file.symbol_by_index(idx).ok().and_then(|s| s.name().ok()) == Some("_current_tcb") {
                found = true; break;
            }
        }
    }
    assert!(found, ".rela.text should contain relocation to '_current_tcb'");
}
