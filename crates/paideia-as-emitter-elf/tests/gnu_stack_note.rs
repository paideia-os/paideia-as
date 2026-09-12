//! Regression test pinning the `.note.GNU-stack` noexec-stack marker.
//!
//! Issue paideia-os#1414: `paideia-as build --emit elf64 X.pdx -o X.o`
//! previously produced objects lacking this section. Under
//! `ld --warn-common --fatal-warnings` (the satellite-build convention
//! across paideia-os), the missing marker triggers
//! "warning: missing .note.GNU-stack section implies executable stack"
//! and `--fatal-warnings` promotes it to a link error.
//!
//! Every mainstream x86_64 Linux assembler (GNU as, LLVM/clang) emits
//! this section unconditionally; paideia-as must match. The section is
//! zero-length by convention — the linker keys off the name and the
//! ABSENCE of `SHF_EXECINSTR` in its flags.

use object::{Object, ObjectSection, SectionFlags};
use paideia_as_emitter_elf::{Arch, ElfWriter, GNU_STACK_SECTION, Kind};

#[test]
fn emitted_relocatable_object_carries_gnu_stack_marker() {
    // Emit a minimal-but-realistic object: a few bytes of code so the
    // fix is exercised on the same shape that satellite builds see, not
    // just on the empty-writer path.
    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    let _ = writer.add_text_bytes(&[0x90, 0x90, 0xc3]); // nop; nop; ret
    let bytes = writer.finalize().expect("finalize must succeed");

    let elf = object::File::parse(bytes.as_slice()).expect("emitted bytes must parse as ELF");

    let section = elf
        .sections()
        .find(|s| s.name().unwrap_or("") == GNU_STACK_SECTION)
        .unwrap_or_else(|| {
            let names: Vec<String> = elf
                .sections()
                .map(|s| s.name().unwrap_or("").to_string())
                .collect();
            panic!(
                "emitted object must contain {} section; got: {:?}",
                GNU_STACK_SECTION, names,
            )
        });

    // Zero-length: the section is a marker, not a payload.
    assert_eq!(
        section.size(),
        0,
        "{} must be zero-length by convention",
        GNU_STACK_SECTION,
    );

    // sh_flags == 0 (no SHF_ALLOC, and specifically no SHF_EXECINSTR).
    // Having SHF_EXECINSTR here would flip the marker's meaning and
    // reintroduce the executable-stack fallback the fix was meant to
    // remove.
    match section.flags() {
        SectionFlags::Elf { sh_flags } => assert_eq!(
            sh_flags, 0,
            "{} must have sh_flags == 0; got 0x{:x}",
            GNU_STACK_SECTION, sh_flags,
        ),
        other => panic!(
            "{} flags must be SectionFlags::Elf, got {:?}",
            GNU_STACK_SECTION, other,
        ),
    }
}
