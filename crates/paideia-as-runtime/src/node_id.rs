//! Stable IR node identifier.

use core::num::NonZeroU32;

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
