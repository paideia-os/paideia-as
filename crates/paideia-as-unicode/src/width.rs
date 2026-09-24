//! UAX#11 East-Asian-Width — the column-count table the R229 REPL
//! renderer consumes to advance its cursor and lay out per-cell prompts.
//!
//! # Why this module
//!
//! The line-editor's cursor position on a monospace terminal is measured
//! in *display columns*, not code points and not grapheme clusters. A
//! CJK ideograph occupies two columns; a combining mark occupies zero; a
//! ZWJ family emoji collapses eight code points into a single two-column
//! cell. Getting cursor-advance wrong past a wide character corrupts the
//! subsequent line's visual state permanently until the user redraws —
//! the exact "the REPL swallowed my input" bug SH-D9 forbids.
//!
//! UAX#11 §5 defines six properties (`N`, `Na`, `A`, `H`, `W`, `F`); the
//! shell renderer maps them to four rendered widths:
//!
//! | UAX#11 property | Rendered columns | This crate's `Width` variant |
//! |---|---|---|
//! | `N`, `Na`, `H` | 1 (narrow) | `Narrow` |
//! | `A` | 1 or 2 depending on locale | `Ambiguous` (caller resolves) |
//! | `W`, `F` | 2 (wide/fullwidth) | `Wide` |
//! | *combining marks, ZWJ, control* | 0 | `Zero` |
//!
//! Ambiguous characters (Greek, Cyrillic, box-drawing) render as narrow
//! in a Latin locale and wide in an East-Asian locale. R229's renderer
//! will decide via terminal-info (`WCWIDTH_MODE` env var, matching
//! `bat`/`hyperfine` convention); until then, ambiguous counts as
//! `Narrow` in `str_width`.
//!
//! # ANSI escape sequences
//!
//! Terminal color / cursor-control escapes (ESC `[` params final-byte,
//! ESC `]` params BEL, and bare two-byte escapes) render as zero
//! columns. `str_width` recognizes and skips them; a caller that has
//! already stripped colors need not care. This matches the `ansi_width`
//! crate's behavior and the convention every mature terminal library
//! (`indicatif`, `crossterm`) inherits.
//!
//! # Grapheme-cluster width
//!
//! `grapheme_width` takes a single *extended-grapheme-cluster* slice
//! (typically obtained by iterating `unicode-segmentation`'s
//! `graphemes(true)`) and returns the terminal columns that cluster
//! occupies. The convention:
//!
//! * A cluster's rendered width is the width of its *base character*.
//!   Combining marks, ZWJs, spacing marks, and the extension code points
//!   that fuse into the cluster contribute zero.
//! * A family emoji `MAN ZWJ WOMAN ZWJ GIRL ZWJ BOY` is one cluster
//!   whose base (`MAN`) is Wide — the cluster is two columns, not eight.
//! * A skin-tone-modified emoji is one cluster, two columns.
//! * A CR-LF cluster is zero columns (both members are control).

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The four terminal-rendering widths per UAX#11, with `Ambiguous`
/// exposed so a locale-aware renderer can pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    /// Zero-column: combining marks (Mn/Me), zero-width joiners,
    /// zero-width space, control characters. Contributes nothing to
    /// cursor advance.
    Zero,
    /// One column: ASCII, Latin, Greek, Cyrillic (in Latin locales),
    /// half-width Katakana, most non-East-Asian scripts.
    Narrow,
    /// One or two columns depending on locale (UAX#11 `A`). Rendered as
    /// `Narrow` in Latin locales and `Wide` in East-Asian locales; the
    /// [`str_width`] convenience treats it as narrow.
    Ambiguous,
    /// Two columns: CJK ideographs (`W`), fullwidth ASCII (`F`), most
    /// emoji, and precomposed Hangul syllables.
    Wide,
}

impl Width {
    /// Column count under a Latin-locale renderer (ambiguous → 1). What
    /// `str_width` sums.
    #[inline]
    pub fn columns_latin(self) -> usize {
        match self {
            Self::Zero => 0,
            Self::Narrow | Self::Ambiguous => 1,
            Self::Wide => 2,
        }
    }

    /// Column count under an East-Asian-locale renderer (ambiguous → 2).
    #[inline]
    pub fn columns_east_asian(self) -> usize {
        match self {
            Self::Zero => 0,
            Self::Narrow => 1,
            Self::Ambiguous | Self::Wide => 2,
        }
    }
}

