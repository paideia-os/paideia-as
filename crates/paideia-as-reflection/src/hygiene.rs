//! Hygienic identifier substrate for the R220.M1 reflection surface.
//!
//! `HygienicId` is a lightweight newtype the `Syntax` constructors attach
//! to every identifier they introduce. It is deliberately kept **thin**
//! at R220.M1: a globally-fresh `NonZeroU32` tag plus an `unmarked`
//! sentinel for identifiers copied through from the use-site AST.
//!
//! **What R220.M2 will do here.** The Ullrich 2020 alpha-rename pass
//! (already implemented for phase-1 pattern macros in
//! `paideia_as_elaborator::hygiene::HygienicName`) will be threaded
//! through the reflective `Syntax` API, mapping every `HygienicId` to a
//! `HygienicName { spelling, tags }` at elaboration time. Keeping the
//! seat as a bare `NonZeroU32` here lets R220.M2 evolve the resolution
//! machinery without breaking the R220.M1 constructor signatures.
//!
//! **Why not reuse `paideia_as_elaborator::hygiene::MacroId` today?**
//! The elaborator crate depends on `paideia-as-effects` (and much more);
//! this reflection crate must be low in the dependency graph so R220.M3's
//! `@dsl_parser` (a parser-side hook) can consume it without a cyclic
//! dependency. R220.M2 will land a shared identifier type (either lifted
//! from the elaborator into this crate or a new one both consume).

use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU32, Ordering};

/// Sentinel `HygienicId` for identifiers copied verbatim from the caller
/// AST (i.e., not introduced by the reflected DSL).
///
/// R220.M2 will read this as "unmarked at the DSL level; retain whatever
/// hygiene tags the use-site AST already carried."
pub const HYGIENIC_ID_UNTAGGED: HygienicId = HygienicId(NonZeroU32::MIN);

/// A hygienic identifier tag attached to a `Syntax`-introduced name.
///
/// Two `HygienicId`s compare equal iff their underlying tags are equal.
/// [`HYGIENIC_ID_UNTAGGED`] is the sentinel for "no dedicated tag — treat
/// as pass-through from the use site." Every `fresh_hygienic_id()` call
/// returns a monotonically-increasing tag, distinct from every other tag
/// this process has minted (including the untagged sentinel).
///
/// R220.M2 will grow this into a full `HygienicName { spelling, tags }`
/// (per `paideia_as_elaborator::hygiene::HygienicName`) so name
/// resolution can distinguish `temp` introduced by macro `M` from a
/// use-site `temp` of the same spelling. Today the mapping is
/// intentionally minimal — enough to prove the plumbing is in place.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct HygienicId(NonZeroU32);

impl HygienicId {
    /// The raw integer value of this tag. Never zero.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Construct a `HygienicId` from a positive integer. Returns `None`
    /// if the integer is zero. Prefer [`fresh_hygienic_id`] for new IDs;
    /// use this only when round-tripping through a persisted form.
    #[must_use]
    pub fn from_raw(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// True if this ID is the [`HYGIENIC_ID_UNTAGGED`] sentinel — the
    /// "pass this identifier through unchanged" marker.
    #[must_use]
    pub fn is_untagged(self) -> bool {
        self == HYGIENIC_ID_UNTAGGED
    }
}

impl core::fmt::Display for HygienicId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_untagged() {
            write!(f, "h#untagged")
        } else {
            write!(f, "h#{}", self.0.get())
        }
    }
}

/// Allocate a fresh, globally-unique `HygienicId`.
///
/// Each returned tag is monotonically increasing across the process;
/// the counter never wraps in practice (≥ 2^31 DSL-introduced
/// identifiers is not a realistic budget for one compilation).
/// Skips over the reserved [`HYGIENIC_ID_UNTAGGED`] value.
#[must_use]
pub fn fresh_hygienic_id() -> HygienicId {
    static NEXT: AtomicU32 = AtomicU32::new(2);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    HygienicId(
        NonZeroU32::new(n).expect("fresh_hygienic_id counter starts at 2 and only grows"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_hygienic_ids_are_distinct() {
        let a = fresh_hygienic_id();
        let b = fresh_hygienic_id();
        assert_ne!(a, b);
        assert!(a.get() >= 2);
        assert!(b.get() >= 2);
    }

    #[test]
    fn fresh_hygienic_id_never_returns_untagged_sentinel() {
        // The allocator starts at 2 to avoid colliding with
        // HYGIENIC_ID_UNTAGGED (raw value 1). We check the invariant on
        // a batch of fresh ids to guard against a future counter change.
        for _ in 0..1024 {
            let id = fresh_hygienic_id();
            assert!(!id.is_untagged(), "fresh id must not be the untagged sentinel");
            assert!(id.get() >= 2);
        }
    }

    #[test]
    fn untagged_sentinel_is_untagged() {
        assert!(HYGIENIC_ID_UNTAGGED.is_untagged());
        assert_eq!(HYGIENIC_ID_UNTAGGED.get(), 1);
    }

    #[test]
    fn from_raw_round_trip() {
        let id = fresh_hygienic_id();
        let raw = id.get();
        let round = HygienicId::from_raw(raw).unwrap();
        assert_eq!(round, id);
    }

    #[test]
    fn from_raw_rejects_zero() {
        assert!(HygienicId::from_raw(0).is_none());
    }

    #[test]
    fn display_formats() {
        assert_eq!(format!("{}", HYGIENIC_ID_UNTAGGED), "h#untagged");
        let id = HygienicId::from_raw(42).unwrap();
        assert_eq!(format!("{}", id), "h#42");
    }
}
