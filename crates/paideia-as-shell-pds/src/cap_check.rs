//! R227.M2 capability-declaration checker.
//!
//! A `.pds` script's header declares which capabilities the script
//! intends to exercise (via `#capability "cap.name"` pragmas, parsed
//! by [`crate::header::parse_header`]). Before the runtime hands
//! that script to the shell body loader, it must confirm the
//! *invoker* (the ambient capability set of whichever principal is
//! about to run the script) holds every capability the header
//! declares. This module owns that subset check.
//!
//! The checker is deliberately dependency-free: it operates on a
//! `HashSet<String>` of invoker-held names and a `&[String]` of
//! declared names. No wildcards, no hierarchical parent-implies-
//! child ("fs" implies "fs.read.home") — subset is *exact-name* at
//! M2. Hierarchical / prefix implication is deferred; the arbiter
//! that owns the ambient set (R227.M4+) will canonicalise names
//! before handing them here.
//!
//! # Position in the pipeline
//!
//! ```text
//!   .pds source
//!       │
//!       ▼
//!   parse_header  ── PdsHeader { capabilities: Vec<String>, .. }
//!       │
//!       ▼
//!   PdsHeader::check_against(&invoker: CapabilitySet)  ← this module
//!       │
//!       ├── Ok(())                       → shell-lex the body
//!       └── Err(MissingCapabilities{..}) → refuse to load; report
//! ```
//!
//! # Case sensitivity
//!
//! Capability names are compared byte-exact. `"fs.read"` and
//! `"FS.READ"` are distinct capabilities. Canonicalisation (case
//! folding, dot normalisation, NFC) is the arbiter's job, not the
//! checker's — this module refuses to guess whether two spellings
//! were meant to name the same right.
//!
//! # Fingerprints
//!
//! The R227.M2 test corpus tags each fixture with `r227m2-cap-NN`
//! so the R220.M10 `@fingerprint` correlator can attribute pass/fail
//! to a specific fixture without re-parsing its name.

use std::collections::HashSet;
use std::fmt;

/// The set of capabilities an invoker currently holds.
///
/// Newtype over `HashSet<String>` so that the check API can't be
/// called with an arbitrary `Vec<String>` and silently pay `O(n·m)`
/// membership tests: the invoker side is always hashed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(HashSet<String>);

impl CapabilitySet {
    /// Construct an empty capability set (an invoker holding no
    /// rights). Only a script whose header declares no capabilities
    /// passes [`check_subset`] against this set.
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    /// Build a capability set from any iterator of owned capability
    /// names. Duplicates in the input collapse to one entry (a set,
    /// not a multiset).
    pub fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }

    /// Insert one capability name into the set.
    ///
    /// Returns `true` if the name was newly added, `false` if it was
    /// already present (matching the [`HashSet::insert`] convention).
    pub fn insert(&mut self, cap: String) -> bool {
        self.0.insert(cap)
    }

    /// Byte-exact membership test.
    ///
    /// Case-sensitive, no wildcard expansion, no hierarchical
    /// parent-implies-child. See the module doc for why.
    pub fn contains(&self, cap: &str) -> bool {
        self.0.contains(cap)
    }

    /// Number of distinct capability names in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True iff [`Self::len`] is zero.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Discriminated failure modes for [`check_subset`].
///
/// A single variant today (`MissingCapabilities`); the enum shape
/// exists so R227.M3+ can add version-scoped or arbiter-side errors
/// (`RevokedCapability`, `ExpiredGrant`, …) without a breaking
/// signature change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapCheckError {
    /// One or more declared capabilities are absent from the
    /// invoker's set. `missing` lists them *in the order they
    /// appeared in the script header* — order is preserved so a
    /// diagnostic can point to the specific `#capability` line that
    /// tripped, not a hash-order permutation.
    MissingCapabilities {
        /// The names the script declared but the invoker does not hold.
        missing: Vec<String>,
    },
}

impl fmt::Display for CapCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCapabilities { missing } => {
                write!(
                    f,
                    "invoker missing {} capability declaration(s): {}",
                    missing.len(),
                    missing.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for CapCheckError {}

/// Check that every declared capability is held by the invoker.
///
/// Walks `declared` once, collecting any name not present in
/// `invoker`. If the collected list is empty the check passes;
/// otherwise a [`CapCheckError::MissingCapabilities`] carries the
/// full list of misses in header-declaration order.
///
/// The scan does *not* short-circuit on the first miss: a script
/// with three unmet capabilities should produce a diagnostic that
/// names all three, not one at a time across three runs. Duplicates
/// in `declared` (which the M1 parser does not prevent) are echoed
/// as duplicates in `missing` — the arbiter, not the checker,
/// decides whether a repeated declaration is worth de-duplicating.
///
/// # Errors
///
/// Returns [`CapCheckError::MissingCapabilities`] with a non-empty
/// `missing` vector when at least one declared capability is
/// absent from `invoker`.
pub fn check_subset(
    declared: &[String],
    invoker: &CapabilitySet,
) -> Result<(), CapCheckError> {
    let missing: Vec<String> = declared
        .iter()
        .filter(|cap| !invoker.contains(cap.as_str()))
        .cloned()
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CapCheckError::MissingCapabilities { missing })
    }
}
