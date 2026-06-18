//! Typed-core IR node data per `custom-assembler.md` §6.1.
//!
//! Every IR variant carries the substructural lattice class
//! (`LinClass`) and an effect-row reference. Phase-1 defaults both to
//! `Unrestricted` / `empty_row`; the elaborator (PR-29+) populates them
//! as types are inferred.

use core::num::NonZeroU32;
use paideia_as_diagnostics::Span;
use static_assertions::const_assert;
use std::mem::size_of;

/// Substructural lattice class per Walker (2005).
///
/// The lattice ordering is `Ordered < Linear < Affine < Unrestricted`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
#[non_exhaustive]
pub enum LinClass {
    /// Ordered (linearity + ordering).
    Ordered,
    /// Linear (must consume exactly once).
    Linear,
    /// Affine (must consume at most once).
    Affine,
    /// Unrestricted (default).
    #[default]
    Unrestricted,
}

/// Stable identifier for an IR node interned in an [`crate::IrArena`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct IrNodeId(NonZeroU32);

impl IrNodeId {
    /// Construct an `IrNodeId` from a positive integer.
    #[must_use]
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Index into a zero-based Vec (the arena's storage).
    #[must_use]
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

impl core::fmt::Display for IrNodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "i{}", self.0.get())
    }
}

/// Effect-row reference: an interned id from the arena's effect-row table.
///
/// `EffectRowId(0)` is reserved for the empty row (no effects).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct EffectRowId(pub u32);

impl EffectRowId {
    /// The sentinel for an empty effect row.
    pub const EMPTY: Self = EffectRowId(0);
}

/// Variant discriminant for an IR node.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
#[non_exhaustive]
pub enum IrKind {
    /// Module (struct or functor result).
    Module,
    /// Functor (parameterised module).
    Functor,
    /// Let-binding.
    Let,
    /// Lambda abstraction.
    Lambda,
    /// Function application.
    App,
    /// Variable reference.
    Var,
    /// Literal (int / string / etc. — payload in side-table; out of scope here).
    Literal,
    /// Effect operation (handler perform).
    Perform,
    /// With-handler installation.
    Handle,
    /// Action block (effectful sequence).
    Action,
    /// Unsafe-block escape hatch.
    Unsafe,
    /// Placeholder kind, used until elaborator (PR-29+) fills the real variant.
    Placeholder,
}

/// Per-node IR storage.
///
/// Carries the variant discriminant, source span, substructural class,
/// and effect-row reference. Per the AC, every variant has a `LinClass`
/// and an `EffectRowId` slot — the elaborator may leave them at their
/// `Default` until checking runs.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct IrNodeData {
    /// Variant discriminant.
    pub kind: IrKind,
    /// Substructural class.
    pub lin_class: LinClass,
    /// Effect-row interned id.
    pub effect_row: EffectRowId,
    /// Source span this node was derived from.
    pub span: Span,
}

// AC: size_of::<IrNodeData>() ≤ 48 bytes. Current shape is well under.
const_assert!(size_of::<IrNodeData>() <= 48);

impl IrNodeData {
    /// Construct an `IrNodeData` with defaults for class + effect row.
    #[must_use]
    pub fn new(kind: IrKind, span: Span) -> Self {
        Self {
            kind,
            lin_class: LinClass::Unrestricted,
            effect_row: EffectRowId::EMPTY,
            span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    #[test]
    fn lin_class_default_is_unrestricted() {
        assert_eq!(LinClass::default(), LinClass::Unrestricted);
    }

    #[test]
    fn effect_row_default_is_empty() {
        assert_eq!(EffectRowId::default(), EffectRowId::EMPTY);
        assert_eq!(EffectRowId::EMPTY.0, 0);
    }

    #[test]
    fn ir_node_id_round_trips() {
        let id = IrNodeId::new(42).unwrap();
        assert_eq!(id.get(), 42);
        assert_eq!(id.index(), 41);
        assert_eq!(format!("{id}"), "i42");
    }

    #[test]
    fn ir_node_id_rejects_zero() {
        assert!(IrNodeId::new(0).is_none());
    }

    #[test]
    fn option_ir_node_id_is_4_bytes() {
        assert_eq!(size_of::<Option<IrNodeId>>(), 4);
    }

    #[test]
    fn ir_node_data_size_within_budget() {
        assert!(size_of::<IrNodeData>() <= 48);
    }

    #[test]
    fn new_node_data_defaults() {
        let span = Span::new(FileId::new(1).unwrap(), 0, 1);
        let d = IrNodeData::new(IrKind::Placeholder, span);
        assert_eq!(d.lin_class, LinClass::Unrestricted);
        assert_eq!(d.effect_row, EffectRowId::EMPTY);
        assert_eq!(d.kind, IrKind::Placeholder);
    }
}
