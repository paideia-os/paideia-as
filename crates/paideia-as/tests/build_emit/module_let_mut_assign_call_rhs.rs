//! #1136: module-let-mut assignment with an App RHS (call-materialization to RAX).

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::text_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn bump_stores_call_result_via_rax() {
    let out = run_build(build_emit("module_let_mut_assign_call_rhs.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on App RHS after #1136");
    assert!(!out.stderr.contains("T0521"), "T0521 must not fire on this single-arg 0-index call");

    let bytes = out.artifact_bytes();
    let text = text_bytes(&bytes);
    assert!(
        text.windows(1).any(|w| w[0] == 0xe8),
        ".text must contain a CALL (0xe8 opcode)"
    );
    assert!(
        text.windows(3).any(|w| w == [0x48, 0x89, 0x05]),
        ".text must contain `mov [rip+sink], rax` (48 89 05) for the App-RHS store"
    );

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let mut sink_reloc = false;
    let mut helper_reloc = false;
    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "sink" {
                            sink_reloc = true;
                        }
                        if name == "helper" {
                            helper_reloc = true;
                        }
                    }
                }
            }
        }
    }
    assert!(sink_reloc, "must have a reloc targeting `sink` (the RIP-relative store)");
    assert!(helper_reloc, "must have a reloc targeting `helper` (the CALL)");
}
