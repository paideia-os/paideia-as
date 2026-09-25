//! R227.M5 `.pds` script-as-functor shape.
//!
//! A `.pds` script's header declares a list of capabilities (via
//! `#capability "cap.name"` pragmas, parsed by
//! [`crate::header::parse_header`]). R227.M5 reframes that list as the
//! *formal parameter vector* of a **functor**: the script body is the
//! functor's implementation, and each declared capability is a
//! positional binding site that must be supplied — with a concrete
//! resolved capability handle — at application time.
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
//!   make_functor(&header, body, source_path)                        ← this module
//!       │  ── ScriptFunctor { params: Vec<CapParam>,
//!       │                    body:   Vec<u8>,
//!       │                    source_path: PathBuf }
//!       ▼
//!   apply(functor, resolved_caps)                                   ← this module
//!       │  ── FunctorApplication { functor, bindings: Vec<(String,String)> }
//!       ▼
//!   R227.M6+ arbiter cross-check + shell-lex the body
//! ```
//!
//! # Positional binding
//!
//! Parameters are named positionally — `p0`, `p1`, `p2`, … — mirroring
//! the order in which the header's `#capability` pragmas appeared. The
//! `cap_ident` field on each [`CapParam`] carries the original string
//! from the header (e.g. `"fs.read.home"`); this is the *type* the
//! arbiter will later cross-check against the resolved capability
//! handle supplied at application. R227.M5 does not perform that
//! cross-check — [`apply`] binds strictly by position, and a resolved
//! capability whose name disagrees with the declared `cap_ident` is
//! recorded in `bindings` as-is. The R227.M6 arbitration layer is the
//! right place for that check (it holds the canonicalisation rules and
//! knows when a supplied grant subsumes a declared one).
//!
//! # Why a separate module
//!
//! The functor shape is deliberately **not** folded into
//! [`crate::PdsHeader`]. A header describes *what the source declares*;
//! a functor describes *how the loader consumes those declarations at
//! application time*. Keeping them apart lets the header parser stay a
//! pure syntactic pass — no allocation of the body, no notion of
//! "source path" — while [`ScriptFunctor`] owns the body bytes and the
//! filesystem provenance the runtime needs downstream.
//!
//! # Fingerprints
//!
//! The R227.M5 test corpus tags each fixture with `r227m5-func-NN` so
//! the R220.M10 `@fingerprint` correlator can attribute pass/fail to a
//! specific fixture without re-parsing its name.

use std::fmt;
use std::path::PathBuf;

/// One positional capability parameter of a [`ScriptFunctor`].
///
/// `name` is a locally-generated positional binding (`p0`, `p1`, …)
/// that mirrors the order of `#capability` pragmas in the header;
/// `cap_ident` is the string as written in the header (e.g.
/// `"fs.read.home"`) — the *type* the arbiter will later cross-check
/// against the resolved capability handle supplied at application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapParam {
    /// Positional binding site name (`p0`, `p1`, `p2`, …). Generated
    /// from the parameter's index in the header, not from any user-
    /// supplied identifier — script-side references to a formal
    /// parameter use this positional name.
    pub name: String,
    /// Capability identifier as written in the header pragma (byte-
    /// exact, no canonicalisation). Retained so a later arbiter can
    /// confirm that the resolved capability supplied at application
    /// matches what the script declared it would consume.
    pub cap_ident: String,
}

/// A `.pds` script viewed as a functor: a body of bytes parameterised
/// by an ordered list of capability parameters.
///
/// Produced by [`make_functor`] from a parsed [`crate::PdsHeader`] and
/// the corresponding body slice. The body is copied into the struct so
/// callers can drop the original source buffer immediately; the source
/// path is retained purely as filesystem provenance (for diagnostics
/// and, downstream, for module-graph identity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptFunctor {
    /// Positional capability parameters, in header-declaration order.
    /// Zero-length when the header declared no `#capability` pragmas —
    /// a 0-arity functor is legal and describes a script that names no
    /// capabilities.
    pub params: Vec<CapParam>,
    /// The script body — every byte from `header.body_offset` to end
    /// of source, cloned. Kept as `Vec<u8>` (not `String`) so a body
    /// that is not valid UTF-8 does not fault this shape; the shell-
    /// lex layer decides UTF-8 policy on its own.
    pub body: Vec<u8>,
    /// Filesystem path the source was loaded from. Preserved verbatim
    /// through [`apply`] into [`FunctorApplication::functor`].
    pub source_path: PathBuf,
}

