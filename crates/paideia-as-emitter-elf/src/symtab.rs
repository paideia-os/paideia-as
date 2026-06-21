//! Symbol table entries for paideia-as ELF emission.

use object::SymbolKind;
use object::write::SymbolId;
use paideia_as_ir::SectionKind;

/// Type alias for a symbol index in the ELF symbol table.
/// Represents the handle returned by the object crate when adding a symbol.
pub type SymbolIndex = SymbolId;

/// Symbol table entry kinds paideia-as emits per `custom-assembler.md` §12.1.
///
/// Maps directly to ELF symbol types: `[STT_FUNC]`, `[STT_OBJECT]`, and undefined.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SymKind {
    /// Exported function (`[STT_FUNC]`).
    Func,
    /// Data label (`[STT_OBJECT]`).
    Data,
    /// Undefined external reference (resolved by the linker).
    Undefined,
}

impl SymKind {
    /// Map to the `object` crate's [`SymbolKind`].
    ///
    /// - `SymKind::Func` → [`SymbolKind::Text`]
    /// - `SymKind::Data` → [`SymbolKind::Data`]
    /// - `SymKind::Undefined` → [`SymbolKind::Unknown`]
    pub fn to_object_kind(self) -> SymbolKind {
        match self {
            SymKind::Func => SymbolKind::Text,
            SymKind::Data => SymbolKind::Data,
            SymKind::Undefined => SymbolKind::Unknown,
        }
    }
}

/// One symbol-table entry.
///
/// Phase-1 is a small POD; the writer translates to `object::write::Symbol`
/// at finalization. Represents either a defined symbol (with offset and size)
/// or an undefined external reference.
#[derive(Clone, Debug)]
pub struct SymbolEntry {
    /// The symbol's name in the string table.
    pub name: String,
    /// The kind of symbol (`[STT_FUNC]`, `[STT_OBJECT]`, or undefined).
    pub kind: SymKind,
    /// Whether this symbol is globally visible (`[STB_GLOBAL]` vs `[STB_LOCAL]`).
    pub is_global: bool,
    /// Offset within the symbol's owning section, or `None` for an undefined external.
    pub offset: Option<u64>,
    /// Size in bytes; 0 for undefined symbols.
    pub size: u64,
    /// Which section this symbol belongs to. Phase 6 m5-003: used for .bss symbols.
    pub section: Option<SectionKind>,
}

impl SymbolEntry {
    /// Construct a function symbol.
    ///
    /// Creates a globally visible function symbol with the given name, offset, and size.
    /// Equivalent to `SymbolEntry { name, kind: SymKind::Func, is_global: true, offset: Some(offset), size, section: None }`.
    #[inline]
    pub fn func(name: impl Into<String>, offset: u64, size: u64) -> Self {
        Self {
            name: name.into(),
            kind: SymKind::Func,
            is_global: true,
            offset: Some(offset),
            size,
            section: None,
        }
    }

    /// Construct a data symbol.
    ///
    /// Creates a globally visible data symbol with the given name, offset, and size.
    /// Equivalent to `SymbolEntry { name, kind: SymKind::Data, is_global: true, offset: Some(offset), size, section: None }`.
    #[inline]
    pub fn data(name: impl Into<String>, offset: u64, size: u64) -> Self {
        Self {
            name: name.into(),
            kind: SymKind::Data,
            is_global: true,
            offset: Some(offset),
            size,
            section: None,
        }
    }

    /// Construct a data symbol with explicit section kind.
    ///
    /// Creates a globally visible data symbol with the given name, offset, size, and section.
    /// Phase 6 m5-003: used for .bss symbols.
    #[inline]
    pub fn data_with_section(
        name: impl Into<String>,
        offset: u64,
        size: u64,
        section: SectionKind,
    ) -> Self {
        Self {
            name: name.into(),
            kind: SymKind::Data,
            is_global: true,
            offset: Some(offset),
            size,
            section: Some(section),
        }
    }

    /// Construct an undefined external symbol.
    ///
    /// Creates a globally visible undefined symbol (resolved by the linker) with no offset.
    /// Equivalent to `SymbolEntry { name, kind: SymKind::Undefined, is_global: true, offset: None, size: 0, section: None }`.
    #[inline]
    pub fn undefined(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: SymKind::Undefined,
            is_global: true,
            offset: None,
            size: 0,
            section: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_kinds_map_to_object_kinds() {
        assert_eq!(SymKind::Func.to_object_kind(), SymbolKind::Text);
        assert_eq!(SymKind::Data.to_object_kind(), SymbolKind::Data);
        assert_eq!(SymKind::Undefined.to_object_kind(), SymbolKind::Unknown);
    }

    #[test]
    fn func_helper_constructs_global_symbol() {
        let sym = SymbolEntry::func("main", 0, 5);
        assert_eq!(sym.name, "main");
        assert_eq!(sym.kind, SymKind::Func);
        assert!(sym.is_global);
        assert_eq!(sym.offset, Some(0));
        assert_eq!(sym.size, 5);
    }

    #[test]
    fn undefined_helper_has_no_offset() {
        let sym = SymbolEntry::undefined("printf");
        assert_eq!(sym.name, "printf");
        assert_eq!(sym.kind, SymKind::Undefined);
        assert!(sym.is_global);
        assert!(sym.offset.is_none());
        assert_eq!(sym.size, 0);
    }

    #[test]
    fn data_helper_constructs_data_symbol() {
        let sym = SymbolEntry::data("msg", 10, 20);
        assert_eq!(sym.name, "msg");
        assert_eq!(sym.kind, SymKind::Data);
        assert!(sym.is_global);
        assert_eq!(sym.offset, Some(10));
        assert_eq!(sym.size, 20);
    }
}
