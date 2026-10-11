//! UNI-3 (paideia-as#1590) — UAX #14 line-break opportunities + the
//! `wrap_at_width` helper that composes with UNI-W1's `str_width` and
//! grapheme-cluster segmentation.
//!
//! # What this module owns
//!
//! * [`LineBreak`] — the three UAX #14 opportunity classes
//!   (`Mandatory`, `Allowed`, `Prohibited`) in paideia-as vocabulary.
//!   `#[non_exhaustive]` so future UAX revisions (CSS word-break, UAX
//!   #14 soft-hyphen extension) can add variants without a major-version
//!   bump; `#[default] Allowed` so a defaulted opportunity does not
//!   accidentally become a hard break.
//! * [`LineBreakOpportunity`] — a `(byte_offset, kind)` pair naming a
//!   single opportunity. The offset follows `unicode-linebreak`'s native
//!   convention: it is the byte index of the *character immediately
//!   following* the break — i.e. the start of the next line were a
//!   break taken here. For `"hello world"` this makes the opportunity
//!   between the space and `'w'` be byte 6 (where `'w'` begins), which
//!   is also what a `s[..6] | s[6..]` split would use.
//! * [`line_break_opportunities`] — enumerate every opportunity in a
//!   `&str`. Return value is a `Vec<LineBreakOpportunity>` (not an
//!   iterator), matching the paideia-fmt FMT-7 precedent and keeping
//!   the semantic-shell call sites simple (R229 cursor layout wants
//!   a slice it can binary-search).
//! * [`wrap_at_width`] — greedy word-wrap that respects UAX #14
//!   opportunities and UAX #11 column widths. Mandatory breaks
//!   (`\n`, LF, PS, LS, CRLF) are always honored; Allowed breaks are
//!   taken greedily to fit `max_width` columns. When a single
//!   unbreakable segment exceeds `max_width`, a grapheme-level forced
//!   break prevents infinite loops (the "aaaa" ASCII fast-path case).
//! * [`next_line_break`] — streaming helper for incremental layout.
//!   Returns the first opportunity at offset `≥ from`.
//! * [`SegmentError`] + [`SegmentErrorKind`] — the UNI-0 §6.3 error
//!   taxonomy. `line_break_opportunities` is total; only `wrap_at_width`
//!   can fail (currently, nothing in this module returns a
//!   `SegmentError` — the type is defined for the UNI-11 API freeze
//!   so future `try_wrap_at_width` additions can reuse it without a
//!   major-version bump).
//!
//! # ASCII fast path
//!
//! An input whose every byte is `< 0x80` and that contains neither `\n`
//! nor `\r` has `str_width == s.len()` and is normalized in every form.
//! For `wrap_at_width` on such an input we drop into a byte-local greedy
//! wrapper (walk ASCII spaces as break candidates, chunk by `max_width`
//! bytes). This matters because the R222/R228 command-name and help-text
//! workloads dominate wrap traffic and are overwhelmingly pure ASCII;
//! short-circuiting avoids pulling the UCD line-break table into the
//! L1.
//!
//! # Trailing whitespace
//!
//! When a wrap boundary falls after a run of spaces, the emitted line
//! slice **trims** that trailing whitespace. This matches the
//! `textwrap` and GNU `fmt(1)` convention: the user expects
//! `wrap("hello world", 5)` to produce `["hello", "world"]`, not
//! `["hello ", "world"]`. The returned `&str` is still a sub-slice of
//! the input (we trim by shrinking the end-index; no reallocation).
//!
//! # Mandatory-break skip
//!
//! At a mandatory break (`\n`, `\r\n`, `\r`, `\x0C`, `\x0B`, U+2028,
//! U+2029), the newline bytes themselves are *consumed* — the next line
//! begins immediately after them. This is a convenience: were we to
//! include the LF in the emitted slice, every caller would have to
//! strip it, and the UAX #14 convention is that the break *replaces*
//! the LF for display purposes.

use unicode_segmentation::UnicodeSegmentation;

use crate::width::{grapheme_width, str_width};

