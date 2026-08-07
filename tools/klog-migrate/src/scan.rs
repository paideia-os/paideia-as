//! Token-stream pattern scanner.
//!
//! Walks the lexer output for the direct-UART pattern and produces one
//! [`Match`] per hit. Trivia (comments, whitespace) between adjacent tokens
//! is transparent because the lexer already collects it separately. Matches
//! inside comments or string literals cannot occur: comments become
//! `Trivia` and strings become a single `StringLit` token whose interior is
//! never traversed.
//!
//! ## Pattern shape
//!
//! The core pattern is 10 tokens (`lea rdi, [rip + SYM] call uart_puts`).
//! `.pdx` accepts semicolon-optional statement terminators, so two
//! semicolons may or may not be present:
//!
//! - Position 8: `;` after `]`   (between the two statements)
//! - Position 11: `;` after `uart_puts` (end of the second statement)
//!
//! Both are matched greedily when present, but neither is required. Real
//! `.pdx` sources mix all four variants freely (see paideia-os#717 / #1273).
//!
//! ## Opt-out annotation (#1275)
//!
//! A site may be marked as intentionally-preserved raw `uart_puts` by
//! adding a `// klog-migrate: skip` comment anywhere on a line that
//! contains part of the matched pattern (the `lea` line, the `call
//! uart_puts` line, or any intermediate line). The scanner still emits a
//! [`Match`] for such sites but sets `Match::skip` to
//! `Some(SkipReason::Annotation)`, and downstream stages (the migrator in
//! [`crate::migrate`]) exclude annotated sites from rewriting while still
//! surfacing them in operator reports.
//!
//! Rationale: `kernel_main.pdx` at paideia-os HEAD keeps two
//! `uart_rx_smoke_prefix_msg` sites raw (klog's synthetic `\n` would split
//! the two-line "UART RX: abc" wire fingerprint). Without an opt-out,
//! every dry-run reports these as noise.

use std::sync::OnceLock;

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_lexer::{Lexer, SourceText, Token, TokenKind};
use regex::Regex;

/// A single successful pattern match.
///
/// `byte_start` .. `byte_end` covers the full span of the matched pattern
/// in the source (inclusive of the trailing `;` of `call uart_puts;` when
/// present; otherwise ending at the last consumed token — the `uart_puts`
/// identifier or the preceding `;` after `]`). `msg_symbol` is the captured
/// identifier at position 6, e.g. `banner_msg`.
///
/// `skip` is `Some(SkipReason::Annotation)` when the site carries a
/// `// klog-migrate: skip` opt-out on one of its lines (#1275). Skipped
/// matches still appear in the vector — downstream stages surface them in
/// operator reports — but the migrator does not rewrite them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Match {
    /// Byte offset of the first character of `lea` in the source.
    pub byte_start: usize,
    /// Byte offset one past the last character (`;`) in the source.
    pub byte_end: usize,
    /// The captured msg symbol name, verbatim.
    pub msg_symbol: String,
    /// Reason this site is skipped from rewriting, if any. `None` means
    /// the site is a normal rewrite target.
    pub skip: Option<SkipReason>,
}

/// Why a matched site is excluded from rewriting.
///
/// Currently only [`SkipReason::Annotation`] exists (`// klog-migrate: skip`).
/// New reasons (e.g. a whole-file directive, a token-window heuristic) are
/// additive — construction sites remain forward-compatible via the
/// `#[non_exhaustive]` marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SkipReason {
    /// The site carries a `// klog-migrate: skip` opt-out on one of its
    /// pattern-covered lines (#1275). Case-insensitive; whitespace between
    /// `klog-migrate`, `:`, and `skip` is permitted.
    Annotation,
}

/// Scan a source buffer for every direct-UART pattern.
///
/// Returns matches in source order (ascending `byte_start`). Malformed
/// sources still produce best-effort matches — the lexer error-recovers
/// past unrecognised bytes, and any partial pattern simply fails to
/// match.
pub fn scan(source: &str) -> Vec<Match> {
    let file = FileId::new(1).expect("FileId(1) is non-zero");
    // paideia-as-lexer expects a validated SourceText. If the input isn't
    // valid UTF-8 or is empty, we return no matches — this is a source-in
    // rewriter, not a diagnostic tool.
    let src = match SourceText::from_bytes(file, source.as_bytes()) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut lexer = Lexer::new(file, &src);
    let mut sink = SilentSink::default();
    let tokens = lexer.collect_tokens(&mut sink);
    scan_tokens(source, &tokens)
}