/// The result of applying a [`ScriptFunctor`] to a concrete resolved-
/// capability vector.
///
/// `bindings` pairs each formal parameter's positional name with the
/// resolved-capability string that fills it, in header-declaration
/// order. R227.M5 records the pairing verbatim; the R227.M6 arbiter
/// consumes `bindings` alongside `functor.params[i].cap_ident` to
/// perform the eventual grant-vs-declaration cross-check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctorApplication {
    /// The functor as supplied to [`apply`], preserved so downstream
    /// consumers can reach the body bytes and source path through a
    /// single value.
    pub functor: ScriptFunctor,
    /// `(param.name, resolved_cap)` pairs in header-declaration order.
    /// Length equals `functor.params.len()` — [`apply`] refuses arity
    /// mismatches before constructing this vector.
    pub bindings: Vec<(String, String)>,
}

/// Discriminated failure modes for [`apply`].
///
/// `ArityMismatch` is the only variant R227.M5 raises. `MissingCap` is
/// reserved for the R227.M6 arbitration layer, which will refuse an
/// application whose resolved-capability vector is arity-correct but
/// names a right the arbiter cannot honour; keeping the variant on the
/// enum today avoids a breaking signature change when that layer lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The resolved-capability vector's length did not match the
    /// functor's formal parameter arity.
    ArityMismatch {
        /// The number of formal parameters the functor declares.
        expected: usize,
        /// The number of resolved capabilities the caller supplied.
        got: usize,
    },
    /// Reserved for R227.M6: the resolved vector was arity-correct but
    /// a specific named parameter could not be satisfied by any of the
    /// arbiter's grants. Not produced by the R227.M5 [`apply`] path.
    MissingCap {
        /// The formal parameter name (`p0`, `p1`, …) that could not
        /// be satisfied.
        name: String,
    },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArityMismatch { expected, got } => {
                write!(
                    f,
                    "capability arity mismatch: functor expects {expected} parameter(s), got {got}"
                )
            }
            Self::MissingCap { name } => {
                write!(f, "capability parameter `{name}` has no grant")
            }
        }
    }
}

impl std::error::Error for ApplyError {}

/// Build a [`ScriptFunctor`] from a parsed header, the body bytes, and
/// the source path.
///
/// The header's `capabilities` vector becomes the functor's formal
/// parameter list: entry `i` yields a [`CapParam`] whose `name` is
/// `format!("p{i}")` and whose `cap_ident` clones
/// `header.capabilities[i]`. `body` is cloned into the returned struct
/// so the caller may drop the source buffer.
///
/// A header with no `#capability` pragmas produces a 0-arity functor —
/// a legal, first-class shape describing a script that names no
/// capabilities. This mirrors the C-level treatment of a nullary
/// function: neither the parser nor this constructor treats "no
/// parameters" as an error.
pub fn make_functor(
    header: &crate::PdsHeader,
    body: &[u8],
    source_path: PathBuf,
) -> ScriptFunctor {
    let params = header
        .capabilities
        .iter()
        .enumerate()
        .map(|(i, cap)| CapParam {
            name: format!("p{i}"),
            cap_ident: cap.clone(),
        })
        .collect();
    ScriptFunctor {
        params,
        body: body.to_vec(),
        source_path,
    }
}

/// Apply a [`ScriptFunctor`] to a concrete vector of resolved
/// capability strings.
///
/// The application is *positional*: `resolved_caps[i]` fills the
/// functor's `i`-th formal parameter. R227.M5 performs a single check —
/// arity — and refuses any mismatch with
/// [`ApplyError::ArityMismatch`]. It does **not** cross-check that
/// `resolved_caps[i]` names the same right as
/// `functor.params[i].cap_ident`; that check belongs in the R227.M6
/// arbitration layer, which owns the canonicalisation rules and knows
/// when a supplied grant subsumes the declared one.
///
/// On success the resolved-capability vector is consumed into a
/// `bindings` vector of `(param.name, resolved_cap)` pairs, and the
/// original functor is returned unchanged inside the
/// [`FunctorApplication`] so downstream consumers can reach the body
/// and source path through a single value.
///
/// # Errors
///
/// Returns [`ApplyError::ArityMismatch`] when
/// `resolved_caps.len() != functor.params.len()`. R227.M5 raises no
/// other variant; [`ApplyError::MissingCap`] is reserved for R227.M6.
pub fn apply(
    functor: ScriptFunctor,
    resolved_caps: &[String],
) -> Result<FunctorApplication, ApplyError> {
    if resolved_caps.len() != functor.params.len() {
        return Err(ApplyError::ArityMismatch {
            expected: functor.params.len(),
            got: resolved_caps.len(),
        });
    }
    let bindings = functor
        .params
        .iter()
        .zip(resolved_caps.iter())
        .map(|(p, r)| (p.name.clone(), r.clone()))
        .collect();
    Ok(FunctorApplication { functor, bindings })
}
