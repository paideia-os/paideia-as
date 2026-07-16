//! Side-table for Let IR nodes recording mutability information.
//!
//! Phase 6 m5-002: Each `IrKind::Let` node carries structural children
//! in the arena's `children_table`. This module provides a side-table
//! (`LetMetaTable`) mapping Let node ids to their mutability metadata.
//!
//! This design parallels `LoadStoreSideTable` and keeps `IrNodeData` at 48 bytes
//! while allowing tracking of whether a let binding is mutable.

use std::collections::HashMap;

use crate::enum_layout::EnumTypeId;
use crate::monomorphisation::TypeId;
use crate::node::IrNodeId;

/// Calling convention for function bindings (IR-level copy of AST enum).
///
/// Specifies the ABI (Application Binary Interface) calling convention
/// for function-shaped bindings. `None` on LetInfo means "paideia default" (unannotated).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CallingConvention {
    /// Microsoft x64 calling convention (used for UEFI ABI compatibility).
    Ms,
    /// System V AMD64 ABI calling convention (used for Unix/Linux targets).
    Sysv,
}

/// Metadata for a Let IR node.
///
/// Records whether the let binding is mutable (let mut x : T = ...) and,
/// optionally, the declared type of the binding.
///
/// Phase 6 m5-002: `mutable` distinguishes rodata (immutable), data
/// (mutable initialized), and bss (mutable uninitialized) sections.
///
/// Phase 7 m4-003 (PA7C-m4-003): `ty` carries the binding's declared
/// [`TypeId`] (when known) so the emit pass can width-thread integer-literal
/// bindings — e.g. `let x : u32 = 42` emits a 5-byte `B8 imm32` move instead
/// of the generic 10-byte 64-bit move. `ty` is `None` for untyped/legacy
/// bindings, in which case the generic 64-bit path is preserved.
///
/// Phase 14 PA14-r14-008: `ring` carries the ring buffer directive `@ring(slots=M, slot_size=K)`
/// for ring-buffer-annotated bindings.
///
/// Phase 19 PA19-r19-010: `link_section` carries the link_section directive `@link_section("name")`
/// for custom-section-annotated bindings.
///
/// Phase 19 PA19-r19-001: `abi` carries the calling convention directive `@abi("ms"|"sysv")`
/// for function-shaped bindings. `None` means paideia default (unannotated), not explicitly `Sysv`.
///
/// NOTE: Copy trait removed in v0.19 due to Option<String> field in link_section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LetInfo {
    /// true if this is `let mut x : T = ...`, false for `let x : T = ...`.
    pub mutable: bool,
    /// Declared type of the binding, if resolved. `None` for untyped bindings.
    pub ty: Option<TypeId>,
    /// Optional alignment directive `@align(N)` for per-symbol alignment (PA10-006y).
    /// When `Some(N)`, the symbol must be aligned to N bytes (power of 2, 1..2^30).
    pub align: Option<u32>,
    /// Optional ring buffer directive `@ring(slots=M, slot_size=K)` (PA14-r14-008).
    /// When `Some((M, K))`, allocates ring structures with M slots of K bytes each.
    pub ring: Option<(u32, u32)>,
    /// Optional link_section directive `@link_section("name")` (PA19-r19-010).
    /// When `Some(name)`, emits data into a custom-named ELF section.
    pub link_section: Option<String>,
    /// Optional ABI calling convention directive `@abi("ms"|"sysv")` (PA19-r19-001).
    /// When `Some(cc)`, specifies the calling convention for function-shaped bindings.
    /// `None` means paideia default (unannotated), not explicitly `Sysv`.
    pub abi: Option<CallingConvention>,
    /// Optional enum type ID if the binding is annotated with an enum type (#1222).
    /// When `Some(eid)`, the binding's declared type is an enum variant of that type.
    pub enum_type_id: Option<EnumTypeId>,
}

impl LetInfo {
    /// Construct a new LetInfo for an immutable binding (no declared type).
    #[must_use]
    pub fn immutable() -> Self {
        Self {
            mutable: false,
            ty: None,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            enum_type_id: None,
        }
    }

    /// Construct a new LetInfo for a mutable binding (no declared type).
    #[must_use]
    pub fn mutable() -> Self {
        Self {
            mutable: true,
            ty: None,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            enum_type_id: None,
        }
    }

    /// Construct a LetInfo with an explicit mutability and optional declared type.
    ///
    /// Phase 7 m4-003: the lowerer calls this when the binding's declared type
    /// is known, enabling width-threaded integer-literal emission.
    #[must_use]
    pub fn with_type(mutable: bool, ty: Option<TypeId>) -> Self {
        Self { mutable, ty, align: None, ring: None, link_section: None, abi: None, enum_type_id: None }
    }

