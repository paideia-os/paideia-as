//! R227.M6 `#requires-paideia` version-compatibility checker.
//!
//! A `.pds` script's header may carry a `#requires-paideia >= X.Y.Z`
//! pragma (parsed by [`crate::header::parse_header`] into
//! `PdsHeader::requires_paideia`). Before the runtime hands that
//! script's body to the shell-lex layer, it must confirm the running
//! PaideiaOS is at least the version the script demands. This module
//! owns that check.
//!
//! The checker is deliberately dependency-free: it operates on the
//! parsed [`Version`] tuple and a compile-time [`SYSTEM_VERSION`]
//! constant. The subset rule at M2 asked whether the invoker's
//! capability set covered the script's declared set; the analogous
//! rule here asks whether the *running system* covers the script's
//! declared minimum. A script with no `#requires-paideia` passes
//! trivially — the pragma is opt-in.
//!
//! # Position in the pipeline
//!
//! ```text
//!   .pds source
//!       │
//!       ▼
//!   parse_header  ── PdsHeader { requires_paideia: Option<Version>, .. }
//!       │
//!       ▼
//!   PdsHeader::check_version()                       ← this module
//!       │
//!       ├── Ok(())                                    → cap-check, then load body
//!       └── Err(VersionTooLow { required, actual })   → refuse to load; report
//! ```
//!
//! # Semantics
//!
//! `#requires-paideia >= X.Y.Z` names the *minimum* version the
//! script is known to work with. A running system whose version is
//! greater than or equal to `X.Y.Z` under tuple ordering
//! `(major, minor, patch)` satisfies the demand; a strictly-lower
//! running system does not, and the load is refused with
//! [`VersionCheckError::VersionTooLow`].
//!
//! No pre-release or build-metadata suffix parsing is performed — the
//! M1 parser rejects anything that isn't three `u32`s, and this
//! comparator matches that shape exactly. Version-range operators
//! other than `>=` are likewise out of scope: the pragma's grammar
//! at R227.M1 fixed on `>=` and this checker follows.
//!
//! # Fingerprints
//!
//! The R227.M6 test corpus tags each fixture with `r227m6-ver-NN`
//! so the R220.M10 `@fingerprint` correlator can attribute pass/fail
//! to a specific fixture without re-parsing its name.

use std::fmt;

use crate::header::{PdsHeader, Version};

/// The PaideiaOS / paideia-as workspace version this build ships as.
///
/// Compared against a script's `#requires-paideia >= X.Y.Z` pin by
/// [`check_requires_paideia`]. Keep in lock-step with the
/// `[workspace.package] version` field of the top-level `Cargo.toml`:
/// **bump this constant whenever the workspace version bumps**, so a
/// script pinning the just-released version is not refused by the
/// build that ships it. A future R227.Mn milestone may replace this
/// hand-maintained constant with a `build.rs`-generated value read
/// from `CARGO_PKG_VERSION`; until then, treat this line as part of
/// the release checklist.
pub const SYSTEM_VERSION: Version = Version {
    major: 0,
    minor: 36,
    patch: 33,
};

/// Discriminated failure modes for [`check_requires_paideia`].
///
/// A single variant today (`VersionTooLow`); the enum shape mirrors
/// [`crate::cap_check::CapCheckError`] so R227.M7+ can add
/// range-operator or pre-release variants (`VersionOutOfRange`,
/// `UnsupportedPreRelease`, …) without a breaking signature change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionCheckError {
    /// The script's `#requires-paideia` pin demands a version strictly
    /// higher than [`SYSTEM_VERSION`]. `required` is the version the
    /// script pinned; `actual` is the running system version at the
    /// time of the check (i.e. [`SYSTEM_VERSION`]).
    VersionTooLow {
        /// The `X.Y.Z` triple the script pinned.
        required: Version,
        /// The running system version — copied out of [`SYSTEM_VERSION`]
        /// so a diagnostic doesn't have to look it up separately.
        actual: Version,
    },
}

impl fmt::Display for VersionCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionTooLow { required, actual } => {
                write!(
                    f,
                    "script requires paideia >= {}.{}.{}, but running system is {}.{}.{}",
                    required.major,
                    required.minor,
                    required.patch,
                    actual.major,
                    actual.minor,
                    actual.patch,
                )
            }
        }
    }
}

impl std::error::Error for VersionCheckError {}

/// Check a header's `#requires-paideia` pin against [`SYSTEM_VERSION`].
///
/// Returns `Ok(())` when the header carries no pin (the pragma is
/// opt-in) or when [`SYSTEM_VERSION`] is at least the pinned version
/// under tuple ordering `(major, minor, patch)`. Returns
/// [`VersionCheckError::VersionTooLow`] otherwise, echoing both the
/// pinned demand and the running system's version so the diagnostic
/// can name both sides of the mismatch without a second lookup.
///
/// # Errors
///
/// Returns [`VersionCheckError::VersionTooLow`] when `header` pins a
/// version strictly greater than [`SYSTEM_VERSION`].
pub fn check_requires_paideia(header: &PdsHeader) -> Result<(), VersionCheckError> {
    let Some(required) = header.requires_paideia else {
        return Ok(());
    };
    if SYSTEM_VERSION.is_at_least(&required) {
        Ok(())
    } else {
        Err(VersionCheckError::VersionTooLow {
            required,
            actual: SYSTEM_VERSION,
        })
    }
}
