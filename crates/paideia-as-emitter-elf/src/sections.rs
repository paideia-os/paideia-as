//! ELF section names for paideia-as emitted objects.

/// Names of the standard ELF sections paideia-as emits, in declaration order.
///
/// Phase-1 list per `custom-assembler.md` §12.1:
/// - `.text`: executable code
/// - `.rodata`: read-only data
/// - `.data`: initialized data
/// - `.bss`: uninitialized data (zero-filled)
/// - `.symtab`: symbol table
/// - `.strtab`: string table (for symbol names)
/// - `.shstrtab`: section header string table
pub const STANDARD_SECTIONS: &[&str] = &[
    ".text",
    ".rodata",
    ".data",
    ".bss",
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
