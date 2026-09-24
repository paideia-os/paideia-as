//! Streaming RFC 3629 UTF-8 decoder.
//!
//! The decoder is deliberately hand-rolled rather than delegated to
//! `core::str::from_utf8` for two reasons:
//!
//! 1. **Per-byte error localization.** `core::str::from_utf8` returns a
//!    single error struct at the first invalid byte; the shell's parser
//!    needs to walk forward past a malformed byte and keep decoding so
//!    a `datalog { … }` block with one bad UTF-8 byte in a string
//!    literal still produces a useful diagnostic for the rest of the
//!    block. `Utf8Decoder` returns per-`char` results so the caller
//!    picks the recovery strategy per byte.
//!
//! 2. **Iterator ergonomics.** The lexer at R221.M4 wants
//!    `impl Iterator<Item = Result<char, Utf8Error>>` so it can compose
//!    with `Peekable` and `.take_while(|c| c.is_ok())`. `core::str::Chars`
//!    only vends `char` (already validated) and has no fallible variant.
//!
//! The lookup tables (`LEAD_TABLE`) inline the RFC 3629 §4 "Well-Formed
//! UTF-8 Byte Sequences" table, which is small enough to hand-maintain
//! and is not part of the UCD.

use crate::error::{Utf8Error, Utf8ErrorKind};

/// Streaming UTF-8 decoder over a byte slice.
///
/// # Example
///
/// ```
/// use paideia_as_unicode::Utf8Decoder;
///
/// let bytes = "aé字".as_bytes();
/// let chars: Vec<_> = Utf8Decoder::new(bytes).collect::<Result<_, _>>().unwrap();
/// assert_eq!(chars, vec!['a', 'é', '字']);
/// ```
#[derive(Clone, Debug)]
pub struct Utf8Decoder<'a> {
    bytes: &'a [u8],
    /// Absolute byte offset within `bytes` of the next byte to read.
    pos: usize,
}

impl<'a> Utf8Decoder<'a> {
    /// Wrap a byte slice for streaming UTF-8 decoding.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// The absolute byte offset of the next byte the decoder will read.
    /// Useful for spans in the R221.M6 provenance-tracking layer.
    #[inline]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Whether the decoder has consumed every byte.
    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.pos >= self.bytes.len()
    }
}

/// Number of bytes in a UTF-8 sequence given its lead byte, or `None`
/// if the byte is not a legal lead. Indexed by lead byte.
///
/// * `0x00..=0x7F` → 1
/// * `0xC2..=0xDF` → 2
/// * `0xE0..=0xEF` → 3
/// * `0xF0..=0xF4` → 4
/// * everything else (continuation bytes, `0xC0`, `0xC1`, `0xF5..=0xFF`) → `None`
#[inline]
fn lead_seq_len(byte: u8) -> Option<usize> {
    match byte {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

/// Legal continuation byte range per RFC 3629 §3.
#[inline]
fn is_continuation(byte: u8) -> bool {
    (byte & 0b1100_0000) == 0b1000_0000
}

impl<'a> Iterator for Utf8Decoder<'a> {
    type Item = Result<char, Utf8Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        let lead = self.bytes[start];
        let Some(seq_len) = lead_seq_len(lead) else {
            self.pos += 1;
            return Some(Err(Utf8Error {
                offset: start,
                reason: Utf8ErrorKind::InvalidLeadByte,
            }));
        };
        if start + seq_len > self.bytes.len() {
            // Advance to the end so the decoder terminates rather than
            // looping on truncated tail bytes.
            self.pos = self.bytes.len();
            return Some(Err(Utf8Error {
                offset: start,
                reason: Utf8ErrorKind::UnexpectedEndOfInput,
            }));
        }
        // Validate continuation bytes with the RFC 3629 §4 lead-byte
        // subranges that keep the sequence out of the overlong /
        // surrogate / oversized cells.
        //
        // Encoding form: uuuuu uzzzz yyyyyy xxxxxx.
        let (cp, bad_offset) = match seq_len {
            1 => (lead as u32, None),
            2 => {
                let c1 = self.bytes[start + 1];
                if !is_continuation(c1) {
                    (0, Some(start + 1))
                } else {
                    let cp = ((lead as u32 & 0x1F) << 6) | (c1 as u32 & 0x3F);
                    (cp, None)
                }
            }
            3 => {
                let c1 = self.bytes[start + 1];
                let c2 = self.bytes[start + 2];
                // RFC 3629 §4: for lead 0xE0, c1 must be 0xA0..=0xBF
                // (rules out overlong three-byte encodings of code
                // points < U+0800). For lead 0xED, c1 must be
                // 0x80..=0x9F (rules out the D800..DFFF surrogate
                // half). Everything else: 0x80..=0xBF.
                let c1_ok = match lead {
                    0xE0 => (0xA0..=0xBF).contains(&c1),
                    0xED => (0x80..=0x9F).contains(&c1),
                    _ => is_continuation(c1),
                };
                if !c1_ok {
                    (0, Some(start + 1))
                } else if !is_continuation(c2) {
                    (0, Some(start + 2))
                } else {
                    let cp = ((lead as u32 & 0x0F) << 12)
                        | ((c1 as u32 & 0x3F) << 6)
                        | (c2 as u32 & 0x3F);
                    (cp, None)
                }
            }
            4 => {
                let c1 = self.bytes[start + 1];
                let c2 = self.bytes[start + 2];
                let c3 = self.bytes[start + 3];
                // RFC 3629 §4: for lead 0xF0, c1 must be 0x90..=0xBF
                // (rules out overlong four-byte encodings of code
                // points < U+10000). For lead 0xF4, c1 must be
                // 0x80..=0x8F (rules out code points > U+10FFFF).
                let c1_ok = match lead {
                    0xF0 => (0x90..=0xBF).contains(&c1),
                    0xF4 => (0x80..=0x8F).contains(&c1),
                    _ => is_continuation(c1),
                };
                if !c1_ok {
                    (0, Some(start + 1))
                } else if !is_continuation(c2) {
                    (0, Some(start + 2))
                } else if !is_continuation(c3) {
                    (0, Some(start + 3))
                } else {
                    let cp = ((lead as u32 & 0x07) << 18)
                        | ((c1 as u32 & 0x3F) << 12)
                        | ((c2 as u32 & 0x3F) << 6)
                        | (c3 as u32 & 0x3F);
                    (cp, None)
                }
            }
            _ => unreachable!("seq_len returned {} from lead_seq_len", seq_len),
        };
        if let Some(bad) = bad_offset {
            // Advance past the offending continuation byte so the next
            // call resynchronizes at the next candidate lead byte.
            self.pos = bad + 1;
            return Some(Err(Utf8Error {
                offset: start,
                reason: Utf8ErrorKind::InvalidContinuationByte,
            }));
        }
        self.pos = start + seq_len;
        // After the RFC 3629 §4 lead-byte subranges, the only remaining
        // impossibility is a code point > U+10FFFF, which the
        // 0xF0/0xF4 gating above already rules out — but the sanity
        // check stays cheap and defensive.
        if cp > 0x10_FFFF {
            return Some(Err(Utf8Error {
                offset: start,
                reason: Utf8ErrorKind::OutOfRange,
            }));
        }
        // char::from_u32 also rejects surrogates; the §4 subranges
        // already prevent them, but the branch is free.
        match char::from_u32(cp) {
            Some(c) => Some(Ok(c)),
            None => Some(Err(Utf8Error {
                offset: start,
                reason: Utf8ErrorKind::Surrogate,
            })),
        }
    }
}