fn scan_tokens(source: &str, tokens: &[Token]) -> Vec<Match> {
    let mut out = Vec::new();
    // Minimum pattern is 10 tokens (both statement-terminating semicolons
    // absent); maximum is 12 (both present). Advance by whatever the match
    // consumed on hit, one token on miss.
    let mut i = 0;
    while i < tokens.len() {
        if let Some((m, consumed)) = try_match_at(source, tokens, i) {
            debug_assert!(consumed >= 10 && consumed <= 12);
            out.push(m);
            i += consumed;
        } else {
            i += 1;
        }
    }
    out
}

/// Try to match the pattern starting at `tokens[start]`.
///
/// Returns `(Match, tokens_consumed)` on success. Positions 8 (`;` after
/// `]`) and 11 (`;` after `uart_puts`) are optional — greedily consumed
/// when present, quietly skipped when absent. This mirrors `.pdx`'s
/// semicolon-optional statement terminators; without it the scanner
/// silently drops every valid site written in the trailing-newline style,
/// as paideia-os#717 / #1273 discovered on real kernel sources.
fn try_match_at(
    source: &str,
    tokens: &[Token],
    start: usize,
) -> Option<(Match, usize)> {
    // Fast reject: the shortest legal pattern needs 10 tokens.
    if start + 10 > tokens.len() {
        return None;
    }
    let mut cursor = start;

    // 0: Ident "lea"
    ident_text_eq(source, &tokens[cursor], "lea")?;
    let byte_start = tokens[cursor].span.byte_start() as usize;
    cursor += 1;
    // 1: Ident "rdi"
    ident_text_eq(source, &tokens[cursor], "rdi")?;
    cursor += 1;
    // 2: Comma
    kind_eq(&tokens[cursor], TokenKind::Comma)?;
    cursor += 1;
    // 3: LBracket
    kind_eq(&tokens[cursor], TokenKind::LBracket)?;
    cursor += 1;
    // 4: Ident "rip"
    ident_text_eq(source, &tokens[cursor], "rip")?;
    cursor += 1;
    // 5: Plus
    kind_eq(&tokens[cursor], TokenKind::Plus)?;
    cursor += 1;
    // 6: Ident (captured msg symbol)
    if tokens[cursor].kind != TokenKind::Ident {
        return None;
    }
    let msg_symbol = span_text(source, tokens[cursor].span).to_owned();
    cursor += 1;
    // 7: RBracket
    kind_eq(&tokens[cursor], TokenKind::RBracket)?;
    cursor += 1;
    // 8: Optional Semicolon (semicolon-optional style: this may be omitted
    // when the two instructions sit on separate lines).
    if cursor < tokens.len() && tokens[cursor].kind == TokenKind::Semicolon {
        cursor += 1;
    }
    // Need at least two more tokens: `call` `uart_puts`.
    if cursor + 2 > tokens.len() {
        return None;
    }
    // 9: Ident "call"
    ident_text_eq(source, &tokens[cursor], "call")?;
    cursor += 1;
    // 10: Ident "uart_puts" — always the last mandatory token, so its end
    // is the running byte_end baseline.
    ident_text_eq(source, &tokens[cursor], "uart_puts")?;
    let mut byte_end = tokens[cursor].span.byte_end() as usize;
    cursor += 1;
    // 11: Optional Semicolon (same rationale as position 8).
    if cursor < tokens.len() && tokens[cursor].kind == TokenKind::Semicolon {
        byte_end = tokens[cursor].span.byte_end() as usize;
        cursor += 1;
    }

    let skip = if has_skip_annotation(source, byte_start, byte_end) {
        Some(SkipReason::Annotation)
    } else {
        None
    };

    Some((
        Match {
            byte_start,
            byte_end,
            msg_symbol,
            skip,
        },
        cursor - start,
    ))
}

/// Scan the source region covering every line that the match touches for a
/// `// klog-migrate: skip` opt-out marker (#1275).
///
/// The check window expands the match's `byte_start..byte_end` outward to
/// the surrounding line boundaries so a marker anywhere on the `lea` line,
/// the `call uart_puts` line, or any intermediate line is honored. The
/// window is guaranteed to contain no string literals (the lexer already
/// admitted the enclosed tokens as real code), so a raw regex scan cannot
/// pick up a false positive from inside a string.
///
/// Case-insensitive. Whitespace between `klog-migrate`, `:`, and `skip` is
/// permitted so authors can space the marker naturally
/// (`// klog-migrate : skip`, `// KLOG-MIGRATE:SKIP`).
fn has_skip_annotation(source: &str, byte_start: usize, byte_end: usize) -> bool {
    let line_start = source[..byte_start].rfind('\n').map_or(0, |n| n + 1);
    let line_end = source[byte_end..]
        .find('\n')
        .map_or(source.len(), |n| byte_end + n);
    let window = &source[line_start..line_end];
    skip_regex().is_match(window)
}

