//! Source-text construction with encoding validation.
//!
//! Per `syntax-reference.md` §1.2 (decision SY-D1), paideia-as accepts only
//! UTF-8 input. This module implements the pre-lex validation pass:
//!
//! 1. Strip a leading `U+FEFF` BOM if present.
//! 2. Reject empty input with `E0018`.
//! 3. Validate the remaining bytes as UTF-8 (`E0001` on failure).
//! 4. Reject lone CR not followed by LF (`E0002`); `\r\n` is permitted and
//!    normalized lazily during line/column conversion.
//!
//! Diagnostics emitted here report byte offsets into the **original** byte
//! stream (BOM included). The resulting `SourceText` itself stores the
//! post-BOM content; downstream lexer offsets are relative to it.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

/// The BOM byte sequence (UTF-8 encoding of `U+FEFF`).
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Source text post-validation. Holds the BOM-stripped, UTF-8-validated
/// content and the byte length of the stripped BOM (if any).
#[derive(Debug, Clone)]
pub struct SourceText {
    content: String,
    bom_offset: u32,
}

impl SourceText {
    /// Validates `bytes` as UTF-8 (after optional BOM stripping) and produces
    /// a `SourceText`. Returns a fatal `Diagnostic` on failure.
    ///
    /// `file` is the `FileId` to embed in any emitted diagnostic span.
    /// Byte offsets in returned diagnostics refer to the **original** input
    /// (i.e., they include the BOM if one was present).
    pub fn from_bytes(file: FileId, bytes: &[u8]) -> Result<Self, Box<Diagnostic>> {
        let (post_bom, bom_offset) = if bytes.starts_with(BOM) {
            (&bytes[BOM.len()..], BOM.len() as u32)
        } else {
            (bytes, 0u32)
        };

        if post_bom.is_empty() {
            return Err(make_diag(file, e_code(18), 0, 0, "source file is empty"));
        }

        let content = match std::str::from_utf8(post_bom) {
            Ok(s) => s.to_owned(),
            Err(e) => {
                let offset_in_post_bom = e.valid_up_to() as u32;
                let bad_len = e.error_len().unwrap_or(1) as u32;
                return Err(make_diag(
                    file,
                    e_code(1),
                    bom_offset + offset_in_post_bom,
                    bad_len,
                    "invalid UTF-8 byte sequence",
                ));
            }
        };

        if let Some(cr_offset) = lone_cr(&content) {
            return Err(make_diag(
                file,
                e_code(2),
                bom_offset + cr_offset as u32,
                1,
                "lone CR (U+000D) without following LF",
            ));
        }

        Ok(SourceText {
            content,
            bom_offset,
        })
    }

    /// The validated, BOM-stripped source text.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Byte length of the BOM that was stripped, or 0 if no BOM was present.
    pub fn bom_offset(&self) -> u32 {
        self.bom_offset
    }
}

/// Returns the byte offset of the first lone CR (a `\r` not followed by `\n`),
/// or `None` if every CR is part of a CRLF pair.
fn lone_cr(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Some(i);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}

fn e_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::E, Severity::Error, n).expect("valid E code")
}

fn make_diag(
    file: FileId,
    code: DiagnosticCode,
    byte_start: u32,
    byte_len: u32,
    message: &str,
) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::error(code)
            .message(message)
            .with_span(Span::new(file, byte_start, byte_len))
            .finish(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::Category;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    #[test]
    fn plain_utf8_accepted() {
        let st = SourceText::from_bytes(file_id(), b"hello\n").unwrap();
        assert_eq!(st.content(), "hello\n");
        assert_eq!(st.bom_offset(), 0);
    }

    #[test]
    fn bom_stripped_and_offset_recorded() {
        let mut input = BOM.to_vec();
        input.extend_from_slice(b"abc\n");
        let st = SourceText::from_bytes(file_id(), &input).unwrap();
        assert_eq!(st.content(), "abc\n");
        assert_eq!(st.bom_offset(), 3);
    }

    #[test]
    fn empty_file_emits_e0018() {
        let err = SourceText::from_bytes(file_id(), b"").unwrap_err();
        assert_eq!(err.code().category(), Category::E);
        assert_eq!(err.code().number(), 18);
        let span = err.primary_span().unwrap();
        assert_eq!(span.byte_start(), 0);
    }

    #[test]
    fn bom_only_file_emits_e0018() {
        let err = SourceText::from_bytes(file_id(), BOM).unwrap_err();
        assert_eq!(err.code().number(), 18);
    }

    #[test]
    fn invalid_utf8_emits_e0001_at_byte_offset() {
        // "hello" (5 bytes) + invalid byte 0xFF.
        let input = b"hello\xFFworld";
        let err = SourceText::from_bytes(file_id(), input).unwrap_err();
        assert_eq!(err.code().number(), 1);
        assert_eq!(err.primary_span().unwrap().byte_start(), 5);
    }

    #[test]
    fn invalid_utf8_after_bom_uses_absolute_offset() {
        let mut input = BOM.to_vec();
        input.extend_from_slice(b"a\xFFb");
        let err = SourceText::from_bytes(file_id(), &input).unwrap_err();
        assert_eq!(err.code().number(), 1);
        // BOM (3) + "a" (1) = 4.
        assert_eq!(err.primary_span().unwrap().byte_start(), 4);
    }

    #[test]
    fn lone_cr_emits_e0002() {
        let err = SourceText::from_bytes(file_id(), b"abc\rdef\n").unwrap_err();
        assert_eq!(err.code().number(), 2);
        assert_eq!(err.primary_span().unwrap().byte_start(), 3);
    }

    #[test]
    fn lone_cr_at_eof_emits_e0002() {
        let err = SourceText::from_bytes(file_id(), b"abc\r").unwrap_err();
        assert_eq!(err.code().number(), 2);
        assert_eq!(err.primary_span().unwrap().byte_start(), 3);
    }

    #[test]
    fn crlf_accepted() {
        let st = SourceText::from_bytes(file_id(), b"abc\r\ndef\r\n").unwrap();
        assert_eq!(st.content(), "abc\r\ndef\r\n");
    }

    #[test]
    fn lone_cr_after_bom_uses_absolute_offset() {
        let mut input = BOM.to_vec();
        input.extend_from_slice(b"a\rb");
        let err = SourceText::from_bytes(file_id(), &input).unwrap_err();
        assert_eq!(err.code().number(), 2);
        // BOM (3) + "a" (1) = 4.
        assert_eq!(err.primary_span().unwrap().byte_start(), 4);
    }

    #[test]
    fn cr_in_middle_of_otherwise_valid_text() {
        // Lone CR followed by something other than LF.
        let err = SourceText::from_bytes(file_id(), b"hello\rworld\n").unwrap_err();
        assert_eq!(err.code().number(), 2);
        assert_eq!(err.primary_span().unwrap().byte_start(), 5);
    }
}
