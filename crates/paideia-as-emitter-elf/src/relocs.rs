//! Relocation entries for paideia-as ELF emission.

use paideia_as_encoder::RelocKind as EncoderRelocKind;

/// Relocation kinds per `custom-assembler.md` §12.1.
///
/// Specifies the type of relocation and how the linker should patch the code.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RelocKind {
    /// PC-relative calls and references (4-byte), e.g., `[R_X86_64_PC32]`.
    PC32,
    /// 64-bit absolute references, e.g., `[R_X86_64_64]`.
    Abs64,
    /// PLT-relative 32-bit references for external function symbols.
    PLT32,
}

impl RelocKind {
    /// Convert an encoder relocation kind to ELF relocation kind.
    ///
    /// Maps encoder RelocKind (generic x86_64 ABI) to ELF-specific kinds
    /// used by the writer.
    pub fn from_encoder(kind: EncoderRelocKind) -> Self {
        match kind {
            EncoderRelocKind::PcRel32 => Self::PC32,
            EncoderRelocKind::Plt32 => Self::PLT32,
            EncoderRelocKind::Abs64 => Self::Abs64,
        }
    }
}

/// One relocation request the writer will lower into `object::write::Relocation`.
///
/// Represents a reference from one symbol to another that must be patched
/// by the linker. The writer resolves the target symbol name against the
/// symbol table before encoding.
#[derive(Clone, Debug)]
pub struct RelocEntry {
    /// Offset within the section being patched (typically `.text`).
    pub offset: u64,
    /// Target symbol name; resolved by the writer against the symbol table.
    pub target: String,
    /// Relocation kind (`[R_X86_64_PC32]` or `[R_X86_64_64]`).
    pub kind: RelocKind,
    /// Addend: adjustment to the relocation value.
    ///
    /// Typically -4 for PC32 call relocations to account for the fact that
    /// the relocation offset points at the displacement, not the end of
    /// the instruction.
    pub addend: i64,
}

impl RelocEntry {
    /// Convenience constructor for PC32 call relocations.
    ///
    /// Creates a `RelocEntry` with:
    /// - `kind: RelocKind::PC32`
    /// - `addend: -4` (standard for call relocations)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rel = RelocEntry::call(10, "main");
    /// assert_eq!(rel.offset, 10);
    /// assert_eq!(rel.target, "main");
    /// assert_eq!(rel.kind, RelocKind::PC32);
    /// assert_eq!(rel.addend, -4);
    /// ```
    #[inline]
    pub fn call(offset: u64, target: impl Into<String>) -> Self {
        Self {
            offset,
            target: target.into(),
            kind: RelocKind::PC32,
            addend: -4,
        }
    }

    /// Convenience constructor for 64-bit data reference relocations.
    ///
    /// Creates a `RelocEntry` with:
    /// - `kind: RelocKind::Abs64`
    /// - `addend: 0`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rel = RelocEntry::data64(20, "msg");
    /// assert_eq!(rel.offset, 20);
    /// assert_eq!(rel.target, "msg");
    /// assert_eq!(rel.kind, RelocKind::Abs64);
    /// assert_eq!(rel.addend, 0);
    /// ```
    #[inline]
    pub fn data64(offset: u64, target: impl Into<String>) -> Self {
        Self {
            offset,
            target: target.into(),
            kind: RelocKind::Abs64,
            addend: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_helper_uses_pc32_with_addend_minus_4() {
        let rel = RelocEntry::call(10, "foo");
        assert_eq!(rel.offset, 10);
        assert_eq!(rel.target, "foo");
        assert_eq!(rel.kind, RelocKind::PC32);
        assert_eq!(rel.addend, -4);
    }

    #[test]
    fn data64_helper_uses_abs64_with_zero_addend() {
        let rel = RelocEntry::data64(20, "msg");
        assert_eq!(rel.offset, 20);
        assert_eq!(rel.target, "msg");
        assert_eq!(rel.kind, RelocKind::Abs64);
        assert_eq!(rel.addend, 0);
    }

    #[test]
    fn call_helper_accepts_string_like() {
        let rel1 = RelocEntry::call(5, "print");
        let rel2 = RelocEntry::call(5, String::from("print"));
        assert_eq!(rel1.target, rel2.target);
    }

    #[test]
    fn data64_helper_accepts_string_like() {
        let rel1 = RelocEntry::data64(8, "data");
        let rel2 = RelocEntry::data64(8, String::from("data"));
        assert_eq!(rel1.target, rel2.target);
    }
}
