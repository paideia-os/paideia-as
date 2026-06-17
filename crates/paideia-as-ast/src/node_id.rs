//! `NodeId` — stable identifier for an AST node interned in an [`AstArena`].
//!
//! [`AstArena`]: crate::AstArena

use core::num::NonZeroU32;
use static_assertions::const_assert_eq;
use std::mem::size_of;

/// Stable identifier for an AST node, valid for the lifetime of the
/// arena that minted it.
///
/// `NodeId` is a newtype around [`NonZeroU32`] so that `Option<NodeId>`
/// fits in 4 bytes via niche optimization. IDs start at 1; the arena
/// hands them out in allocation order.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// Construct a `NodeId` from a positive integer.
    ///
    /// Returns `None` if `n == 0`.
    #[must_use]
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Index into a zero-based `Vec` (the arena's storage).
    #[must_use]
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

impl core::fmt::Display for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "n{}", self.0.get())
    }
}

// `Option<NodeId>` must fit in 4 bytes (niche optimization).
const_assert_eq!(size_of::<Option<NodeId>>(), 4);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_rejected() {
        assert!(NodeId::new(0).is_none());
    }

    #[test]
    fn nonzero_is_accepted() {
        let id = NodeId::new(7).unwrap();
        assert_eq!(id.get(), 7);
    }

    #[test]
    fn index_is_zero_based() {
        assert_eq!(NodeId::new(1).unwrap().index(), 0);
        assert_eq!(NodeId::new(42).unwrap().index(), 41);
    }

    #[test]
    fn display_format() {
        let id = NodeId::new(3).unwrap();
        assert_eq!(format!("{id}"), "n3");
    }

    #[test]
    fn ordering_matches_integer() {
        let a = NodeId::new(1).unwrap();
        let b = NodeId::new(2).unwrap();
        assert!(a < b);
    }
}