    /// Construct a LetInfo with explicit mutability, type, and alignment.
    ///
    /// Phase 10 PA10-006y: the lowerer calls this when the binding's declared type
    /// and optional alignment directive are known.
    #[must_use]
    pub fn with_align(mutable: bool, ty: Option<TypeId>, align: Option<u32>) -> Self {
        Self { mutable, ty, align, ring: None, link_section: None, abi: None, enum_type_id: None }
    }

    /// Construct a LetInfo with explicit mutability, type, alignment, and ring.
    ///
    /// Phase 14 PA14-r14-008: the lowerer calls this when the binding's declared type,
    /// optional alignment directive, and optional ring buffer directive are known.
    #[must_use]
    pub fn with_ring(mutable: bool, ty: Option<TypeId>, align: Option<u32>, ring: Option<(u32, u32)>) -> Self {
        Self { mutable, ty, align, ring, link_section: None, abi: None, enum_type_id: None }
    }

    /// Construct a LetInfo with explicit mutability, type, alignment, ring, and link_section.
    ///
    /// Phase 19 PA19-r19-010: the lowerer calls this when the binding's declared type,
    /// optional alignment directive, optional ring buffer directive, and optional link_section
    /// directive are known.
    #[must_use]
    pub fn with_link_section(mutable: bool, ty: Option<TypeId>, align: Option<u32>, ring: Option<(u32, u32)>, link_section: Option<String>) -> Self {
        Self { mutable, ty, align, ring, link_section, abi: None, enum_type_id: None }
    }

    /// Construct a LetInfo with explicit mutability, type, alignment, ring, link_section, and abi.
    ///
    /// Phase 19 PA19-r19-001: the lowerer calls this when the binding's declared type,
    /// optional alignment directive, optional ring buffer directive, optional link_section
    /// directive, and optional calling convention directive are known.
    #[must_use]
    pub fn with_abi(mutable: bool, ty: Option<TypeId>, align: Option<u32>, ring: Option<(u32, u32)>, link_section: Option<String>, abi: Option<CallingConvention>) -> Self {
        Self { mutable, ty, align, ring, link_section, abi, enum_type_id: None }
    }
}

/// Side-table mapping Let IR node IDs → LetInfo.
///
/// Sparse mapping: let node id -> LetInfo.
#[derive(Default, Debug, Clone)]
pub struct LetMetaTable {
    entries: HashMap<IrNodeId, LetInfo>,
}

