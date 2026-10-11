//! paideia-as-unicode — R221.M1 (UTF-8 + NFC) + R221.M2 (TR#29 graphemes)
//! + R221.M3 (TR#11 East-Asian width) semantic-shell Unicode substrate.
//!
//! # What this crate owns
//!
//! * [`Utf8Decoder`] — a streaming, byte-slice-driven decoder that
//!   yields `Result<char, Utf8Error>` per RFC 3629. Overlong encodings,
//!   surrogates (U+D800..=U+DFFF), and code points above U+10FFFF are
//!   rejected as `Utf8Error::Invalid` at the offending byte offset,
//!   rather than silently replaced with U+FFFD — hosted DSLs above
//!   this layer decide the recovery strategy (semantic-shell errors
//!   on malformed input at the parse boundary per SH-D9).
//!
//! * [`nfc_normalize`] — Unicode Normalization Form C (UAX#15) at the
//!   `&str` boundary. Backed by `unicode-normalization::UnicodeNormalization::nfc`,
//!   with a paideia-as-owned ASCII fast path (any `&str` whose bytes are
//!   all `< 0x80` is already NFC, and we return an owned copy without
//!   the tables ever loading). The fast path is what R220.M4
//!   `Str::eq` needs to keep the common "compare two ASCII command
//!   names" case linear in the shorter length rather than pulling the
//!   NFC combining-class table into the L1.
//!
//! * [`GraphemeBoundaries`] + [`grapheme_advance`] + [`grapheme_retreat`]
//!   — the TR#29 extended-grapheme-cluster boundary iterator R229's REPL
//!   line editor uses for cursor-position math and R228's tab-completion
//!   uses to segment argument tokens.
//!
//! * [`width`] + [`str_width`] + [`grapheme_width`] — UAX#11 East-Asian
//!   width lookup and rendered-column counts. The R229 renderer maps a
//!   buffer position to a screen column by summing `str_width` over the
//!   preceding text; ANSI escapes are stripped, wide CJK characters
//!   count as two columns, and grapheme clusters (family emoji,
//!   skin-tone modifiers) collapse to their base cell.
//!
//! * [`general_category`] + [`script`] + [`block`] (UNI-5) — the UCD
//!   property adapter. Three queries over `char`:
//!   [`GeneralCategory`] (UAX#44 §5.7, 30 variants in UCD short form),
//!   [`Script`] (UAX#24, re-exported from `unicode-script`), and
//!   [`Block`] (UCD Blocks.txt, 344 variants). Seven group predicates
//!   ([`is_letter`], [`is_mark`], [`is_number`], [`is_punctuation`],
//!   [`is_symbol`], [`is_separator`], [`is_other`]) collapse the 30
//!   General_Category variants to the top-level groups. See
//!   `src/ucd.rs` for the dependency and `#[non_exhaustive]` rationale.
//!
//! # What this crate does NOT own
//!
//! * The context-tracking lexer that switches between pipeline /
//!   Datalog / lambda contexts (R221.M4 — lives in the separate
//!   `paideia-as-shell-lex` crate, which depends on this one).
//! * The unified AST sum type (R221.M5).
//! * Source-span tracking through the NFC transform (R221.M6) — that
//!   milestone consumes this crate's decoder + normalizer but adds its
//!   own byte-range provenance map.
//!
//! # Invariants
//!
//! For any `s: &str`:
//! * `grapheme_count(&s) <= s.chars().count() <= s.len()` (bytes).
//! * `nfc_normalize(nfc_normalize(&s).as_str()) == nfc_normalize(&s)`
//!   (idempotence; UAX#15 §1.4).
//! * `grapheme_advance(s, 0)` returns `s.len()` iff `s` contains at
//!   most one grapheme cluster (or is empty).
//!
//! These invariants are asserted by the property test suite under
//! `tests/`.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod case;
pub mod error;
pub mod fingerprint;
pub mod grapheme;
pub mod ident;
pub mod nfc;
pub mod norm;
pub mod segment;
pub mod ucd;
pub mod utf8;
pub mod width;

// UNI-2 (paideia-as#1589) — Unicode case folding (CaseFolding.txt
// common + full folds), locale-aware equality, full uppercase /
// lowercase. See `src/case.rs` for the API-surface rationale.
pub use case::{
    CaseFoldLocale, eq_ignore_case_unicode, fold, fold_default, lowercase, uppercase,
};
pub use error::Utf8Error;
pub use grapheme::{
    GraphemeBoundaries, grapheme_advance, grapheme_boundaries, grapheme_count, grapheme_retreat,
};
// UNI-4 (paideia-as#1591) — UAX #31 identifier predicates + UTS #39
// identifier-security profile. See `src/ident.rs` for the NFC-stability
// extension to UAX #31, the three-level `IdentProfile`, and the
// `mixed_script` script-set helper that backs the single-script
// restriction.
pub use ident::{
    IdentError, IdentErrorKind, IdentProfile, check_ident, is_identifier, is_xid_continue,
    is_xid_start, mixed_script,
};
// Backward-compatible NFC surface — the names stay re-exported from the
// crate root so existing shell-lex / shell-ast / shell-completion
// consumers (`use paideia_as_unicode::nfc_normalize`) keep compiling.
pub use nfc::{is_ascii_fast_path_eligible, is_nfc, nfc_normalize};
// UNI-1 generalization: NFD / NFKC / NFKD + `Form` dispatcher. See
// `design/toolchain/paideia-as-unicode-spec.md` for the per-form
// contracts.
pub use norm::{
    Form, is_form, is_nfd, is_nfkc, is_nfkd, nfd_normalize, nfkc_normalize, nfkd_normalize,
    normalize,
};
// UNI-3 (paideia-as#1590) — UAX #14 line-break opportunity surface +
// `wrap_at_width` + streaming `next_line_break`. See `src/segment.rs`
// for the trimming convention, the ASCII fast path, and the grapheme-
// level forced-break fallback that keeps the function total when a
// single unbreakable run exceeds `max_width`.
pub use segment::{
    LineBreak, LineBreakOpportunity, SegmentError, SegmentErrorKind, line_break_opportunities,
    next_line_break, wrap_at_width,
};
// UNI-5 (paideia-as#1592) — UCD property adapter: General_Category /
// Script / Block + the seven top-level group predicates. See
// `src/ucd.rs` for the module rationale, the `#[non_exhaustive]`
// discipline, and the `unicode-properties` / `unicode-script` /
// `unicode-blocks` dependency choice.
pub use ucd::{
    Block, GeneralCategory, Script, block, general_category, is_letter, is_mark, is_number,
    is_other, is_punctuation, is_separator, is_symbol, script,
};
pub use utf8::Utf8Decoder;
pub use width::{
    Width, WidthMode, grapheme_width, grapheme_width_in, str_width, str_width_in, width, width_in,
};
