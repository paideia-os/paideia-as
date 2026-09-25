//! R225.M8: HM LSP hover — resolve a [`MonoType`] at a source position.
//!
//! Where R225.M7 gave the inference layer a *presentation* channel for
//! failures — [`crate::diagnostic::TypeDiagnostic`] with an optional
//! [`crate::diagnostic::TypeSpan`] — R225.M8 gives it a *query* channel
//! for successes: a [`TypeCache`] populated during (or after) inference
//! with `(span, mono)` entries, and a [`hover_at`] lookup that resolves
//! the tightest span covering a given byte offset. This is the substrate
//! an LSP server's `textDocument/hover` request lowers onto.
//!
//! # Design notes
//!
//! * **The cache is caller-populated.** This crate never mints spans of
//!   its own — spans belong to the surface parser, and the M8 layer's
//!   contract is to store and index whatever the caller hands over. The
//!   R229 shell will drive the population from its parser; the M1–M6
//!   inference layers stay untouched.
//! * **The lookup is smallest-enclosing.** A cache typically contains
//!   nested spans (a `let` body's outer span encloses the inner
//!   expression's spans, and each of those encloses their own sub-spans).
//!   Hover resolution wants the *innermost* enclosing entry — the same
//!   convention rustc's `hover_type_of` and most LSP implementations use
//!   — so [`hover_at`] returns the candidate with the smallest
//!   `end - start` among those covering `pos`.
//! * **Half-open span semantics.** Following
//!   [`crate::diagnostic::TypeSpan`] and `Range<usize>`, a span
//!   `[start, end)` covers positions `start..end` — an offset `end`
//!   itself belongs to whatever span begins there, not to the one that
//!   ended there. The `start <= pos < end` check enforces this.
//! * **Deterministic tie-break by insertion order.** When two spans have
//!   the same length and both cover `pos`, [`Iterator::min_by_key`]
//!   returns the *first* seen minimum. Callers that need to disambiguate
//!   further should push the more-specific entry first (parser passes
//!   naturally do this).
//!
//! # Non-goals at M8
//!
//! * No incremental invalidation — a re-check builds a fresh cache. The
//!   R229 shell may layer an interval-tree index on top when the corpus
//!   grows large enough to matter; the current linear filter is
//!   O(entries) per hover, which is fine for the shell's per-file scale.
//! * No hover *content* policy beyond "label plus displayed monotype".
//!   Fancier rendering (parameter names, doc snippets, source links) is
//!   left to a later milestone once the surface UX has opinions.

use crate::diagnostic::TypeSpan;
use crate::ty::MonoType;

/// One hover entry: a source span and the [`MonoType`] the caller wants
/// to surface for it, plus a human-readable label (typically the
/// syntactic form's name — `"x"` for a variable, `"pipeline stage 2"`
/// for a synthesised binding).
///
/// The label is a plain string rather than a structured enum because
/// its consumer is a presentation layer (an LSP hover popup, a REPL
/// `:type` command), not an algebraic pass — the crate never inspects
/// its contents.
#[derive(Clone, Debug)]
pub struct HoverEntry {
    /// The source span this entry describes.
    pub span: TypeSpan,
    /// The inferred monotype at that span.
    pub mono: MonoType,
    /// A short human-readable label — typically the syntactic form's
    /// name. Displayed left of the `:` in [`render_hover`].
    pub label: String,
}

/// A collection of hover entries, keyed by their source spans.
///
/// The cache is intentionally a flat vector rather than an interval
/// tree: the shell's per-file scale keeps linear lookup cheap, and a
/// vector composes naturally with the parser's push-order population.
/// A future milestone may layer an index on top when the entry count
/// grows large enough to make the O(n) filter measurable.
#[derive(Clone, Debug, Default)]
pub struct TypeCache {
    /// The entries in insertion order — see [`hover_at`] for the
    /// tie-break policy when two entries have equal-length spans.
    pub entries: Vec<HoverEntry>,
}

impl TypeCache {
    /// Construct an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one entry. Ordering matters only as a tie-breaker for
    /// [`hover_at`] — see the module docs.
    pub fn push(&mut self, entry: HoverEntry) {
        self.entries.push(entry);
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache carries no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Resolve the tightest hover entry covering `pos`.
///
/// Returns the entry whose span both encloses `pos` under half-open
/// semantics (`span.start <= pos < span.end`) *and* has the smallest
/// `end - start` among all such candidates. When no entry covers `pos`
/// — an out-of-source position, or an unindexed region — returns
/// [`None`]. Ties among equal-length spans resolve to the
/// first-inserted, matching `Iterator::min_by_key`'s stability contract.
pub fn hover_at<'a>(cache: &'a TypeCache, pos: usize) -> Option<&'a HoverEntry> {
    cache
        .entries
        .iter()
        .filter(|e| e.span.start <= pos && pos < e.span.end)
        .min_by_key(|e| e.span.end - e.span.start)
}

/// Render a hover entry to a single line: `"{label}: {mono}"`.
///
/// Delegates the type half to [`MonoType`]'s `Display` impl so every
/// row / effect / typed-value shape from R225.M2–M5 renders through
/// its existing text unmodified. Callers building richer surfaces (an
/// LSP hover markdown block, say) should treat this as the default
/// one-liner and layer their own formatting on top.
pub fn render_hover(entry: &HoverEntry) -> String {
    format!("{}: {}", entry.label, entry.mono)
}
