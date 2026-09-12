//! ELF section names for paideia-as emitted objects.

/// Name of the zero-length `.note.GNU-stack` marker section.
///
/// GNU/BSD toolchain convention (matched by binutils' `ld`, LLVM's `lld`,
/// and mold) on x86_64 Linux: the ABSENCE of this section makes the linker
/// assume the object requires an executable stack and emit either a
/// `PT_GNU_STACK` program header with `PF_X`, or (under
/// `--warn-common --fatal-warnings`) a fatal
/// "missing .note.GNU-stack section implies executable stack" warning.
///
/// The section is deliberately zero-length and carries `sh_type =
/// SHT_PROGBITS` with `sh_flags = 0` — no `SHF_ALLOC`, and specifically
/// no `SHF_EXECINSTR`. It is the ABSENCE of `SHF_EXECINSTR` on this named
/// section that signals a non-executable stack requirement. See binutils
/// `bfd/elflink.c` and the `gABI` supplement.
///
/// Issue: `paideia-os#1414`.
pub const GNU_STACK_SECTION: &str = ".note.GNU-stack";

/// Names of the standard ELF sections paideia-as emits, in declaration order.
///
/// Phase-1 list per `custom-assembler.md` §12.1, plus the GNU noexec-stack
/// marker required by every GNU/BSD linker on x86_64 Linux:
/// - `.text`: executable code
/// - `.rodata`: read-only data
/// - `.data`: initialized data
/// - `.bss`: uninitialized data (zero-filled)
/// - `.note.GNU-stack`: zero-length marker, `sh_flags = 0` — asks the
///   linker for a non-executable stack (issue #1414)
/// - `.symtab`: symbol table
/// - `.strtab`: string table (for symbol names)
/// - `.shstrtab`: section header string table
pub const STANDARD_SECTIONS: &[&str] = &[
    ".text",
    ".rodata",
    ".data",
    ".bss",
    GNU_STACK_SECTION,
    ".symtab",
    ".strtab",
    ".shstrtab",
];

/// Names of the PaideiaOS-specific ELF sections paideia-as emits, in declaration order.
///
/// These custom sections hold capability and effect metadata:
/// - `.paideia.caps`: capability annotations
/// - `.paideia.effects`: effect annotations
/// - `.paideia.sig`: signature or verification data
pub const PAIDEIA_SECTIONS: &[&str] = &[".paideia.caps", ".paideia.effects", ".paideia.sig"];

/// All section names paideia-as emits (standard + PaideiaOS-specific).
///
/// Returns a vector combining all standard and PaideiaOS-specific sections
/// in declaration order.
pub fn all_sections() -> Vec<&'static str> {
    let mut all = Vec::with_capacity(STANDARD_SECTIONS.len() + PAIDEIA_SECTIONS.len());
    all.extend_from_slice(STANDARD_SECTIONS);
    all.extend_from_slice(PAIDEIA_SECTIONS);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_sections_present() {
        assert!(STANDARD_SECTIONS.contains(&".text"));
        assert!(STANDARD_SECTIONS.contains(&".rodata"));
        assert!(STANDARD_SECTIONS.contains(&".data"));
        assert!(STANDARD_SECTIONS.contains(&".bss"));
        assert!(STANDARD_SECTIONS.contains(&".symtab"));
        assert!(STANDARD_SECTIONS.contains(&".strtab"));
        assert!(STANDARD_SECTIONS.contains(&".shstrtab"));
    }

    #[test]
    fn gnu_stack_marker_in_standard_sections() {
        // Issue paideia-os#1414: `.note.GNU-stack` must be a standing member
        // of the emitted section set so ld does not fall back to
        // "executable stack" under `--warn-common --fatal-warnings`.
        assert!(
            STANDARD_SECTIONS.contains(&GNU_STACK_SECTION),
            "STANDARD_SECTIONS must include the noexec-stack marker {}",
            GNU_STACK_SECTION,
        );
        assert_eq!(GNU_STACK_SECTION, ".note.GNU-stack");
    }

    #[test]
    fn paideia_sections_present() {
        assert!(PAIDEIA_SECTIONS.contains(&".paideia.caps"));
        assert!(PAIDEIA_SECTIONS.contains(&".paideia.effects"));
        assert!(PAIDEIA_SECTIONS.contains(&".paideia.sig"));
    }

    #[test]
    fn all_sections_is_union() {
        let all = all_sections();
        assert_eq!(
            all.len(),
            STANDARD_SECTIONS.len() + PAIDEIA_SECTIONS.len(),
            "all_sections should contain exactly STANDARD_SECTIONS + PAIDEIA_SECTIONS"
        );

        // Verify order: standard first, then paideia
        let expected_len = STANDARD_SECTIONS.len() + PAIDEIA_SECTIONS.len();
        assert_eq!(all.len(), expected_len);

        for (i, section) in STANDARD_SECTIONS.iter().enumerate() {
            assert_eq!(all[i], *section, "standard section mismatch at index {}", i);
        }

        for (i, section) in PAIDEIA_SECTIONS.iter().enumerate() {
            assert_eq!(
                all[STANDARD_SECTIONS.len() + i],
                *section,
                "paideia section mismatch at index {}",
                STANDARD_SECTIONS.len() + i
            );
        }
    }
}
