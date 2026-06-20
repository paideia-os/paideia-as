//! Concrete type structure and identifiers.
//!
//! `Type` is the core enum representing monomorphic types. `TypeId` is a
//! stable, niche-optimized handle for interned types. `CapSetId` represents
//! an interned capability set (phase-1: simple u32 index).

use core::num::NonZeroU32;
use paideia_as_ir::EffectRowId;
use smallvec::SmallVec;

/// Type variable for unification. Used during HM type inference.
/// `Option<TyVar>` is 4 bytes via niche optimization.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct TyVar(NonZeroU32);

impl TyVar {
    /// Construct a `TyVar` from a positive integer.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this variable.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl core::fmt::Display for TyVar {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "α{}", self.0.get())
    }
}

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
    /// Type variable awaiting unification.
    Var(TyVar),
    /// Reflective syntactic term (introduced by `quote`, eliminated by `~`).
    ///
    /// The typer doesn't yet refine `Term` by the shape of what's quoted;
    /// m2-003+ may add `Term<expr>` / `Term<type>` variants. Phase-1 treats
    /// all `quote` and `~` forms as producing/expecting `Term`.
    Term,
    /// Raw pointer type `*T` or `*mut T`.
    ///
    /// `mutable = false` represents `*T` (immutable raw pointer).
    /// `mutable = true` represents `*mut T` (mutable raw pointer; reserved for future parser extension).
    /// Phase3-m1-002 ships only `*T` (mutable = false) from the parser.
    Ptr {
        /// The type being pointed to.
        pointee: TypeId,
        /// Whether this is a mutable pointer (`*mut T`) or immutable (`*T`).
        mutable: bool,
    },
    /// Record (struct) type with named fields.
    ///
    /// Fields are stored in order of declaration. Two records with the same
    /// field names and types (in the same order) intern to the same TypeId
    /// via hash-consing.
    Record {
        /// Field names (interned symbols) and their types, in declaration order.
        /// Using SmallVec with inline capacity 4 for typical cases.
        fields: SmallVec<[(u32, TypeId); 4]>,
    },
    /// Enum (tagged union) type with named variants.
    ///
    /// Variants are stored in order of declaration. Each variant has a name
    /// (interned symbol) and a payload (Unit, Tuple, or Record).
    /// Two enums with the same variants (in the same order) intern to the same TypeId.
    Enum {
        /// Variant names (interned symbols) and their payloads, in declaration order.
        /// Using SmallVec with inline capacity 4 for typical cases.
        variants: SmallVec<[(u32, EnumPayload); 4]>,
    },
}

/// Payload shape for an enum variant.
///
/// Each enum variant carries either no data, a tuple of types, or a record
/// of named fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EnumPayload {
    /// Unit variant (no associated data).
    Unit,
    /// Tuple variant: `Variant(T1, T2, ...)`.
    Tuple(SmallVec<[TypeId; 4]>),
    /// Record variant: `Variant { field1: T1, field2: T2, ... }`.
    Record(SmallVec<[(u32, TypeId); 4]>),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;

    #[test]
    fn ptr_interns_same_pointee_to_same_id() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let ptr1 = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });
        let ptr2 = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });

        assert_eq!(ptr1, ptr2, "*u64 and *u64 should intern to the same TypeId");
    }

    #[test]
    fn ptr_with_different_pointee_interns_distinct() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let u8_id = interner.uint(8);

        let ptr_u64 = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });
        let ptr_u8 = interner.intern(Type::Ptr {
            pointee: u8_id,
            mutable: false,
        });

        assert_ne!(
            ptr_u64, ptr_u8,
            "*u64 and *u8 should intern to different TypeIds"
        );
    }

    #[test]
    fn ptr_mutable_and_immutable_distinct() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let ptr_immutable = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });
        let ptr_mutable = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: true,
        });

        assert_ne!(
            ptr_immutable, ptr_mutable,
            "*u64 and *mut u64 should intern to different TypeIds"
        );
    }

    #[test]
    fn record_interns_same_fields_to_same_id() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let record1 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });
        let record2 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });

        assert_eq!(
            record1, record2,
            "Records with same fields should intern to same TypeId"
        );
    }

    #[test]
    fn record_interns_different_field_order_distinct() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let record1 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });
        let record2 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(2, bool_id), (1, u64_id)],
        });

        assert_ne!(
            record1, record2,
            "Records with different field order should have different TypeIds"
        );
    }

    #[test]
    fn enum_interns_same_variants_to_same_id() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let enum1 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
            ],
        });
        let enum2 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
            ],
        });

        assert_eq!(
            enum1, enum2,
            "Enums with same variants should intern to same TypeId"
        );
    }

    #[test]
    fn enum_interns_different_variant_order_distinct() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let enum1 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
            ],
        });
        let enum2 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
                (1, EnumPayload::Unit),
            ],
        });

        assert_ne!(
            enum1, enum2,
            "Enums with different variant order should have different TypeIds"
        );
    }
}
