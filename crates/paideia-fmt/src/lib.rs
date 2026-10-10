//! paideia-fmt: source code formatter for paideia-as.
//!
//! # Top-level entry
//!
//! * [`format`] — the Phase-2-FMT entry: lex → parse → attach trivia →
//!   AST walk → Doc layout. Returns `Result<String, FormatError>` and
//!   falls back to **byte-identical** verbatim output on any pre-parse
//!   or parse failure. See the fallback contract in [`format`]'s doc
//!   comment.
//! * [`format_whitespace`] — the Phase-2-m8-010 legacy entry: whitespace
//!   normalization (trailing whitespace, blank-line cap, tab/space,
//!   ASCII↔Unicode glyphs). Kept as the current public contract for
//!   `paideia-as fmt`, the LSP, and the stand-alone CLI until the
//!   FMT-4..8 per-kind formatters land and [`format`] produces real
//!   structural output.
//!
//! # Pipeline (FMT-1..FMT-10)
//!
//! | module                | sprint    | status (2026-10-10) |
//! |-----------------------|-----------|---------------------|
//! | [`cst`]               | FMT-1a    | landed              |
//! | [`attach`]            | FMT-1b    | landed              |
//! | [`doc`] + [`layout`]  | FMT-2     | landed              |
//! | [`walker`]            | FMT-3     | this ticket         |
//! | items                 | FMT-4a/4b | after this ticket   |
//! | expressions           | FMT-5a/5b | after this ticket   |
//! | types                 | FMT-6     | after this ticket   |
//! | patterns              | FMT-7     | after this ticket   |
//! | attributes            | FMT-8     | after this ticket   |
//! | doc-comment pass      | FMT-9     | after FMT-4..8      |
//! | width policy          | FMT-10    | after FMT-9         |

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod attach;
pub mod cst;
pub mod doc;
pub mod format_expr;
pub mod format_item;
pub mod format_type;
pub mod layout;
pub mod walker;

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{DiagnosticSink, FileId, VecSink};
use paideia_as_lexer::SourceText;

/// Format options for the paideia-as formatter.
#[derive(Clone, Debug)]
pub struct FormatOptions {
    /// Number of spaces per tab (default 4).
    pub tab_width: usize,
    /// Use tabs instead of spaces (default false).
    pub use_tabs: bool,
    /// Maximum consecutive blank lines (default 2).
    pub max_blank_lines: usize,
    /// Glyph normalization style.
    pub glyph_style: GlyphStyle,
    /// Maximum column width before a group must break (FMT-2 default
    /// 100 per style-spec paideia-as#1561). Forwarded into
    /// [`layout::FormatOptions`] by the top-level [`format`] entry.
    pub max_width: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            tab_width: 4,
            use_tabs: false,
            max_blank_lines: 2,
            glyph_style: GlyphStyle::Preserve,
            max_width: 100,
        }
    }
}

/// Glyph normalization style.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum GlyphStyle {
    /// Leave glyphs as-is (default).
    #[default]
    Preserve,
    /// Convert Unicode → ASCII.
    Ascii,
    /// Convert ASCII → Unicode.
    Unicode,
}

/// Error type for the Phase-2-FMT [`format`] entry.
///
/// FMT-3 ships this as a stable *placeholder* so downstream callers can
/// learn to pattern-match on it; today **no code path returns `Err`**
/// — every failure (empty source, parse error, invalid UTF-8) routes
/// through the byte-identical fallback and returns `Ok(source)`. FMT-9
/// and FMT-10 will populate it with real variants as diagnostic
/// propagation lands.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum FormatError {
    /// Reserved placeholder — never returned by FMT-3. Present so the
    /// `Result` type carries a meaningful `Err` branch (callers write
    /// `match` rather than `.unwrap()`). FMT-9 / FMT-10 will add real
    /// variants as diagnostic propagation lands.
    Reserved,
}