/// The three UAX #14 line-break opportunity classes, in paideia-as
/// vocabulary.
///
/// `Mandatory` opportunities are hard breaks UAX #14 rule LB4 / LB5
/// requires: LF, CR + LF (one break), standalone CR, form-feed, next
/// line (U+0085), line separator (U+2028), paragraph separator
/// (U+2029). A renderer MUST break there.
///
/// `Allowed` opportunities are soft breaks the renderer MAY take if a
/// wrap column limit would otherwise be exceeded. UAX #14 enumerates
/// these via the pair table (LB6..LB30); `unicode-linebreak` executes
/// that table on our behalf.
///
/// `Prohibited` is reserved for a future enrichment of the API — the
/// current `line_break_opportunities` iterator yields only `Mandatory`
/// and `Allowed` positions (positions where no opportunity exists are
/// simply absent from the stream). A future per-byte query interface
/// would emit `Prohibited` to say "the UAX #14 class pair here is
/// no-break"; the variant is here now so adding that query does not
/// break downstream exhaustive match arms (`#[non_exhaustive]` would
/// cover that too, but having the variant on the enum lets the enum's
/// `Debug` / match-pattern surface stabilize at UNI-11 freeze time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum LineBreak {
    /// UAX #14 LB4 / LB5 hard break (LF, CRLF, CR, FF, VT, NEL, LS, PS).
    /// A renderer MUST break here regardless of column budget.
    Mandatory,
    /// UAX #14 LB6..LB30 soft break. A renderer MAY break here; our
    /// `wrap_at_width` takes these greedily to fit `max_width`.
    #[default]
    Allowed,
    /// Reserved for a future per-byte query interface (`is_break_at`).
    /// Not currently emitted by [`line_break_opportunities`].
    Prohibited,
}

/// A single UAX #14 opportunity: where in the input a break may (or
/// must) occur, and of what kind.
///
/// `byte_offset` follows `unicode-linebreak`'s native convention: it is
/// the byte index of the *first character of the next line* were a
/// break taken here. In particular, `s.split_at(opp.byte_offset)` is
/// always a valid UTF-8-boundary split, and the end-of-string sentinel
/// opportunity has `byte_offset == s.len()`.
///
/// `#[non_exhaustive]` so future fields (soft-hyphen hint, bidi mirror
/// flag) can be added without a major-version bump.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LineBreakOpportunity {
    /// Byte offset of the character following the break. Always a
    /// UTF-8 boundary; always in `0..=s.len()`.
    pub byte_offset: usize,
    /// Which UAX #14 class this opportunity falls under.
    pub kind: LineBreak,
}

/// A segmentation operation failed.
///
/// Currently only `wrap_at_width` can produce one (and it does not
/// today — the forced-break fallback keeps the function total).
/// The type is defined for the UNI-11 API freeze so future
/// `try_wrap_at_width` / `try_segment_lines` additions can reuse
/// the taxonomy without a major-version bump.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SegmentError {
    /// Byte offset into the input slice at which segmentation failed.
    pub offset: usize,
    /// Discriminated reason.
    pub kind: SegmentErrorKind,
}

/// Discriminated reason a segmentation operation failed.
///
/// `#[non_exhaustive]` so future UCD-driven failure classes (e.g.
/// malformed emoji ZWJ sequences at UNI-10's grapheme-stress tests,
/// bidi-run isolation faults at a future UAX #9 landing) can be added
/// without a major-version bump.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SegmentErrorKind {
    /// `wrap_at_width` was asked to wrap at a width smaller than the
    /// smallest unbreakable cluster. The `minimum` field names the
    /// smallest column count that would have let the operation succeed
    /// without a forced in-cluster break. Not currently returned —
    /// `wrap_at_width` falls back to a grapheme-level forced break
    /// rather than erroring — but reserved for a future
    /// `try_wrap_at_width` strict variant.
    WidthTooSmall {
        /// The minimum `max_width` the input would require.
        minimum: usize,
    },
}

impl core::fmt::Display for SegmentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "segmentation error at byte {}: {}", self.offset, self.kind)
    }
}

