//! paideia-fmt: source code formatter for paideia-as.
//!
//! Phase-2-m8-010 minimum:
//! - Normalises trailing whitespace (strips).
//! - Normalises tab-vs-space (configurable; default 4 spaces).
//! - Normalises blank-line runs (max 2 consecutive blank lines).
//! - Converts ASCII↔Unicode glyphs by config: `->` ↔ `→`, `=>` ↔ `⇒`, `\` ↔ `λ`, `forall` ↔ `∀`, etc.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

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
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            tab_width: 4,
            use_tabs: false,
            max_blank_lines: 2,
            glyph_style: GlyphStyle::Preserve,
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

/// ASCII ↔ Unicode glyph pairs.
const GLYPHS: &[(&str, &str)] = &[
    ("->", "→"),
    ("=>", "⇒"),
    ("/=", "≠"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("\\\\", "λ"),
];

/// Format the source code with the given options.
pub fn format(source: &str, opts: &FormatOptions) -> String {
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
    fn format_strips_trailing_whitespace() {
        let input = "let x = 1   \nlet y = 2  ";
        let opts = FormatOptions::default();
        let result = format(input, &opts);
        assert!(result.lines().all(|line| !line.ends_with(' ')));
    }

    #[test]
    fn format_caps_blank_line_runs_at_max_blank_lines() {
        let input = "line1\n\n\n\n\nline2";
        let opts = FormatOptions {
            max_blank_lines: 2,
            ..Default::default()
        };
        let result = format(input, &opts);
        let blank_count = result
            .lines()
            .zip(result.lines().skip(1))
            .filter(|(a, b)| a.trim().is_empty() && b.trim().is_empty())
            .count();
        assert!(blank_count <= 2);
    }

    #[test]
    fn format_no_op_on_already_formatted() {
        let input = "let x = 1\nlet y = 2\n";
        let opts = FormatOptions::default();
        let result = format(input, &opts);
        assert_eq!(result, input);
    }

    #[test]
    fn format_idempotent() {
        let input = "let x = 1   \n\n\n\n\nlet y = 2  ";
        let opts = FormatOptions::default();
        let result1 = format(input, &opts);
        let result2 = format(&result1, &opts);
        assert_eq!(result1, result2);
    }

    #[test]
    fn format_with_glyph_conversion() {
        let input = "fn foo() -> int";
        let opts = FormatOptions {
            glyph_style: GlyphStyle::Unicode,
            ..Default::default()
        };
        let result = format(input, &opts);
        assert_eq!(result, "fn foo() → int\n");
    }
}
