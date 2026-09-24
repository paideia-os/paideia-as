//! UAX#29 extended-grapheme-cluster boundary iterator.
//!
//! The R229 REPL line editor's cursor-position math and the R228 tab
//! completion's argument tokenizer both need to reason about
//! *user-perceived characters*, not `char`s (which are Unicode scalar
//! values) and not bytes. UAX#29 §3 defines the extended grapheme
//! cluster — one "user-perceived character" — as a sequence of code
//! points that stay together across every operation the user sees as
//! atomic: cursor movement, selection, deletion, and rendering width.
//!
//! Backed by `unicode-segmentation::UnicodeSegmentation::grapheme_indices`,
//! which implements UAX#29 §3.1 rules GB1..GB13 including the
//! extended-pictographic ZWJ sequences (family emoji, professions,
//! skin-tone modifiers, flag sequences) and CR/LF fusing.

use unicode_segmentation::UnicodeSegmentation;

/// Iterator over the *byte offsets* of every extended-grapheme-cluster
/// boundary in a `&str`.
///
/// The first yielded offset is always `0` (the start of the string is a
/// boundary), and the last is always `s.len()` (the end of the string
/// is a boundary). For an empty string, only `0` is yielded (which
/// equals `s.len()`).
///
/// # Example
///
/// ```
/// use paideia_as_unicode::grapheme_boundaries;
///
/// // "é" as U+0065 U+0301 (2 chars, 3 bytes) is one grapheme.
/// let s = "e\u{0301}";
/// let boundaries: Vec<usize> = grapheme_boundaries(s).collect();
/// assert_eq!(boundaries, vec![0, 3]);
/// ```
#[derive(Clone, Debug)]
pub struct GraphemeBoundaries<'a> {
    /// The input slice; kept for `.len()` at end-of-iteration.
    input: &'a str,
    /// Front and back iterators over `(byte_offset, grapheme_slice)`.
    /// We only consume the byte offset; the slice is discarded.
    inner: unicode_segmentation::GraphemeIndices<'a>,
    /// Whether we still owe the caller the leading `0`.
    yielded_start: bool,
    /// Whether we still owe the caller the trailing `input.len()`.
    yielded_end: bool,
}

impl<'a> GraphemeBoundaries<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            inner: input.grapheme_indices(true),
            yielded_start: false,
            yielded_end: false,
        }
    }
}

impl<'a> Iterator for GraphemeBoundaries<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if !self.yielded_start {
            self.yielded_start = true;
            return Some(0);
        }
        // Every subsequent boundary is the *start* of the next cluster,
        // which `GraphemeIndices` yields as the tuple's first element.
        // We deliberately skip the very first tuple (its offset is 0,
        // which we already yielded above) by advancing past it once.
        loop {
            match self.inner.next() {
                Some((0, _)) => continue,
                Some((offset, _)) => return Some(offset),
                None => {
                    if !self.yielded_end {
                        self.yielded_end = true;
                        // For an empty string, start == end == 0 and
                        // the leading `0` we already yielded suffices;
                        // don't double-count. For any non-empty input
                        // the end boundary is strictly greater than
                        // every prior yielded offset (the last cluster
                        // starts at some offset < len).
                        if self.input.is_empty() {
                            return None;
                        }
                        return Some(self.input.len());
                    }
                    return None;
                }
            }
        }
    }
}

/// Return an iterator over every extended-grapheme-cluster boundary in
/// `input` as byte offsets, per UAX#29 §3.
pub fn grapheme_boundaries(input: &str) -> GraphemeBoundaries<'_> {
    GraphemeBoundaries::new(input)
}

/// Total number of extended grapheme clusters in `input`.
///
/// For any `s: &str`:
/// `grapheme_count(s) <= s.chars().count() <= s.len()`.
pub fn grapheme_count(input: &str) -> usize {
    input.graphemes(true).count()
}

/// Given a byte offset inside `input` that lies on a grapheme-cluster
/// boundary (or `0`), return the byte offset of the *next* boundary —
/// i.e., what a right-arrow keystroke in the R229 REPL should advance
/// the cursor to.
///
/// If `byte_offset >= input.len()`, returns `input.len()` (cursor
/// cannot advance past end-of-input). If `byte_offset` does not lie on
/// a grapheme boundary, we round down to the nearest boundary at or
/// before it and advance from there — the same convention `emacs`,
/// `readline`, and every well-behaved terminal line editor use.
///
/// # Panics
///
/// Does not panic. An out-of-range or mid-code-point `byte_offset` is
/// silently clamped, matching the "cursor math never crashes the REPL"
/// contract SH-D9 requires.
pub fn grapheme_advance(input: &str, byte_offset: usize) -> usize {
    if byte_offset >= input.len() {
        return input.len();
    }
    // Walk boundaries; return the first strictly greater than the
    // (rounded-down) input offset.
    let clamped = round_down_to_boundary(input, byte_offset);
    grapheme_boundaries(input)
        .find(|&b| b > clamped)
        .unwrap_or(input.len())
}

/// Symmetric counterpart of [`grapheme_advance`]: the byte offset of
/// the previous grapheme boundary. What a left-arrow keystroke should
/// yield.
pub fn grapheme_retreat(input: &str, byte_offset: usize) -> usize {
    if byte_offset == 0 {
        return 0;
    }
    let clamped = round_down_to_boundary(input, byte_offset.min(input.len()));
    // `rev` on `GraphemeBoundaries` is not directly available (the
    // wrapper only exposes forward iteration); collect and scan
    // instead. The boundary count is bounded by grapheme count, so
    // this is O(graphemes).
    let mut previous = 0usize;
    for boundary in grapheme_boundaries(input) {
        if boundary >= clamped {
            return previous;
        }
        previous = boundary;
    }
    previous
}

/// Round `byte_offset` down to the nearest grapheme boundary at or
/// before it (never past `input.len()`). Used by `grapheme_advance` /
/// `grapheme_retreat` to normalize a possibly-invalid cursor.
fn round_down_to_boundary(input: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(input.len());
    let mut best = 0usize;
    for boundary in grapheme_boundaries(input) {
        if boundary > clamped {
            break;
        }
        best = boundary;
    }
    best
}