impl core::fmt::Display for SegmentErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WidthTooSmall { minimum } => {
                write!(f, "wrap width too small (minimum {} columns required)", minimum)
            }
        }
    }
}

impl std::error::Error for SegmentError {}

// ---------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------

/// Enumerate every UAX #14 line-break opportunity in `s`.
///
/// The returned vector is sorted ascending by `byte_offset`, and the
/// last entry always has `byte_offset == s.len()` with
/// `kind == LineBreak::Mandatory` (UAX #14 LB3 — the end of text is
/// always a mandatory break). An empty input returns an empty vector.
///
/// Only `Allowed` and `Mandatory` opportunities are emitted. Positions
/// in `s` where UAX #14 prohibits a break are simply absent from the
/// stream; the `Prohibited` variant of [`LineBreak`] is reserved for a
/// future per-byte query API.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{LineBreak, line_break_opportunities};
///
/// let opps = line_break_opportunities("foo\nbar");
/// // Mandatory break after '\n' at byte 4, mandatory at EOS at byte 7.
/// assert_eq!(opps.first().map(|o| (o.byte_offset, o.kind)),
///            Some((4, LineBreak::Mandatory)));
/// ```
pub fn line_break_opportunities(s: &str) -> Vec<LineBreakOpportunity> {
    if s.is_empty() {
        return Vec::new();
    }
    unicode_linebreak::linebreaks(s)
        .map(|(offset, class)| LineBreakOpportunity {
            byte_offset: offset,
            kind: map_break_opportunity(class),
        })
        .collect()
}

/// Return the first UAX #14 opportunity at offset `≥ from`.
///
/// `None` if no opportunity exists at or after `from` (which can only
/// happen when `from > s.len()`; otherwise the end-of-text mandatory
/// break at `s.len()` always matches).
///
/// This is the streaming helper the R229 renderer's incremental
/// re-layout path calls — rather than re-enumerating every opportunity
/// in the whole buffer on each cursor move, it queries forward from
/// the dirty mark.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{LineBreak, next_line_break};
///
/// let (off, kind) = next_line_break("hello world", 0).unwrap();
/// // First opportunity: Allowed break between the space and 'w'.
/// assert_eq!((off, kind), (6, LineBreak::Allowed));
/// ```
pub fn next_line_break(s: &str, from: usize) -> Option<(usize, LineBreak)> {
    if s.is_empty() {
        return None;
    }
    for (offset, class) in unicode_linebreak::linebreaks(s) {
        if offset >= from {
            return Some((offset, map_break_opportunity(class)));
        }
    }
    None
}

/// Greedy word-wrap `s` so each returned slice has `str_width` at
/// most `max_width`.
///
/// Honors UAX #14 mandatory breaks (`\n`, `\r\n`, `\r`, LS, PS, FF,
/// VT, NEL) unconditionally. Allowed breaks are taken greedily to fit
/// `max_width` columns, measured by [`crate::str_width`] so CJK
/// ideographs count as two columns and ZWJ emoji clusters collapse to
/// their base cell. Trailing whitespace at each wrap boundary is
/// stripped from the emitted slice (so `wrap_at_width("hello world", 5)`
/// returns `["hello", "world"]`, not `["hello ", "world"]`).
///
/// When a single unbreakable UAX #14 segment is wider than `max_width`
/// (e.g. `wrap_at_width("aaaa", 2)` where there is no mid-"aaaa" break
/// opportunity), the function falls back to a *forced* grapheme-level
/// break rather than overflowing or looping — the "aaaa" case returns
/// `["aa", "aa"]`.
///
/// An input where `max_width == 0` is treated as `max_width == 1`
/// (we must emit at least one column per line to make progress).
///
/// An empty input returns an empty vector.
///
/// # ASCII fast path
///
/// If `s.is_ascii()` and `s` contains neither `\n` nor `\r`,
/// `str_width(s) == s.len()`, so the wrap decision reduces to a
/// byte-local scan — no UCD table ever loads. The slow path handles
/// everything else (mixed-script, CJK, embedded newlines).
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::wrap_at_width;
///
/// assert_eq!(wrap_at_width("hello world", 5), vec!["hello", "world"]);
/// assert_eq!(wrap_at_width("ab\nc", 10),     vec!["ab",    "c"]);
/// assert_eq!(wrap_at_width("日本語", 2),     vec!["日",   "本", "語"]);
/// ```
pub fn wrap_at_width(s: &str, max_width: usize) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    let max_width = max_width.max(1);

    // ASCII fast path: no UCD table, no grapheme walk — bytes are
    // columns 1:1 and the newline-less check means no mandatory
    // breaks need honoring.
    if s.is_ascii() && !s.as_bytes().contains(&b'\n') && !s.as_bytes().contains(&b'\r') {
        return wrap_ascii(s, max_width);
    }

    wrap_general(s, max_width)
}

