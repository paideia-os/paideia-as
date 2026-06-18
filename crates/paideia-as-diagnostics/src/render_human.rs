#![forbid(unsafe_code)]

use crate::{Catalog, Diagnostic, Severity, SourceMap, Span};
use std::borrow::Cow;

/// Renders diagnostics in human-readable format with ANSI color support.
///
/// Produces output similar to Rust compiler errors, with source excerpts,
/// caret underlining, and structured source location information.
pub struct HumanRenderer<'a> {
    source_map: &'a SourceMap,
    catalog: Option<&'a Catalog>,
    color: bool,
}

impl<'a> HumanRenderer<'a> {
    /// Creates a new renderer without catalog support.
    ///
    /// Header text will fall back to the diagnostic's message.
    #[must_use]
    pub fn new(source_map: &'a SourceMap, color: bool) -> Self {
        Self {
            source_map,
            catalog: None,
            color,
        }
    }

    /// Creates a new renderer with catalog support.
    ///
    /// If a diagnostic's code is in the catalog, the header text uses the catalog's `brief` field.
    /// Otherwise, falls back to the diagnostic's message.
    #[must_use]
    pub fn with_catalog(source_map: &'a SourceMap, color: bool, catalog: &'a Catalog) -> Self {
        Self {
            source_map,
            catalog: Some(catalog),
            color,
        }
    }

    /// Renders a diagnostic into a formatted string.
    ///
    /// Returns a single-allocation String containing the full rendered output.
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let mut output = String::new();

        // Get header text: prefer catalog brief, fall back to diagnostic message.
        let header = if let Some(cat) = self.catalog {
            cat.lookup_code(diagnostic.code())
                .map(|entry| entry.brief.as_str())
                .unwrap_or(diagnostic.message())
        } else {
            diagnostic.message()
        };

        // Render severity + code + header.
        let code_str = diagnostic.code().to_string();
        let sev_text = self.severity_word(diagnostic.severity());
        let sev_colored = self.paint(self.severity_ansi_code(diagnostic.severity()), sev_text);

        output.push_str(&format!(
            "{}[{}]: {}",
            sev_colored,
            code_str,
            self.paint("\x1b[1m", header)
        ));
        output.push('\n');

        // If no primary span, render summary-only form.
        if diagnostic.primary_span().is_none() {
            return output;
        }

        output.push('\n');

        // Render primary span.
        if let Some(primary_span) = diagnostic.primary_span() {
            self.render_span_block(&mut output, primary_span, true, None);
        }

        // Render secondary spans.
        for secondary in diagnostic.secondary_spans() {
            output.push('\n');
            self.render_span_block(&mut output, secondary.span, false, Some(&secondary.label));
        }

        output
    }

    /// Helper: apply ANSI color code (or return text unchanged if `!self.color`).
    fn paint<'b>(&self, ansi_code: &str, text: &'b str) -> Cow<'b, str> {
        if !self.color {
            Cow::Borrowed(text)
        } else {
            Cow::Owned(format!("{}{}\x1b[0m", ansi_code, text))
        }
    }

    /// Get the English severity word (e.g., "error", "warning").
    fn severity_word(&self, severity: Severity) -> &'static str {
        match severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Hint => "help",
            Severity::Lint => "lint",
        }
    }

    /// Get the ANSI code for a severity (bold prefix + color).
    fn severity_ansi_code(&self, severity: Severity) -> &'static str {
        match severity {
            Severity::Error => "\x1b[1;31m",   // bold red
            Severity::Warning => "\x1b[1;33m", // bold yellow
            Severity::Note => "\x1b[1;36m",    // bold cyan
            Severity::Hint => "\x1b[1;34m",    // bold blue
            Severity::Lint => "\x1b[1;35m",    // bold magenta
        }
    }

    /// Render a single source excerpt block (primary or secondary).
    fn render_span_block(
        &self,
        output: &mut String,
        span: Span,
        is_primary: bool,
        label: Option<&str>,
    ) {
        let file_id = span.file();
        let byte_start = span.byte_start();
        let byte_len = span.byte_len();

        // Resolve to line/col. If out of range, skip the excerpt.
        let line_col = match self.source_map.byte_to_line_col(file_id, byte_start) {
            Some(lc) => lc,
            None => return,
        };

        let path = self.source_map.path(file_id);
        let content = self.source_map.content(file_id);

        // Determine line bytes (from last newline or 0, to next newline or end).
        let line_start_byte = self.find_line_start_byte(content, byte_start as usize);
        let line_end_byte = self.find_line_end_byte(content, byte_start as usize);
        let line_content = &content[line_start_byte..line_end_byte];

        // Calculate padding: chars from line start to byte_start.
        let padding_chars = content[line_start_byte..byte_start as usize]
            .chars()
            .count() as u32;

        // Render location header.
        let arrow = if is_primary { "-->" } else { ":::" };
        output.push_str(&format!(
            "  {} {}:{}:{}\n",
            arrow,
            path.display(),
            line_col.line,
            line_col.col
        ));
        output.push_str("   |\n");

        // Render source line.
        output.push_str(&format!(" {} | {}\n", line_col.line, line_content));

        // Render caret line. Clip byte_end to content length so we never
        // panic on a malformed span that runs past EOF — emitters can
        // (and have, e.g. early lexer literals) produce such spans.
        let byte_end = ((byte_start + byte_len) as usize).min(content.len());

        // Check if span extends past this line.
        if byte_end <= line_end_byte {
            // Single-line span: count caret chars in the span.
            let caret_count = content[byte_start as usize..byte_end].chars().count();
            let caret_str = "^".repeat(caret_count);
            output.push_str(&format!(
                "   | {}{}",
                " ".repeat(padding_chars as usize),
                caret_str
            ));
            if let Some(lbl) = label {
                output.push_str(&format!(" {}", lbl));
            }
            output.push('\n');
        } else {
            // Multi-line span: carets on first line only, add text.
            let line_end_for_carets = content[byte_start as usize..line_end_byte].chars().count();
            let caret_str = "^".repeat(line_end_for_carets);

            // Count newlines in the span to compute the total number of lines spanned.
            let newline_count = content[byte_start as usize..byte_end]
                .chars()
                .filter(|&ch| ch == '\n')
                .count();
            let lines_spanned = newline_count + 1;

            output.push_str(&format!(
                "   | {}{} (spans {} lines)",
                " ".repeat(padding_chars as usize),
                caret_str,
                lines_spanned
            ));
            if let Some(lbl) = label {
                output.push_str(&format!(" {}", lbl));
            }
            output.push('\n');
        }
    }

    /// Find the byte offset of the start of the line containing the given byte offset.
    fn find_line_start_byte(&self, content: &str, byte: usize) -> usize {
        let mut start = byte;
        while start > 0 && content.as_bytes()[start - 1] != b'\n' {
            start -= 1;
        }
        start
    }

    /// Find the byte offset of the end of the line containing the given byte offset (exclusive, before `\n`).
    fn find_line_end_byte(&self, content: &str, byte: usize) -> usize {
        let mut end = byte;
        while end < content.len() && content.as_bytes()[end] != b'\n' {
            end += 1;
        }
        end
    }
}
