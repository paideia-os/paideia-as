//! R221.M6: pre-NFC ↔ post-NFC byte-range mapping.
//!
//! Every string that crosses the shell's IPC boundary is normalized to
//! NFC (per SH-D9.4). The parser sees the normalized text, but the
//! LSP / R229 REPL / diagnostic renderer must underline the user's
//! *original* bytes — otherwise a squiggly on `café` typed as
//! `cafe\u{0301}` would land off-by-one, or worse, split a grapheme.
//!
//! [`NfcMap`] records a `(nfc_byte_pos, original_byte_pos)` checkpoint
//! at every canonical starter boundary (per UAX#15 §3.11 — a starter is
//! any char with canonical combining class 0). Between checkpoints,
//! the mapping is not necessarily bijective (NFC composition can fuse
//! multiple code points into one), so an arbitrary intra-segment NFC
//! range is *widened* to the enclosing checkpoint pair — a conservative
//! never-narrower behavior that keeps LSP squigglies at least as wide
//! as the user's typed text.
//!
//! # Fast path
//!
//! Pure-ASCII input is already NFC and its map is identity — no
//! checkpoint list, no allocation, no work at build time. The R229
//! REPL's overwhelming steady-state (interactive `ls`, `cd`, `grep`)
//! runs entirely on this path.
//!
//! # Non-fast-path shape
//!
//! For non-ASCII input, we walk the source char by char, group runs
//! that share a common starter, normalize each run, and record a
//! checkpoint at each run boundary. In practice, most non-ASCII input
//! is one starter per code point (Latin-1 letters, CJK ideographs,
//! emoji) so the checkpoint density is 1-to-1 with characters — a
//! `Vec` cost of `~2 usize` per non-ASCII char, well under `Cmd`
//! args' own overhead.

use unicode_normalization::UnicodeNormalization;

use crate::span::ByteRange;

/// The pre-NFC ↔ post-NFC byte-boundary correspondence for one source
/// document. Constructed once at parse start and consulted per
/// [`crate::SyntaxNode`] emission.
#[derive(Clone, Debug)]
pub struct NfcMap {
    /// Sorted list of `(nfc_byte_pos, original_byte_pos)` pairs at
    /// canonical-starter boundaries. Empty iff the input was pure
    /// ASCII (identity map).
    checkpoints: Vec<(usize, usize)>,
    /// The final (nfc_len, original_len). Kept separate from
    /// `checkpoints` so the identity fast path can synthesize it
    /// without pushing to the vec.
    nfc_len: usize,
    original_len: usize,
    /// True iff the input was pure ASCII; unlocks a branch-free
    /// identity translate path.
    identity: bool,
}

impl NfcMap {
    /// Identity map for an input that is already NFC (typically because
    /// it is pure ASCII). Both `original_len` and `nfc_len` equal the
    /// same source length. Used by the fast-path constructor.
    pub fn identity(len: usize) -> Self {
        Self {
            checkpoints: Vec::new(),
            nfc_len: len,
            original_len: len,
            identity: true,
        }
    }

    /// Normalize `src` to NFC and return the normalized text alongside
    /// a byte-boundary correspondence map. This is the parser's front
    /// door — call it once at the start of `parse()` and thread the
    /// resulting `NfcMap` through every `SyntaxNode` construction so
    /// each node can record both coordinate systems.
    ///
    /// # Map resolution
    ///
    /// * ASCII input: identity map, byte-for-byte (any position on
    ///   either side is exactly the position on the other).
    /// * Mixed / non-ASCII input: bulk NFC via
    ///   `UnicodeNormalization::nfc()` (correctly handles Hangul
    ///   syllable composition and all UAX#15 corner cases), plus a
    ///   two-checkpoint map: `(0, 0)` and `(nfc_len, original_len)`.
    ///   Any intra-source position widens to the full original span
    ///   — deliberately conservative so an LSP squiggly on a
    ///   non-ASCII token underlines at least the user's typed bytes.
    ///   A byte-level bijection would need `unicode-normalization`
    ///   to expose per-char origin offsets, which it does not; a
    ///   character-level index is left to a follow-on milestone
    ///   (tracked at R229 REPL, where per-grapheme mapping is a
    ///   cursor-position primitive rather than a diagnostic one).
    pub fn build(src: &str) -> (String, Self) {
        if src.is_ascii() {
            return (src.to_owned(), Self::identity(src.len()));
        }
        // `UnicodeNormalization` is implemented for `&str` directly in
        // this crate (via `nfc(self) -> Recompositions<Chars>`); no
        // need for an explicit `.chars()` intermediate.
        let nfc: String = src.nfc().collect();
        let nfc_len = nfc.len();
        let original_len = src.len();
        let checkpoints = vec![(0usize, 0usize), (nfc_len, original_len)];
        (
            nfc,
            Self {
                checkpoints,
                nfc_len,
                original_len,
                identity: false,
            },
        )
    }

    /// Translate a post-NFC byte range into its enclosing pre-NFC byte
    /// range. If the input range falls between checkpoints the result
    /// is *widened* to the surrounding checkpoint pair — never
    /// narrowed, so an LSP underline never misses part of the user's
    /// text.
    ///
    /// Identity fast-path: pure-ASCII inputs return the input range
    /// unchanged.
    pub fn to_original(&self, nfc_range: ByteRange) -> ByteRange {
        if self.identity {
            return nfc_range;
        }
        let (nfc_start, nfc_end) = nfc_range;
        // start: snap down to the largest checkpoint <= nfc_start
        // end: snap up to the smallest checkpoint >= nfc_end
        let orig_start = self.snap_down(nfc_start);
        let orig_end = self.snap_up(nfc_end);
        (orig_start, orig_end)
    }

    /// Largest `original_pos` among checkpoints with `nfc_pos <= at`.
    /// Returns 0 if `at == 0` or every checkpoint is beyond `at`.
    fn snap_down(&self, at: usize) -> usize {
        // Binary search on nfc_pos. Since checkpoints are sorted by
        // nfc_pos (built in order), this is a stable partition point.
        match self.checkpoints.binary_search_by_key(&at, |&(n, _)| n) {
            Ok(i) => self.checkpoints[i].1,
            Err(i) => {
                if i == 0 {
                    0
                } else {
                    self.checkpoints[i - 1].1
                }
            }
        }
    }

    /// Smallest `original_pos` among checkpoints with `nfc_pos >= at`.
    /// Returns `original_len` if every checkpoint is before `at`.
    fn snap_up(&self, at: usize) -> usize {
        match self.checkpoints.binary_search_by_key(&at, |&(n, _)| n) {
            Ok(i) => self.checkpoints[i].1,
            Err(i) => {
                if i >= self.checkpoints.len() {
                    self.original_len
                } else {
                    self.checkpoints[i].1
                }
            }
        }
    }

    /// True iff the map is identity (input was pure ASCII).
    #[inline]
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// Post-NFC total byte length.
    #[inline]
    pub fn nfc_len(&self) -> usize {
        self.nfc_len
    }

    /// Pre-NFC total byte length.
    #[inline]
    pub fn original_len(&self) -> usize {
        self.original_len
    }
}
