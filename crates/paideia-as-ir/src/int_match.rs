//! Side-table for integer-scrutinee match metadata (#1210).
//!
//! Provides storage for scrutinee width and signedness information for match
//! expressions over integer types. Keyed on Match IrNodeId, enabling the emitter
//! to route integer matches around the enum-only dispatch pipeline.

use crate::instruction::IntWidth;
use crate::node::IrNodeId;

/// Metadata for an integer-scrutinee match (#1210).
///
/// Recorded for match expressions where the scrutinee is an integer literal,
/// variable, or other integer-typed expression. Enables code generation to
/// emit cmp/jne cascade instead of enum-discriminant dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntMatchScrutinee {
    /// Width of the integer (W8/W16/W32/W64).
    pub width: IntWidth,
    /// `true` for signed integers (i8/i16/i32/i64), `false` for unsigned.
    pub signed: bool,
}

crate::impl_named_side_table!(
    /// Side-table: Match node IrNodeId -> IntMatchScrutinee for integer-scrutinee matches.
    ///
    /// Populated by populate_int_match_meta post-populate_match_arm_meta.
    /// Consumed by visit_int_match to route around the enum-only pipeline.
    pub struct IntMatchScrutineeTable, IrNodeId => IntMatchScrutinee
);