/// ASCII ↔ Unicode glyph pairs.
const GLYPHS: &[(&str, &str)] = &[
    ("->", "→"),
    ("=>", "⇒"),
    ("/=", "≠"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("\\\\", "λ"),
];

/// Phase-2-FMT entry — lex → parse → trivia → AST walk → Doc layout.
///
/// # Pipeline
///
/// 1. Build a [`SourceText`]. On construction failure (invalid UTF-8,
///    lone CR, empty source) the function returns `Ok(source.to_string())`
///    — the fallback contract covers *every* pre-parse failure, not just
///    parse errors. The lexer's diagnostic is swallowed into a local
///    `VecSink` and discarded (FMT-9 will surface it through a real
///    sink).
/// 2. Lex into a `Vec<Token>` and drive the parser's
///    [`parse_source_file`](paideia_as_parser::Parser::parse_source_file).
///    On parse failure *or* parse-recovered-with-errors, return
///    `Ok(source.to_string())` (byte-identical fallback).
/// 3. Build the FMT-1a [`TriviaStream`](crate::cst::TriviaStream) and
///    FMT-1b [`TriviaAttachmentMap`](crate::attach::TriviaAttachmentMap).
/// 4. Pack a [`walker::FormatContext`] and call
///    [`walker::format_module`] to produce a [`doc::Doc`].
/// 5. Render the Doc via [`layout::render`].
///
/// # Fallback contract — byte-identical today
///
/// Every per-kind formatter in [`walker`] today returns
/// `Doc::text(source[node.span])`, and `format_module` returns
/// `Doc::text(source.content())`. The layout renderer does not scan
/// text for newlines, so the string passes through verbatim. **This
/// function therefore returns its input byte-for-byte on every
/// successfully-parsed source as of FMT-3.** Callers that want the
/// legacy whitespace-normalization behaviour should call
/// [`format_whitespace`].
///
/// # Why `Result` when no `Err` is returned
///
/// The signature is intentionally fallible so FMT-9/FMT-10 can start
/// surfacing non-fatal format errors (e.g., overflow of the width
/// policy, or a sink that elected to bail) without a signature break.
pub fn format(source: &str, opts: &FormatOptions) -> Result<String, FormatError> {
    // Fast-path 1: empty input yields empty output. SourceText rejects
    // empty bytes, so we would otherwise fall straight into the
    // fallback — short-circuiting here avoids the lex+parse spend.
    if source.is_empty() {
        return Ok(String::new());
    }

    let file = match FileId::new(1) {
        Some(f) => f,
        // Unreachable (1 is non-zero), but keeps the match total.
        None => return Ok(source.to_string()),
    };

    // Fast-path 2: SourceText validation failure → fallback verbatim.
    let source_text = match SourceText::from_bytes(file, source.as_bytes()) {
        Ok(s) => s,
        Err(_diag) => return Ok(source.to_string()),
    };

    // Lex (discard diagnostics for FMT-3; FMT-9 wires a real sink).
    let mut lex_sink = VecSink::new();
    let mut lexer = paideia_as_lexer::Lexer::new(file, &source_text);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    if lex_sink.error_count() > 0 {
        return Ok(source.to_string());
    }

    // Parse into an arena.
    let mut arena = AstArena::new();
    let mut parse_sink = VecSink::new();
    let root_id = {
        let mut parser = paideia_as_parser::Parser::new(
            &tokens,
            source_text.content(),
            file,
            &mut arena,
            &mut parse_sink,
        );
        parser.parse_source_file().ok()
    };
    // Fallback on either Err or recovered-with-errors — both surface as
    // non-zero error_count on `parse_sink`.
    let root_id = match root_id {
        Some(id) if parse_sink.error_count() == 0 => id,
        _ => return Ok(source.to_string()),
    };

    // FMT-1a + FMT-1b: trivia stream + attachment map.
    let mut cst_sink = VecSink::new();
    let stream = cst::tokenize_with_trivia(file, &source_text, &mut cst_sink);
    let trivia_map = attach::attach_trivia(&stream, &arena, root_id);

    // FMT-3 walk + FMT-2 render.
    let ctx = walker::FormatContext::new(&source_text, &stream, &arena, &trivia_map, opts);
    let doc_tree = walker::format_module(&ctx, root_id);
    let layout_opts = layout::FormatOptions {
        max_width: opts.max_width,
    };
    Ok(layout::render(&doc_tree, &layout_opts))
}

/// Legacy Phase-2-m8-010 whitespace-only formatter.
///
/// - Normalises trailing whitespace (strips).
/// - Normalises tab-vs-space (configurable; default 4 spaces).
/// - Normalises blank-line runs (max 2 consecutive blank lines).
/// - Converts ASCII↔Unicode glyphs per [`GlyphStyle`]:
///   `->` ↔ `→`, `=>` ↔ `⇒`, `\\` ↔ `λ`, `forall` ↔ `∀`, etc.
///
/// Kept as the current public contract for `paideia-as fmt`, the LSP,
/// and the stand-alone CLI binary until [`format`] produces real
/// structural output. FMT-10 will migrate those callers across and
/// deprecate this function.
pub fn format_whitespace(source: &str, opts: &FormatOptions) -> String {
    // First: Strip trailing whitespace from each line and collect lines.
    let mut lines: Vec<String> = source
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();

    // Second: Normalize tabs/spaces.
    // Convert all tabs to spaces first.
    for line in &mut lines {
        *line = line.replace("\t", &" ".repeat(opts.tab_width));
    }

    // If using tabs, convert leading spaces back to tabs.
    if opts.use_tabs {
        for line in &mut lines {
            let trimmed = line.trim_start();
            let leading = line.len() - trimmed.len();
            let tab_count = leading / opts.tab_width;
            let remainder = leading % opts.tab_width;
            *line = format!(
                "{}{}{}",
                "\t".repeat(tab_count),
                " ".repeat(remainder),
                trimmed
            );
        }
    }

    // Third: Normalize blank line runs (cap at max_blank_lines).
    let mut normalized = Vec::new();
    let mut blank_count = 0;
    for line in lines {
        if line.trim().is_empty() {
            if blank_count < opts.max_blank_lines {
                normalized.push(line);
                blank_count += 1;
            }
        } else {
            blank_count = 0;
            normalized.push(line);
        }
    }

    // Fourth: Apply glyph normalization if requested.
    let result = match opts.glyph_style {
        GlyphStyle::Preserve => normalized.join("\n"),
        GlyphStyle::Ascii => unicode_to_ascii(&normalized.join("\n")),
        GlyphStyle::Unicode => ascii_to_unicode(&normalized.join("\n")),
    };

    // Finally: Ensure trailing newline.
    if result.is_empty() {
        result
    } else {
        result + "\n"
    }
}

/// Convert ASCII glyphs to Unicode equivalents.
pub fn ascii_to_unicode(source: &str) -> String {
    let mut result = source.to_string();
    for (ascii, unicode) in GLYPHS {
        result = result.replace(ascii, unicode);
    }
    result
}

/// Convert Unicode glyphs to ASCII equivalents.
pub fn unicode_to_ascii(source: &str) -> String {
    let mut result = source.to_string();
    for (ascii, unicode) in GLYPHS {
        result = result.replace(unicode, ascii);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Legacy whitespace formatter tests (unchanged) ------------------

    #[test]
    fn ascii_to_unicode_replaces_arrow() {
        let input = "fn foo() -> int";
        let expected = "fn foo() → int";
        assert_eq!(ascii_to_unicode(input), expected);
    }

    #[test]
    fn ascii_to_unicode_replaces_lambda() {
        let input = "let f = \\\\x. x + 1";
        let expected = "let f = λx. x + 1";
        assert_eq!(ascii_to_unicode(input), expected);
    }

    #[test]
    fn unicode_to_ascii_replaces_arrow() {
        let input = "fn foo() → int";
        let expected = "fn foo() -> int";
        assert_eq!(unicode_to_ascii(input), expected);
    }

    #[test]
    fn unicode_to_ascii_replaces_lambda() {
        let input = "let f = λx. x + 1";
        let expected = "let f = \\\\x. x + 1";
        assert_eq!(unicode_to_ascii(input), expected);
    }

    #[test]
    fn format_whitespace_strips_trailing_whitespace() {
        let input = "let x = 1   \nlet y = 2  ";
        let opts = FormatOptions::default();
        let result = format_whitespace(input, &opts);
        assert!(result.lines().all(|line| !line.ends_with(' ')));
    }

    #[test]
    fn format_whitespace_caps_blank_line_runs_at_max_blank_lines() {
        let input = "line1\n\n\n\n\nline2";
        let opts = FormatOptions {
            max_blank_lines: 2,
            ..Default::default()
        };
        let result = format_whitespace(input, &opts);
        let blank_count = result
            .lines()
            .zip(result.lines().skip(1))
            .filter(|(a, b)| a.trim().is_empty() && b.trim().is_empty())
            .count();
        assert!(blank_count <= 2);
    }

    #[test]
    fn format_whitespace_no_op_on_already_formatted() {
        let input = "let x = 1\nlet y = 2\n";
        let opts = FormatOptions::default();
        let result = format_whitespace(input, &opts);
        assert_eq!(result, input);
    }

    #[test]
    fn format_whitespace_idempotent() {
        let input = "let x = 1   \n\n\n\n\nlet y = 2  ";
        let opts = FormatOptions::default();
        let result1 = format_whitespace(input, &opts);
        let result2 = format_whitespace(&result1, &opts);
        assert_eq!(result1, result2);
    }

    #[test]
    fn format_whitespace_with_glyph_conversion() {
        let input = "fn foo() -> int";
        let opts = FormatOptions {
            glyph_style: GlyphStyle::Unicode,
            ..Default::default()
        };
        let result = format_whitespace(input, &opts);
        assert_eq!(result, "fn foo() → int\n");
    }

    // --- FMT-3 top-level pipeline tests ---------------------------------

    #[test]
    fn format_empty_source_yields_empty_output() {
        let opts = FormatOptions::default();
        assert_eq!(format("", &opts).unwrap(), "");
    }

    #[test]
    fn format_single_let_is_byte_identical() {
        let src = "pub let x : u64 = 1";
        let opts = FormatOptions::default();
        let got = format(src, &opts).unwrap();
        assert_eq!(got, src);
    }

    #[test]
    fn format_parse_error_returns_verbatim() {
        // "let 123" is a syntax error (number where identifier
        // expected). The fallback contract requires that we return the
        // source bytes verbatim.
        let src = "let 123";
        let opts = FormatOptions::default();
        let got = format(src, &opts).unwrap();
        assert_eq!(got, src);
    }

    #[test]
    fn format_lex_error_returns_verbatim() {
        // Lone CR hits SourceText's lone-CR validator; the fast-path
        // routes to the verbatim fallback instead of propagating the
        // diagnostic.
        let src = "let x\r = 1\n";
        let opts = FormatOptions::default();
        let got = format(src, &opts).unwrap();
        assert_eq!(got, src);
    }

    #[test]
    fn format_preserves_leading_line_comment() {
        // The whole-source fallback preserves file-level trivia by
        // construction: Doc::text(source.content()).
        let src = "// doc\nlet x = 1\n";
        let opts = FormatOptions::default();
        let got = format(src, &opts).unwrap();
        assert_eq!(got, src);
    }

    #[test]
    fn format_preserves_trailing_line_comment() {
        let src = "let x = 1 // trailing\n";
        let opts = FormatOptions::default();
        let got = format(src, &opts).unwrap();
        assert_eq!(got, src);
    }

    #[test]
    fn format_is_idempotent_on_simple_let() {
        let src = "pub let x : u64 = 1\n";
        let opts = FormatOptions::default();
        let once = format(src, &opts).unwrap();
        let twice = format(&once, &opts).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn format_is_idempotent_on_commented_source() {
        let src = "// header\n// line\nlet x = 1\n";
        let opts = FormatOptions::default();
        let once = format(src, &opts).unwrap();
        let twice = format(&once, &opts).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn format_is_idempotent_on_module() {
        // `module Name = structure { ... }` is the paideia-as module-decl
        // production. Exercises the full lex/parse/walk/render path on
        // a multi-item file, confirming byte-identity holds there too.
        let src = "module M = structure { let x = 1 let y = 2 }\n";
        let opts = FormatOptions::default();
        let once = format(src, &opts).unwrap();
        let twice = format(&once, &opts).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn format_error_reserved_variant_exists() {
        // Compile-only guard: `FormatError::Reserved` is the sole
        // placeholder variant today. FMT-9 / FMT-10 will add real
        // variants; this test keeps the symbol live so a breaking
        // rename of `Reserved` surfaces here.
        let err: FormatError = FormatError::Reserved;
        match err {
            FormatError::Reserved => {}
        }
    }
}