impl LetMetaTable {
    /// Construct an empty LetMetaTable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a let metadata entry.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, info: LetInfo) -> Option<LetInfo> {
        self.entries.insert(id, info)
    }

    /// Look up let metadata.
    ///
    /// Returns `None` if the node was never registered or is not mutable.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&LetInfo> {
        self.entries.get(&id)
    }

    /// Look up let metadata (mutable).
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut LetInfo> {
        self.entries.get_mut(&id)
    }

    /// Number of let metadata entries registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no let metadata entries are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove a let metadata entry.
    ///
    /// Returns the entry if one existed.
    pub fn remove(&mut self, id: IrNodeId) -> Option<LetInfo> {
        self.entries.remove(&id)
    }

    /// Iterate over all entries (id, info) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &LetInfo)> {
        self.entries.iter()
    }

    /// Borrow the underlying HashMap (read-only).
    #[must_use]
    pub fn entries(&self) -> &HashMap<IrNodeId, LetInfo> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn let_info_immutable_constructs() {
        let info = LetInfo::immutable();
        assert!(!info.mutable);
    }

    #[test]
    fn let_info_mutable_constructs() {
        let info = LetInfo::mutable();
        assert!(info.mutable);
    }

    #[test]
    fn let_info_immutable_has_no_type() {
        assert_eq!(LetInfo::immutable().ty, None);
    }

    #[test]
    fn let_info_immutable_has_no_align() {
        assert_eq!(LetInfo::immutable().align, None);
    }

    #[test]
    fn let_info_with_type_records_mutability_and_type() {
        let ty = TypeId(7);
        let info = LetInfo::with_type(true, Some(ty));
        assert!(info.mutable);
        assert_eq!(info.ty, Some(ty));
        assert_eq!(info.align, None);
        assert_eq!(info.ring, None);
        assert_eq!(info.link_section, None);
        assert_eq!(info.enum_type_id, None);

        let untyped = LetInfo::with_type(false, None);
        assert!(!untyped.mutable);
        assert_eq!(untyped.ty, None);
        assert_eq!(untyped.align, None);
        assert_eq!(untyped.ring, None);
        assert_eq!(untyped.link_section, None);
        assert_eq!(untyped.enum_type_id, None);
    }

    #[test]
    fn let_info_with_align_records_all_fields() {
        let ty = TypeId(7);
        let info = LetInfo::with_align(true, Some(ty), Some(4096));
        assert!(info.mutable);
        assert_eq!(info.ty, Some(ty));
        assert_eq!(info.align, Some(4096));
        assert_eq!(info.ring, None);
        assert_eq!(info.link_section, None);
        assert_eq!(info.enum_type_id, None);

        let unaligned = LetInfo::with_align(false, None, None);
        assert!(!unaligned.mutable);
        assert_eq!(unaligned.ty, None);
        assert_eq!(unaligned.align, None);
        assert_eq!(unaligned.ring, None);
        assert_eq!(unaligned.link_section, None);
        assert_eq!(unaligned.enum_type_id, None);
    }

    #[test]
    fn let_info_with_ring_records_all_fields() {
        let ty = TypeId(7);
        let ring_info = (256u32, 128u32);
        let info = LetInfo::with_ring(true, Some(ty), Some(64), Some(ring_info));
        assert!(info.mutable);
        assert_eq!(info.ty, Some(ty));
        assert_eq!(info.align, Some(64));
        assert_eq!(info.ring, Some(ring_info));
        assert_eq!(info.link_section, None);
        assert_eq!(info.enum_type_id, None);

        let no_ring = LetInfo::with_ring(false, None, None, None);
        assert!(!no_ring.mutable);
        assert_eq!(no_ring.ty, None);
        assert_eq!(no_ring.align, None);
        assert_eq!(no_ring.ring, None);
        assert_eq!(no_ring.link_section, None);
        assert_eq!(no_ring.enum_type_id, None);
    }

    #[test]
    fn let_info_with_link_section_records_all_fields() {
        let ty = TypeId(7);
        let ring_info = (256u32, 128u32);
        let link_sec = Some(".uefi_hdr".to_string());
        let info = LetInfo::with_link_section(true, Some(ty), Some(64), Some(ring_info), link_sec.clone());
        assert!(info.mutable);
        assert_eq!(info.ty, Some(ty));
        assert_eq!(info.align, Some(64));
        assert_eq!(info.ring, Some(ring_info));
        assert_eq!(info.link_section, link_sec);
        assert_eq!(info.abi, None);
        assert_eq!(info.enum_type_id, None);

        let no_link_section = LetInfo::with_link_section(false, None, None, None, None);
        assert!(!no_link_section.mutable);
        assert_eq!(no_link_section.ty, None);
        assert_eq!(no_link_section.align, None);
        assert_eq!(no_link_section.ring, None);
        assert_eq!(no_link_section.link_section, None);
        assert_eq!(no_link_section.abi, None);
        assert_eq!(no_link_section.enum_type_id, None);
    }

    #[test]
    fn let_info_with_abi_records_all_fields() {
        let ty = TypeId(7);
        let ring_info = (256u32, 128u32);
        let link_sec = Some(".uefi_hdr".to_string());
        let abi_cc = Some(CallingConvention::Ms);
        let info = LetInfo::with_abi(true, Some(ty), Some(64), Some(ring_info), link_sec.clone(), abi_cc.clone());
        assert!(info.mutable);
        assert_eq!(info.ty, Some(ty));
        assert_eq!(info.align, Some(64));
        assert_eq!(info.ring, Some(ring_info));
        assert_eq!(info.link_section, link_sec);
        assert_eq!(info.abi, abi_cc);
        assert_eq!(info.enum_type_id, None);

        let no_abi = LetInfo::with_abi(false, None, None, None, None, None);
        assert!(!no_abi.mutable);
        assert_eq!(no_abi.ty, None);
        assert_eq!(no_abi.align, None);
        assert_eq!(no_abi.ring, None);
        assert_eq!(no_abi.link_section, None);
        assert_eq!(no_abi.abi, None);
        assert_eq!(no_abi.enum_type_id, None);

        let sysv_abi = Some(CallingConvention::Sysv);
        let sysv_info = LetInfo::with_abi(true, Some(ty), None, None, None, sysv_abi.clone());
        assert_eq!(sysv_info.abi, sysv_abi);
        assert_eq!(sysv_info.enum_type_id, None);
    }

    #[test]
    fn let_meta_table_insert_and_get() {
        let mut table = LetMetaTable::new();
        let let_id = IrNodeId::new(1).unwrap();
        let info = LetInfo::mutable();

        table.insert(let_id, info);
        let retrieved = table.get(let_id).unwrap();
        assert!(retrieved.mutable);
    }

    #[test]
    fn let_meta_table_get_returns_none_for_unknown() {
        let table = LetMetaTable::new();
        let unknown_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unknown_id), None);
    }

    #[test]
    fn let_meta_table_len_and_is_empty() {
        let mut table = LetMetaTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let id1 = IrNodeId::new(1).unwrap();
        table.insert(id1, LetInfo::mutable());
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn let_meta_table_remove() {
        let mut table = LetMetaTable::new();
        let let_id = IrNodeId::new(1).unwrap();
        let info = LetInfo::mutable();

        table.insert(let_id, info);
        assert_eq!(table.len(), 1);

        let removed = table.remove(let_id).unwrap();
        assert!(removed.mutable);
        assert_eq!(table.len(), 0);
    }
}
