//! paideia-as-unicode — R221.M1 (UTF-8 + NFC) + R221.M2 (TR#29 graphemes)
//! semantic-shell Unicode substrate.
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
//! # What this crate does NOT own
//!
//! * The TR#11 East-Asian width table + renderer (R221.M3, follow-on
//!   milestone).
//! * The context-tracking lexer that switches between pipeline /
//!   Datalog / lambda contexts (R221.M4).
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

pub mod error;
pub mod grapheme;
pub mod nfc;
pub mod utf8;

pub use error::Utf8Error;
pub use grapheme::{
    GraphemeBoundaries, grapheme_advance, grapheme_boundaries, grapheme_count, grapheme_retreat,
};
pub use nfc::{is_ascii_fast_path_eligible, is_nfc, nfc_normalize};
pub use utf8::Utf8Decoder;
