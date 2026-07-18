//! Stable IR node identifier.

use core::num::NonZeroU32;
use static_assertions::const_assert;

/// Stable identifier for an IR node interned in an arena.
///
/// `IrNodeId` is a typed index newtype that ensures arena indices cannot be
/// confused with other numeric ids. It uses `NonZeroU32` to avoid the null-pointer
/// optimization size bloat (keeping `Option<IrNodeId>` at 4 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct IrNodeId(NonZeroU32);

impl IrNodeId {
    /// Construct an `IrNodeId` from a positive integer.
    ///
    /// Returns `None` if `n` is 0 (since `NonZeroU32::new` rejects 0).
    #[must_use]
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Index into a zero-based array (the arena's storage).
    ///
    /// Subtracts 1 from the id so that `IrNodeId::new(1)` maps to index 0.
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

// AC: NonZeroU32 size guarantee — Option<IrNodeId> must remain 4 bytes.
const_assert!(core::mem::size_of::<Option<IrNodeId>>() == 4);
