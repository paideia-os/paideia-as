//! Compile-time embed-literal parsers (`@guid`, `@include_bytes`,
//! `@include_str`, `@include_bytes_as_str`).
//!
//! Extracted 2026-08-11 from `parse_primary.rs` (God-file refactor).
//! These parsers all bind an `@`-prefixed keyword to a byte or string
//! literal that's baked into the AST at compile time via the parser's
//! filesystem accessor (`resolve_and_read_embed`). They share a common
//! diagnostic taxonomy (P0278/P0279/P0280/P0281) and pull in
//! `std::fs` + `std::path::Path` — dependencies unrelated to plain
//! expression parsing.
//!
//! All four Parser methods here are `pub(super)` so `parse_primary`'s
//! `parse_inline_directive` dispatcher can call them; nothing outside
//! this module tree references them. `parse_guid` + `GuidError` are
//! re-exported at `super::mod` with `pub(super)` visibility so
//! test-module glob imports pick them up unchanged.

use std::fs;
use std::path::Path;

use paideia_as_ast::{ExprData, NodeKind};
use paideia_as_diagnostics::{Diagnostic, Span};
use paideia_as_lexer::{TokenKind, extract_string_content};

use super::p_code;
use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {

    /// Parse a GUID literal: `@guid("XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX")`.
    ///
    /// Algorithm:
    /// 1. Expect `(`.
    /// 2. Expect a string literal (the GUID string).
    /// 3. Extract the string content.
    /// 4. Parse the GUID string into 16 bytes using parse_guid helper.
    /// 5. On error, emit P0278 diagnostic.
    /// 6. On success, allocate ExprInlineBytes node.
    pub(super) fn parse_guid_literal(&mut self, at_span: Span) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Expect `(`
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(278))
                .message("expected `(` after @guid".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Expect a string literal
        if !self.at(TokenKind::StringLit) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(278))
                .message("expected string literal in @guid(...)".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let source = self.source();
        let start = str_tok.span.byte_start() as usize;
        let end = (str_tok.span.byte_start() + str_tok.span.byte_len()) as usize;
        let token_text = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Extract the string content (handle raw string prefixes if any)
        let is_raw = token_text.starts_with('r');
        let guid_string = match extract_string_content(token_text, 0, is_raw, false) {
            Ok(content) => content,
            Err(_) => {
                let diag = Diagnostic::error(p_code(278))
                    .message("invalid string literal in @guid".to_string())
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Parse the GUID string into 16 bytes
        let guid_bytes = match parse_guid(&guid_string) {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                let diag = Diagnostic::error(p_code(278))
                    .message(format!("malformed GUID literal: {}", e.message()))
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Expect `)`
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        let rparen_tok = self.expect(TokenKind::RParen)?;
        let rparen_span = rparen_tok.span;

        // Compute span from `@` through closing `)`
        let span = Span::new(
            at_span.file(),
            at_span.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - at_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprInlineBytes,
            span,
            ExprData::InlineBytes(guid_bytes),
        ))
    }

    /// Parse an include_bytes literal: `@include_bytes("path/to/file.bin")`.
    ///
    /// Algorithm:
    /// 1. Expect `(`.
    /// 2. Expect a string literal (the file path).
    /// 3. Extract the string content.
    /// 4. Resolve the path against source_dir (fall back to CWD if None).
    /// 5. Read the file and validate size.
    /// 6. On error, emit P0279 or P0280 diagnostic.
    /// 7. On success, allocate ExprInlineBytes node.
    pub(super) fn parse_include_bytes_literal(&mut self, at_span: Span) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Expect `(`
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(279))
                .message("expected `(` after @include_bytes".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Expect a string literal
        if !self.at(TokenKind::StringLit) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(279))
                .message("expected string literal in @include_bytes(...)".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let source = self.source();
        let start = str_tok.span.byte_start() as usize;
        let end = (str_tok.span.byte_start() + str_tok.span.byte_len()) as usize;
        let token_text = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Extract the string content (handle raw string prefixes if any)
        let is_raw = token_text.starts_with('r');
        let raw_path = match extract_string_content(token_text, 0, is_raw, false) {
            Ok(content) => content,
            Err(_) => {
                let diag = Diagnostic::error(p_code(279))
                    .message("invalid string literal in @include_bytes".to_string())
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Resolve and read the embed file
        let bytes = match self.resolve_and_read_embed(&raw_path, str_tok.span) {
            Ok(b) => b,
            Err(_) => return Err(ParseError),
        };

        // Expect `)`
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        let rparen_tok = self.expect(TokenKind::RParen)?;
        let rparen_span = rparen_tok.span;

        // Compute span from `@` through closing `)`
        let span = Span::new(
            at_span.file(),
            at_span.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - at_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprInlineBytes,
            span,
            ExprData::InlineBytes(bytes),
        ))
    }

    /// Parse an include_str or include_bytes_as_str literal.
    ///
    /// Algorithm:
    /// 1. Expect `(`.
    /// 2. Expect a string literal (the file path).
    /// 3. Extract the string content.
    /// 4. Resolve the path against source_dir (fall back to CWD if None).
    /// 5. Read the file.
    /// 6. If `checked=true`: validate UTF-8 and emit P0281 on error.
    /// 7. If `checked=false`: accept invalid UTF-8 silently.
    /// 8. On success, allocate ExprInlineStr node.
    pub(super) fn parse_include_str_literal(&mut self, at_span: Span, checked: bool) -> Result<paideia_as_ast::NodeId, ParseError> {
        // Expect `(`
        if !self.at(TokenKind::LParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(279))
                .message(format!("expected `(` after @{}", if checked { "include_str" } else { "include_bytes_as_str" }))
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let lparen_span = self.expect(TokenKind::LParen)?.span;

        // Expect a string literal
        if !self.at(TokenKind::StringLit) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };
            let diag = Diagnostic::error(p_code(279))
                .message(format!("expected string literal in @{}(...)", if checked { "include_str" } else { "include_bytes_as_str" }))
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let source = self.source();
        let start = str_tok.span.byte_start() as usize;
        let end = (str_tok.span.byte_start() + str_tok.span.byte_len()) as usize;
        let token_text = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        // Extract the string content (handle raw string prefixes if any)
        let is_raw = token_text.starts_with('r');
        let raw_path = match extract_string_content(token_text, 0, is_raw, false) {
            Ok(content) => content,
            Err(_) => {
                let diag = Diagnostic::error(p_code(279))
                    .message(format!("invalid string literal in @{}", if checked { "include_str" } else { "include_bytes_as_str" }))
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Resolve and read the embed file
        let bytes = match self.resolve_and_read_embed(&raw_path, str_tok.span) {
            Ok(b) => b,
            Err(_) => return Err(ParseError),
        };

        // If checked, validate UTF-8
        if checked {
            if let Err(e) = std::str::from_utf8(&bytes) {
                let msg = format!("file is not valid UTF-8: invalid byte sequence at byte {}", e.valid_up_to());
                let diag = Diagnostic::error(p_code(281))
                    .message(msg)
                    .with_span(str_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        }

        // Expect `)`
        if !self.at(TokenKind::RParen) {
            return self.error_mismatched_delimiter(lparen_span);
        }
        let rparen_tok = self.expect(TokenKind::RParen)?;
        let rparen_span = rparen_tok.span;

        // Compute span from `@` through closing `)`
        let span = Span::new(
            at_span.file(),
            at_span.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - at_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprInlineStr,
            span,
            ExprData::InlineStr(bytes),
        ))
    }

    /// Resolve a file path and read its contents into a byte vector.
    ///
    /// Performs the following checks:
    /// - Rejects empty paths (P0279)
    /// - Rejects absolute paths (P0279)
    /// - Rejects paths that don't exist (P0279)
    /// - Rejects paths that aren't regular files (P0279)
    /// - Rejects files > 16 MiB (P0280)
    ///
    /// Returns the file contents as a Vec<u8> on success, or an error after
    /// emitting a diagnostic with span anchored on str_span.
    fn resolve_and_read_embed(&mut self, raw_path: &str, str_span: Span) -> Result<Vec<u8>, ParseError> {
        // Use test override if set (test mode), otherwise use 16 MiB limit
        let max_embed_bytes = self.test_max_embed_bytes.unwrap_or(16 << 20);

        // Reject empty path
        if raw_path.is_empty() {
            let diag = Diagnostic::error(p_code(279))
                .message("empty path in @include_bytes".to_string())
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Reject absolute paths
        if Path::new(raw_path).is_absolute() {
            let diag = Diagnostic::error(p_code(279))
                .message("absolute paths not allowed in @include_bytes; use relative paths only".to_string())
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Resolve the path
        let resolved_path = if let Some(source_dir) = self.source_dir() {
            source_dir.join(raw_path)
        } else {
            // Fall back to CWD (for tests)
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(raw_path)
        };

        // Check metadata: existence, is_file, size
        let metadata = match fs::metadata(&resolved_path) {
            Ok(m) => m,
            Err(e) => {
                let msg = match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        format!("file not found: {}", raw_path)
                    }
                    std::io::ErrorKind::PermissionDenied => {
                        format!("permission denied reading file: {}", raw_path)
                    }
                    _ => {
                        format!("cannot read file: {} ({})", raw_path, e)
                    }
                };
                let diag = Diagnostic::error(p_code(279))
                    .message(msg)
                    .with_span(str_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Check that it's a regular file
        if !metadata.is_file() {
            let diag = Diagnostic::error(p_code(279))
                .message(format!("not a regular file: {}", raw_path))
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Check file size BEFORE reading (reject over limit without allocating)
        let file_size = metadata.len();
        if file_size > max_embed_bytes {
            let diag = Diagnostic::error(p_code(280))
                .message(format!(
                    "file too large for @include_bytes: {} bytes exceeds 16 MiB limit",
                    file_size
                ))
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Read the file
        match fs::read(&resolved_path) {
            Ok(bytes) => Ok(bytes),
            Err(e) => {
                let diag = Diagnostic::error(p_code(279))
                    .message(format!("failed to read file {}: {}", raw_path, e))
                    .with_span(str_span)
                    .finish();
                self.emit_diagnostic(diag);
                Err(ParseError)
            }
        }
    }
}

/// Error type for GUID parsing.
#[derive(Debug, Clone, Copy)]
pub(super) enum GuidError {
    /// GUID string is not exactly 36 characters.
    WrongLength,
    /// Dashes are in wrong positions (must be at 8, 13, 18, 23).
    MalformedDashes,
    /// Non-hexadecimal character encountered outside dash positions.
    NonHex,
}

impl GuidError {
    fn message(self) -> &'static str {
        match self {
            GuidError::WrongLength => "GUID must be exactly 36 characters (format: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX)",
            GuidError::MalformedDashes => "dashes must be at positions 8, 13, 18, 23",
            GuidError::NonHex => "all characters except dashes must be hexadecimal (0-9, a-f, A-F)",
        }
    }
}

/// Parse a GUID string in the format "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
/// into 16 bytes in UEFI mixed-endian byte order.
///
/// UEFI mixed-endian format:
/// - Data1 (0-3): u32 little-endian
/// - Data2 (4-5): u16 little-endian
/// - Data3 (6-7): u16 little-endian
/// - Data4 (8-15): 8 bytes in big-endian (raw order)
///
/// Example: "12345678-1234-1234-1234-123456789abc" produces
/// [0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x12, 0x34, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
pub(super) fn parse_guid(text: &str) -> Result<[u8; 16], GuidError> {
    // Length must be exactly 36
    if text.len() != 36 {
        return Err(GuidError::WrongLength);
    }

    // Verify dashes are at positions 8, 13, 18, 23
    for (i, c) in text.chars().enumerate() {
        let expect_dash = matches!(i, 8 | 13 | 18 | 23);
        if expect_dash && c != '-' {
            return Err(GuidError::MalformedDashes);
        }
        if !expect_dash && !c.is_ascii_hexdigit() {
            return Err(GuidError::NonHex);
        }
    }

    // Parse subfields
    let data1 = u32::from_str_radix(&text[0..8], 16).map_err(|_| GuidError::NonHex)?;
    let data2 = u16::from_str_radix(&text[9..13], 16).map_err(|_| GuidError::NonHex)?;
    let data3 = u16::from_str_radix(&text[14..18], 16).map_err(|_| GuidError::NonHex)?;

    // UEFI mixed-endian layout
    let mut out = [0u8; 16];

    // Data1 as little-endian u32
    out[0..4].copy_from_slice(&data1.to_le_bytes());

    // Data2 as little-endian u16
    out[4..6].copy_from_slice(&data2.to_le_bytes());

    // Data3 as little-endian u16
    out[6..8].copy_from_slice(&data3.to_le_bytes());

    // Data4: first part "xxxx" at positions 19-23 (2 bytes)
    for i in 0..2 {
        out[8 + i] = u8::from_str_radix(&text[19 + i * 2..19 + i * 2 + 2], 16)
            .map_err(|_| GuidError::NonHex)?;
    }

    // Data4: second part "xxxxxxxxxxxx" at positions 24-36 (6 bytes)
    for i in 0..6 {
        out[10 + i] = u8::from_str_radix(&text[24 + i * 2..24 + i * 2 + 2], 16)
            .map_err(|_| GuidError::NonHex)?;
    }

    Ok(out)
}