/// Cached compilation of the `// klog-migrate: skip` opt-out regex.
///
/// The regex matches `klog-migrate`, optional whitespace, `:`, optional
/// whitespace, then `skip`, case-insensitively. It does not require the
/// leading `//` because a comment marker embedded in a wider comment
/// (`// #1275 klog-migrate: skip`) must also fire.
fn skip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)klog-migrate\s*:\s*skip")
            .expect("static regex is well-formed")
    })
}

fn kind_eq(tok: &Token, kind: TokenKind) -> Option<()> {
    if tok.kind == kind { Some(()) } else { None }
}

fn ident_text_eq(source: &str, tok: &Token, expected: &str) -> Option<()> {
    if tok.kind != TokenKind::Ident {
        return None;
    }
    if span_text(source, tok.span) == expected {
        Some(())
    } else {
        None
    }
}

fn span_text(source: &str, span: Span) -> &str {
    let start = span.byte_start() as usize;
    let end = span.byte_end() as usize;
    &source[start..end]
}

/// A `DiagnosticSink` that silently swallows every diagnostic. Migration
/// is a source-in / source-out operation; parse errors are the caller's
/// concern (they surface at `paideia-as build` time).
#[derive(Default)]
struct SilentSink {
    count: usize,
    errors: usize,
}

