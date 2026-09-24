//! UTF-8 decode-error taxonomy.
//!
//! We deliberately expose the offending byte offset and a discriminated
//! reason (rather than a single `Invalid` marker) so hosted DSLs above
//! this crate — the shell parser at R221.M4/M5, the R227 `.pds` loader,
//! the R228 completion matcher — can produce user-facing diagnostics
//! that point at the exact byte range the way `paideia-as-diagnostics`
//! already does for source spans. The variants match the four failure
//! modes RFC 3629 §3 enumerates.

use core::fmt;

/// A UTF-8 decoding failure, tagged with the byte offset at which the
/// decoder detected the problem and a discriminated reason.
///
/// The `offset` is measured from the start of the *input slice passed
/// to the decoder call that produced this error*, not from any larger
/// enclosing document — callers that stream multiple slices are
/// responsible for accumulating the base offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf8Error {
    /// Byte offset (from the start of the current input slice) at which
    /// the offending sequence begins.
    pub offset: usize,
    /// The specific way the sequence was ill-formed.
    pub reason: Utf8ErrorKind,
}

/// The four RFC 3629 §3 ways a UTF-8 sequence can be ill-formed, plus
/// truncation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Utf8ErrorKind {
    /// Lead byte is not one of the legal RFC 3629 lead bytes
    /// (e.g. a continuation byte appearing where a lead byte was
    /// expected, or bytes 0xC0/0xC1/0xF5..=0xFF which no legal
    /// UTF-8 sequence ever contains).
    InvalidLeadByte,
    /// A continuation byte was not in the range 0x80..=0xBF.
    InvalidContinuationByte,
    /// The sequence is shorter than the lead byte declared (input ran
    /// out mid-sequence). A streaming caller can retry after appending
    /// more bytes; a whole-slice caller should treat this as a hard
    /// error.
    UnexpectedEndOfInput,
    /// The decoded sequence is an "overlong" encoding — a code point
    /// encoded with more bytes than necessary (e.g. U+0000 encoded as
    /// 0xC0 0x80 rather than 0x00). RFC 3629 §3 forbids these
    /// unconditionally.
    Overlong,
    /// The decoded code point lies in the surrogate range
    /// U+D800..=U+DFFF, which UTF-8 forbids by RFC 3629 §3.
    Surrogate,
    /// The decoded code point exceeds U+10FFFF, the Unicode code space
    /// upper bound.
    OutOfRange,
}

impl fmt::Display for Utf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "utf-8 decode error at byte {}: {}", self.offset, self.reason)
    }
}

impl fmt::Display for Utf8ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::InvalidLeadByte => "invalid UTF-8 lead byte",
            Self::InvalidContinuationByte => "invalid UTF-8 continuation byte",
            Self::UnexpectedEndOfInput => "unexpected end of input mid-sequence",
            Self::Overlong => "overlong UTF-8 encoding",
            Self::Surrogate => "surrogate code point (U+D800..=U+DFFF) forbidden by RFC 3629",
            Self::OutOfRange => "code point above U+10FFFF",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for Utf8Error {}