// ---------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------

/// Map `unicode-linebreak`'s class to our enum. The crate exposes two
/// variants, `Mandatory` and `Allowed`; our `Prohibited` variant is
/// reserved for a future per-byte API and never emitted here.
#[inline]
fn map_break_opportunity(class: unicode_linebreak::BreakOpportunity) -> LineBreak {
    match class {
        unicode_linebreak::BreakOpportunity::Mandatory => LineBreak::Mandatory,
        unicode_linebreak::BreakOpportunity::Allowed => LineBreak::Allowed,
    }
}

/// Byte-local greedy wrapper for pure-ASCII, newline-free input.
///
/// Walks `s` scanning for spaces (the only ASCII break-candidate class);
/// at each `max_width`-byte boundary, back up to the last space seen in
/// the current window and emit the slice up to (but not including) it.
/// When no space is available in the window, forces a byte-level break
/// — the test corpus's `wrap_at_width("aaaa", 2) → ["aa", "aa"]` case.
fn wrap_ascii(s: &str, max_width: usize) -> Vec<&str> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut result = Vec::new();
    let mut start = 0usize;

    while start < n {
        // Skip any leading whitespace on this line.
        while start < n && bytes[start] == b' ' {
            start += 1;
        }
        if start >= n {
            break;
        }

        let max_end = (start + max_width).min(n);

        // Find the last space in bytes[start..=max_end] (inclusive of
        // max_end only when max_end == n, otherwise we never look past
        // max_end). We prefer breaking at the latest space that keeps
        // the trimmed line within max_width columns.
        let mut break_pos = max_end;
        if max_end < n {
            // Scan backward from max_end down to start+1 for a space.
            let mut j = max_end;
            let mut found = None;
            while j > start {
                if bytes[j] == b' ' {
                    found = Some(j);
                    break;
                }
                j -= 1;
            }
            if let Some(sp) = found {
                break_pos = sp;
            }
            // else: no space in window — forced break at max_end.
        }

        // Emit s[start..break_pos] with any trailing whitespace stripped.
        let line = &s[start..break_pos];
        let trimmed_len = line.trim_end().len();
        if trimmed_len > 0 {
            result.push(&s[start..start + trimmed_len]);
        }
        start = break_pos;
    }

    result
}