impl paideia_as_diagnostics::DiagnosticSink for SilentSink {
    fn emit(
        &mut self,
        d: paideia_as_diagnostics::Diagnostic,
    ) -> Result<(), paideia_as_diagnostics::DiagnosticOverflow> {
        self.count += 1;
        if d.severity() == paideia_as_diagnostics::Severity::Error {
            self.errors += 1;
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.count
    }

    fn error_count(&self) -> usize {
        self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Vec<Match> {
        scan(src)
    }

    #[test]
    fn empty_source_is_empty() {
        assert!(one("").is_empty());
    }

    #[test]
    fn single_pattern_matches() {
        let src = "lea rdi, [rip + banner_msg]; call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
        assert_eq!(ms[0].byte_start, 0);
        // Match ends at the ';' after uart_puts.
        assert_eq!(ms[0].byte_end, src.trim_end_matches('\n').len());
    }

    #[test]
    fn multiline_pattern_matches() {
        let src = "      lea rdi, [rip + idt_ok_msg];\n      call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "idt_ok_msg");
    }

    #[test]
    fn lea_without_call_does_not_match() {
        let src = "lea rdi, [rip + banner_msg]; nop;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn call_without_lea_does_not_match() {
        let src = "mov rax, 1; call uart_puts;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn wrong_register_does_not_match() {
        // lea rsi, not rdi
        let src = "lea rsi, [rip + banner_msg]; call uart_puts;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn comment_is_not_matched() {
        let src = "// lea rdi, [rip + banner_msg]; call uart_puts;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn string_literal_is_not_matched() {
        let src = r#"let x : str = "lea rdi, [rip + banner_msg]; call uart_puts;""#;
        assert!(one(src).is_empty());
    }

    #[test]
    fn two_adjacent_patterns_match() {
        let src = "lea rdi, [rip + a_msg]; call uart_puts;\n\
                   lea rdi, [rip + b_msg]; call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].msg_symbol, "a_msg");
        assert_eq!(ms[1].msg_symbol, "b_msg");
        assert!(ms[0].byte_end <= ms[1].byte_start);
    }

    #[test]
    fn different_call_target_does_not_match() {
        // call uart_putc, not uart_puts
        let src = "lea rdi, [rip + banner_msg]; call uart_putc;\n";
        assert!(one(src).is_empty());
    }

    #[test]
    fn trailing_comment_still_matches() {
        let src = "lea rdi, [rip + banner_msg]; call uart_puts;  // OK marker\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
    }

    // ---- semicolon-optional variants (paideia-as#1273) -----------------

    #[test]
    fn no_semi_after_lea_still_matches() {
        // The `;` after `]` is elided; `.pdx` allows this when the two
        // statements land on separate lines. paideia-os#717 / #1273: was
        // silently skipped by the fixed 12-token window before the fix.
        let src = "      lea rdi, [rip + banner_msg]\n      call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
        // byte_end points at the `;` after `uart_puts`.
        assert_eq!(&src[..ms[0].byte_end].chars().last().unwrap(), &';');
    }

    #[test]
    fn no_semi_after_call_still_matches() {
        // The trailing `;` after `uart_puts` is elided (common on the last
        // instruction of a block, seen on kernel_main.pdx:316,319).
        let src = "      lea rdi, [rip + banner_msg]; call uart_puts\n      ret;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
        // byte_end lands at the end of `uart_puts`.
        assert_eq!(&src[ms[0].byte_start..ms[0].byte_end].ends_with("uart_puts"), &true);
    }

    #[test]
    fn no_semi_at_either_position_still_matches() {
        // Both semicolons elided — the "purest" newline-terminated style.
        let src = "      lea rdi, [rip + banner_msg]\n      call uart_puts\n      ret\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].msg_symbol, "banner_msg");
        assert_eq!(&src[ms[0].byte_start..ms[0].byte_end].ends_with("uart_puts"), &true);
    }

    #[test]
    fn mixed_semi_and_no_semi_patterns_all_match() {
        // Two consecutive patterns where the first uses semicolon-optional
        // style (both semis omitted) and the second uses the semicolon
        // style (both present). Both must match.
        let src = "      lea rdi, [rip + a_msg]\n      call uart_puts\n\
                   \n\
                   \n      lea rdi, [rip + b_msg]; call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 2, "got: {:?}", ms);
        assert_eq!(ms[0].msg_symbol, "a_msg");
        assert_eq!(ms[1].msg_symbol, "b_msg");
        assert!(ms[0].byte_end <= ms[1].byte_start);
    }

    // ---- opt-out annotation (paideia-as#1275) --------------------------

    #[test]
    fn unannotated_match_has_no_skip_reason() {
        // Baseline: an ordinary match carries `skip: None` so downstream
        // rewriting proceeds as before the #1275 feature landed.
        let src = "      lea rdi, [rip + banner_msg];\n      call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert!(ms[0].skip.is_none(), "expected None, got {:?}", ms[0].skip);
    }

    #[test]
    fn annotation_on_lea_line_marks_skip() {
        // Marker sits after the `;` on the `lea` line — same line, valid site.
        let src = "      lea rdi, [rip + banner_msg]; // klog-migrate: skip\n\
                   \x20     call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].skip, Some(SkipReason::Annotation));
        // The captured symbol is still correct — the skip decision does not
        // affect the underlying match data.
        assert_eq!(ms[0].msg_symbol, "banner_msg");
    }

    #[test]
    fn annotation_on_call_line_marks_skip() {
        let src = "      lea rdi, [rip + banner_msg];\n\
                   \x20     call uart_puts; // klog-migrate: skip\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].skip, Some(SkipReason::Annotation));
    }

    #[test]
    fn annotation_between_lea_and_call_marks_skip() {
        // Marker on a bare intermediate line (three lines total).
        let src = "      lea rdi, [rip + banner_msg];\n\
                   \x20     // klog-migrate: skip — reason\n\
                   \x20     call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].skip, Some(SkipReason::Annotation));
    }

    #[test]
    fn annotation_is_case_insensitive() {
        // Upper-case, mixed-case, tight-spacing variants all fire.
        for marker in [
            "// KLOG-MIGRATE: SKIP",
            "// KLOG-migrate:skip",
            "// Klog-Migrate : Skip",
            "// klog-migrate:  skip",
        ] {
            let src = format!(
                "      lea rdi, [rip + banner_msg]; {marker}\n\
                 \x20     call uart_puts;\n"
            );
            let ms = one(&src);
            assert_eq!(ms.len(), 1, "marker {marker:?} did not match");
            assert_eq!(
                ms[0].skip,
                Some(SkipReason::Annotation),
                "marker {marker:?} did not trigger skip"
            );
        }
    }

    #[test]
    fn annotation_on_previous_line_does_not_skip() {
        // A marker on the line BEFORE `lea` is out of scope — the check
        // window covers only lines the pattern actually touches.
        let src = "      // klog-migrate: skip\n\
                   \x20     lea rdi, [rip + banner_msg];\n\
                   \x20     call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(
            ms[0].skip, None,
            "annotation on the preceding line must not skip the match"
        );
    }

    #[test]
    fn annotation_on_following_line_does_not_skip() {
        // Same reasoning as previous line — marker on the line AFTER `call
        // uart_puts` is out of scope. Callers who want to skip must place
        // the marker on one of the match's own lines.
        let src = "      lea rdi, [rip + banner_msg];\n\
                   \x20     call uart_puts;\n\
                   \x20     // klog-migrate: skip\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].skip, None);
    }

    #[test]
    fn annotation_embedded_in_longer_comment_still_skips() {
        // Author style: additional context around the marker.
        let src = "      lea rdi, [rip + banner_msg]; // #1275 klog-migrate: skip — wire fingerprint\n\
                   \x20     call uart_puts;\n";
        let ms = one(src);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].skip, Some(SkipReason::Annotation));
    }
}
