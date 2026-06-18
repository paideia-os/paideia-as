//! Concrete type structure and identifiers.
//!
//! `Type` is the core enum representing monomorphic types. `TypeId` is a
//! stable, niche-optimized handle for interned types. `CapSetId` represents
//! an interned capability set (phase-1: simple u32 index).

use core::num::NonZeroU32;
use paideia_as_ir::EffectRowId;

/// Stable identifier for an interned [`Type`]. Niche-optimized so
/// `Option<TypeId>` fits in 4 bytes.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct TypeId(NonZeroU32);

impl TypeId {
    /// Construct a `TypeId` from a positive integer.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Index into a zero-based Vec (the interner's storage).
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

impl core::fmt::Display for TypeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "t{}", self.0.get())
    }
}

/// Identifier for an interned capability set. Phase-1 uses a u32
/// index into the interner's cap-set table; 0 is the empty cap set.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct CapSetId(pub u32);

impl CapSetId {
    /// The empty capability set.
    pub const EMPTY: Self = CapSetId(0);
}

/// Concrete type structure. Hashed/cons'd by the interner.
///
/// This is the monomorphic type representation. The elaborator uses the
/// interner to assign `TypeId` to IR nodes. Equality and hashing are
/// structural, enabling hash-consing (memoization of equal types).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum Type {
    /// `()` — the unit type.
    Unit,
    /// `bool` — boolean.
    Bool,
    /// Unicode scalar value type.
    Char,
    /// Unsigned integer of `bits` width.
    ///
    /// Standard widths: 8, 16, 32, 64, 128. [`SIZE_WIDTH_SENTINEL`] (0xFFFF)
    /// represents `usize`.
    UInt(u16),
    /// Signed integer of `bits` width.
    ///
    /// Standard widths: 8, 16, 32, 64, 128. [`SIZE_WIDTH_SENTINEL`] (0xFFFF)
    /// represents `isize`.
    SInt(u16),
    /// IEEE-754 floating-point (32 or 64 bits).
    Float(u16),
    /// Universal type — top of the lattice. Inhabited by all values.
    Top,
    /// Bottom (empty) type — used for diverging functions.
    Bot,
    /// Function arrow `(params) -> ret !{effects} @{caps}`.
    ///
    /// `params` is the parameter types, `ret` is the return type,
    /// `effects` is the effect row this function may perform, and
    /// `caps` is the capability set required.
    Fn {
        /// Parameter types.
        params: Vec<TypeId>,
        /// Return type.
        ret: TypeId,
        /// Effect row this function performs.
        effects: EffectRowId,
        /// Capability set required by this function.
        caps: CapSetId,
    },
    /// Tuple type.
    Tuple(Vec<TypeId>),
    /// Named (nominal) type, identified by an interned name index.
    ///
    /// Phase-1 just stores a small u32 — naming/resolution arrives
    /// later. Used as a placeholder for the `Cap<T>` family the AC
    /// alludes to. Name index `1` is reserved for the capability placeholder.
    Named {
        /// Name index (interned name; `1` is reserved for Cap<T> placeholder).
        name: u32,
        /// Type arguments.
        args: Vec<TypeId>,
    },
}

/// Sentinel value for "size word" (`usize`/`isize`): stored as the
/// width field with this magic value.
pub const SIZE_WIDTH_SENTINEL: u16 = 0xFFFF;

impl Type {
    /// Build a `Type::UInt` with the right width: 8, 16, 32, 64, 128,
    /// or [`SIZE_WIDTH_SENTINEL`] for `usize`.
    pub fn uint(bits: u16) -> Self {
        Self::UInt(bits)
    }

    /// Build a `Type::SInt` with the right width: 8, 16, 32, 64, 128,
    /// or [`SIZE_WIDTH_SENTINEL`] for `isize`.
    pub fn sint(bits: u16) -> Self {
        Self::SInt(bits)
    }

    /// Build a `Type::Float` with width 32 or 64.
    pub fn float(bits: u16) -> Self {
        Self::Float(bits)
    }
}
