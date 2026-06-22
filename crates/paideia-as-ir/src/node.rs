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
///
/// ## Child Semantics
///
/// Each variant produces children in the following order:
///
/// - **Module**: immediate item children (any number). Children appear
///   in source order.
/// - **Functor**: parameter descriptors + body. Structure TBD (PR-156+).
/// - **Let**: exactly one child — the value expression.
/// - **Lambda**: exactly one child — the body expression. Parameters
///   are stored in a separate side-table (not in children).
/// - **App**: callee + arguments. Children are [callee, arg0, arg1, ...].
/// - **Perform**: operation path + operand arguments. Children are
///   [op_callee, arg0, arg1, ...] per PR-155 semantics.
/// - **Handle**: handler + body. Children are [handler, body].
/// - **Action**: statement sequence (any number). Children appear
///   in source order.
/// - **Unsafe**: statement sequence (any number). Children appear
///   in source order.
/// - **Load**: memory load. Children are [pointer, index].
///   Side-table entry in LoadStoreSideTable records width / signedness / alignment.
/// - **Store**: memory store. Children are [pointer, index, value].
///   Side-table entry in LoadStoreSideTable records width / signedness / alignment.
/// - **Var** / **Literal** / **StringLiteral** / **Placeholder**: no children.
/// - **Resume**: reserved for future extension; no children yet.
/// - **RecordCons**: allocate + populate a record. Children: [field_value_0, field_value_1, ...].
///   Side-table: RecordLayoutTable maps this node to its TypeId (for layout).
/// - **FieldAccess**: access a record field. Children: [record_value].
///   Side-table: FieldAccessSideTable maps this node to (TypeId, field_index).
/// - **EnumCons**: construct an enum variant. Children: [payload_value_0, ...] (empty for Unit).
///   Side-table: EnumConsSideTable maps this node to (TypeId, variant_index).
/// - **EnumDiscriminant**: extract the discriminant of an enum value. Children: [enum_value].
///   Side-table: EnumDiscriminantSideTable maps this node to TypeId for the enum.
/// - **Loop**: loop block. Children: [body]. Side-table: LoopMetaTable records
///   (entry_label, exit_label) for the encoder.
/// - **Break**: break out of the enclosing loop. No children.
/// - **Continue**: continue to the next iteration of the enclosing loop. No children.
/// - **Match**: match expression with pattern arms. Children: [scrutinee, arm0, arm1, ...].
///   Each arm is its own subtree containing pattern and body expressions.
/// - **Branch**: if-then-else conditional. Children = [condition, then_body, else_body (optional)].
///   The then-body and else-body have separate scopes for linearity/effect-row tracking.
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
    /// String literal — immutable UTF-8 byte slice in .rodata.
    /// Side-table entry in StringLiteralTable records (rodata_offset: u64, len: u64).
    StringLiteral,
    /// Effect operation (handler perform).
    Perform,
    /// With-handler installation.
    Handle,
    /// Action block (effectful sequence).
    Action,
    /// Unsafe-block escape hatch.
    Unsafe,
    /// Load: children = [pointer, index].
    /// Side-table entry in LoadStoreSideTable records width / signedness / alignment.
    Load,
    /// Store: children = [pointer, index, value].
    /// Side-table entry in LoadStoreSideTable records width / signedness / alignment.
    Store,
    /// Placeholder kind, used until elaborator (PR-29+) fills the real variant.
    Placeholder,
    /// Allocate + populate a record. Children: [field_value_0, field_value_1, ...].
    /// Side-table entry in RecordLayoutTable records the TypeId.
    RecordCons,
    /// Access a record field. Children: [record_value].
    /// Side-table entry in FieldAccessSideTable records (TypeId, field_index).
    FieldAccess,
    /// Construct an enum variant. Children: [payload_value_0, ...] (empty for Unit).
    /// Side-table entry in EnumConsSideTable records (TypeId, variant_index).
    EnumCons,
    /// Extract the discriminant of an enum value. Children: [enum_value].
    /// Side-table entry in EnumDiscriminantSideTable records the TypeId.
    EnumDiscriminant,
    /// Loop block. Children: [body].
    /// Side-table: LoopMetaTable records (entry_label, exit_label) for the encoder.
    Loop,
    /// Break out of the enclosing loop. No children.
    Break,
    /// Continue to the next iteration of the enclosing loop. No children.
    Continue,
    /// While-loop with condition and body. Children = [condition, body].
    /// Phase 7 m1-001: emits top_label, test instruction, conditional jump to exit_label,
    /// body, unconditional jump back to top_label, then exit_label.
    While,
    /// Match expression: match on a value with multiple arms.
    /// Children = [scrutinee, arm0, arm1, ...].
    /// Each arm is its own subtree with pattern + body.
    Match,
    /// Branch (if-then-else): conditional expression with then and else arms.
    /// Children = [condition, then_body, else_body (optional)].
    /// Each arm is visited with per-arm scope hooks to track linearity and effects
    /// independently within each branch.
    Branch,
    /// Borrow: create an immutable reference. Children = [source].
    /// Side-table entry in BorrowSideTable records (source_binding, lifetime_id, mutable=false).
    Borrow,
    /// BorrowMut: create a mutable reference. Children = [source].
    /// Variant of Borrow with mutable=true (kept distinct for opt-pass dispatch).
    /// Side-table entry in BorrowSideTable records (source_binding, lifetime_id, mutable=true).
    BorrowMut,
    /// Deref: dereference a reference. Children = [reference].
    /// No side-table needed today.
    Deref,
    /// RawInstruction: x86_64 assembly instruction with mnemonic + operands.
    /// The instruction's mnemonic and operand AST shape are persisted
    /// through lowering; the AST back-pointer is available via the
    /// `ast_to_ir` map in `LoweringResult`.
    RawInstruction,
    /// Label declaration: a target for Jcc/Jmp instructions within an unsafe block.
    /// Phase 6 m4-002: Labels are collected by UnsafeWalker and stored in
    /// EmitPassState.labels (HashMap<name, byte_offset>). Duplicate labels → U1609;
    /// unknown references → U1610.
    Label,
    /// Bitwise NOT (one's complement). Children = [operand].
    /// Phase 7 m4-001: lowered from prefix `~` in expression position
    /// (when `in_quote_depth == 0`). The emit pass evaluates the operand
    /// into a register and emits `not r64` (REX.W F7 /2).
    BitNot,
    /// Arithmetic negation (two's complement). Children = [operand].
    /// Phase 7 m4-001: reserved companion of `BitNot` for prefix `-`.
    /// The emit pass evaluates the operand into a register and emits
    /// `neg r64` (REX.W F7 /3).
    Neg,
    /// Type cast (`expr as type`). Children = [operand].
    /// Phase 7 m4-002: lowered from `ExprData::Cast`. The target type is
    /// recorded in the `CastSideTable` (IrNodeId → TypeId). The emit pass
    /// evaluates the operand into a register and dispatches on the source and
    /// destination widths: widening signed → `movsx`; widening unsigned →
    /// `movzx`; narrowing → `mov r32, r32` (implicit zero-extend); same-width →
    /// no-op.
    Cast,
}

/// Per-node IR storage.
///
/// Carries the variant discriminant, source span, substructural class,
/// and effect-row reference. Per the AC, every variant has a `LinClass`
/// and an `EffectRowId` slot — the elaborator may leave them at their
/// `Default` until checking runs.
///
/// **Children storage**: children are stored in a separate side-table
/// (`children_table` in `IrArena`), indexed by `IrNodeId.index()`.
/// This preserves the 48-byte budget while allowing unbounded children
/// via SmallVec<[IrNodeId; 4]> (inline for ≤4, heap-spilled for more).
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

    #[test]
    fn size_budget_preserved_under_48_bytes() {
        // Phase-1 AC: IrNodeData must stay ≤ 48 bytes.
        // Currently 20 bytes (u8+u8+u32 + 12-byte Span).
        // Children are in a side-table (arena), not inline.
        assert!(size_of::<IrNodeData>() <= 48);
    }
}