/// UAX#11 width of a single code point.
///
/// Backed by `unicode-width` (Servo, MIT/Apache), UCD 15.1. Uses the
/// crate's `width` (Latin-locale) *and* `width_cjk` (East-Asian) to
/// distinguish `Narrow` from `Ambiguous` — a character is `Ambiguous`
/// iff the two disagree.
///
/// Control characters, format characters (Cf including ZWJ), and
/// combining marks (Mn/Me) all return `Zero`.
pub fn width(c: char) -> Width {
    let w_latin = UnicodeWidthChar::width(c);
    let w_cjk = UnicodeWidthChar::width_cjk(c);
    match (w_latin, w_cjk) {
        (None, _) | (Some(0), _) => Width::Zero,
        (Some(1), Some(1)) => Width::Narrow,
        (Some(1), Some(2)) => Width::Ambiguous,
        (Some(2), _) => Width::Wide,
        // `unicode-width` currently never emits values other than 0/1/2;
        // any future 3+ we conservatively treat as `Wide` so cursor math
        // over-advances rather than under-advances (visually harmless,
        // versus corruption from under-advance).
        _ => Width::Wide,
    }
}

/// Column count of an entire `&str` under a Latin-locale renderer,
/// ANSI escape sequences excluded (they render as zero columns) and
/// grapheme clusters credited by their base character's width.
///
/// This is what the R229 line editor calls to reposition its cursor
/// after mutating the buffer.
pub fn str_width(input: &str) -> usize {
    let stripped = strip_ansi_escapes(input);
    let mut total = 0usize;
    for cluster in stripped.graphemes(true) {
        total += grapheme_width(cluster);
    }
    total
}

/// Column count of a single extended-grapheme-cluster slice.
///
/// The caller is responsible for having sliced `cluster` on grapheme
/// boundaries (typically via `unicode-segmentation`'s `graphemes(true)`
/// or this crate's [`crate::grapheme_boundaries`]). An input that spans
/// multiple clusters returns the width of the *first* cluster's base
/// character; that is deliberately not asserted (the fast path avoids
/// the boundary re-scan) but callers relying on a per-cluster
/// invariant should feed one cluster at a time.
pub fn grapheme_width(cluster: &str) -> usize {
    if cluster.is_empty() {
        return 0;
    }
    // ANSI escape lone-atom: a single `ESC` code point is an inert
    // control that contributes zero. Any longer ANSI sequence has
    // already been stripped by `str_width` upstream; a bare ESC
    // reaching this helper (as in unit-testing `grapheme_width` alone)
    // still returns 0.
    if cluster == "\u{001B}" {
        return 0;
    }
    // The cluster's base character determines its rendered width. Every
    // subsequent code point in the cluster is (per UAX#29 GB9..GB13)
    // either a combining mark, ZWJ, extend, spacing mark, prepend, or
    // pictographic continuation — all of which the terminal folds into
    // the base cell.
    let base = cluster.chars().next().expect("non-empty checked above");
    let base_w = width(base).columns_latin();
    // Corner case: for a decomposed cluster whose base is a control or
    // ZWJ (e.g. a bare `\u{0301}` combining acute passed as its own
    // cluster), fall back to `UnicodeWidthStr::width` of the whole
    // string — which returns 0 — so we don't accidentally credit a
    // stray combining mark as one column.
    if base_w == 0 {
        return UnicodeWidthStr::width(cluster);
    }
    base_w
}

/// Return an owned copy of `input` with every ANSI escape sequence
/// removed.
///
/// Recognized:
///
/// * **CSI** — `ESC [` parameters final-byte (final byte 0x40..=0x7E).
///   Colors and cursor-control sequences.
/// * **OSC** — `ESC ]` payload terminator (BEL 0x07 or ST `ESC \`).
///   Title-setting and hyperlink sequences.
/// * **Two-byte escapes** — `ESC` followed by any single byte outside
///   `[` and `]` (e.g. `ESC (` for character-set selection, `ESC 7`
///   save-cursor).
///
/// A lone `ESC` at end-of-input is dropped. Any malformed CSI whose
/// tail runs off the end of the input consumes the rest — a pragmatic
/// "if the escape does not terminate, neither does its column cost"
/// rule that matches `strip-ansi-escapes` upstream.
fn strip_ansi_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1B {
            if i + 1 >= bytes.len() {
                // Lone trailing ESC — drop.
                break;
            }
            match bytes[i + 1] {
                b'[' => {
                    // CSI: ESC [ params (0x30..=0x3F, then 0x20..=0x2F)
                    // final-byte (0x40..=0x7E).
                    i += 2;
                    while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                }
                b']' => {
                    // OSC: ESC ] payload (BEL 0x07 | ESC \ terminator).
                    i += 2;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1B
                            && i + 1 < bytes.len()
                            && bytes[i + 1] == b'\\'
                        {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Two-byte escape (charset select, save-cursor, etc.)
                    i += 2;
                }
            }
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    // Safety: we stripped only complete ANSI escape byte-sequences from
    // originally-valid UTF-8; every remaining byte is unchanged from
    // `input`, so the aggregate is still valid UTF-8.
    debug_assert!(std::str::from_utf8(&out).is_ok());
    // Fall back to a lossy conversion in the (impossible) case a caller
    // wedged malformed UTF-8 through: never panic in cursor math.
    String::from_utf8(out).unwrap_or_default()
}