/// Full UAX #14-driven wrap for inputs with embedded newlines, CJK,
/// combining marks, or any non-ASCII byte. Walks the opportunity
/// stream and runs the greedy algorithm against UAX #11 column widths.
fn wrap_general(s: &str, max_width: usize) -> Vec<&str> {
    let opps = line_break_opportunities(s);
    let mut result: Vec<&str> = Vec::new();
    let mut start = 0usize;
    // (next_start_after_opp, trimmed_end_byte) of the last Allowed
    // break that fit within `max_width`.
    let mut last_fit: Option<(usize, usize)> = None;

    for opp in &opps {
        let end = opp.byte_offset;
        if end <= start {
            // Opportunity sits inside a region we have already emitted
            // (can happen after a forced break advanced `start` past
            // an earlier opportunity). Skip.
            continue;
        }

        let seg = &s[start..end];
        let trim_end_byte = start + seg.trim_end().len();
        let seg_width = str_width(&s[start..trim_end_byte]);

        match opp.kind {
            LineBreak::Mandatory => {
                if seg_width <= max_width {
                    // The whole segment fits as-is: emit it (trimmed)
                    // and advance past the mandatory-break bytes.
                    if trim_end_byte > start {
                        result.push(&s[start..trim_end_byte]);
                    }
                    start = end;
                    last_fit = None;
                } else {
                    // Segment is wider than max_width. First try to
                    // use the last Allowed fit (if any), then recurse
                    // from the resulting start.
                    if let Some((lf_end, lf_trim)) = last_fit {
                        if lf_trim > start {
                            result.push(&s[start..lf_trim]);
                        }
                        start = lf_end;
                        last_fit = None;
                    }
                    // Now break anything remaining up to the mandatory
                    // point into grapheme-forced chunks.
                    force_wrap_range(s, start, end, max_width, &mut result);
                    start = end;
                }
            }
            LineBreak::Allowed => {
                if seg_width <= max_width {
                    last_fit = Some((end, trim_end_byte));
                } else {
                    // Current segment too wide. Fall back to the last
                    // Allowed fit, then re-examine the current one
                    // against the new start.
                    if let Some((lf_end, lf_trim)) = last_fit {
                        if lf_trim > start {
                            result.push(&s[start..lf_trim]);
                        }
                        start = lf_end;
                        last_fit = None;
                        // Re-evaluate the current opportunity against
                        // the shrunk window.
                        let new_seg = &s[start..end];
                        let new_trim_end = start + new_seg.trim_end().len();
                        let new_w = str_width(&s[start..new_trim_end]);
                        if new_w <= max_width {
                            last_fit = Some((end, new_trim_end));
                        } else {
                            // Still too wide even standalone — force.
                            force_wrap_range(s, start, end, max_width, &mut result);
                            start = end;
                        }
                    } else {
                        // No prior fit; force a break within this
                        // over-wide run.
                        force_wrap_range(s, start, end, max_width, &mut result);
                        start = end;
                    }
                }
            }
            LineBreak::Prohibited => {
                // Not emitted by `line_break_opportunities` today;
                // treat as a no-op for forward compatibility.
            }
        }
    }

    // The last opportunity is always Mandatory at s.len(), so under
    // normal flow `start == s.len()` here. The guard is defensive
    // against future `Prohibited`-skipping that leaves a tail.
    if start < s.len() {
        let tail = &s[start..];
        let trim_len = tail.trim_end().len();
        if trim_len > 0 {
            result.push(&s[start..start + trim_len]);
        }
    }

    result
}

/// Grapheme-level forced wrap of `s[start..end]`, pushing chunks that
/// each fit within `max_width` columns into `result`. Each chunk is
/// trimmed of trailing whitespace.
///
/// This handles the UAX #14-insufficient case: a single unbreakable
/// run (like `"aaaa"` with no mid-word opportunity) that is wider than
/// `max_width`. Rather than overflow or infinite-loop, we break at the
/// first grapheme boundary that would push the chunk over the limit.
fn force_wrap_range<'a>(
    s: &'a str,
    start: usize,
    end: usize,
    max_width: usize,
    result: &mut Vec<&'a str>,
) {
    if end <= start {
        return;
    }
    let segment = &s[start..end];
    let mut chunk_start = start;
    let mut chunk_w = 0usize;

    for (gidx, gc) in segment.grapheme_indices(true) {
        let gw = grapheme_width(gc);
        if chunk_w > 0 && chunk_w + gw > max_width {
            let chunk_end_byte = start + gidx;
            let trim_len = s[chunk_start..chunk_end_byte].trim_end().len();
            if trim_len > 0 {
                result.push(&s[chunk_start..chunk_start + trim_len]);
            }
            chunk_start = start + gidx;
            chunk_w = 0;
        }
        chunk_w = chunk_w.saturating_add(gw);
    }

    // Flush the final chunk.
    if chunk_start < end {
        let trim_len = s[chunk_start..end].trim_end().len();
        if trim_len > 0 {
            result.push(&s[chunk_start..chunk_start + trim_len]);
        }
    }
}
