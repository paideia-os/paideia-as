//! Replacement rendering for a scanned [`Match`].
//!
//! Given a [`Match`] and [`RenderOpts`], produces a [`Replacement`] that
//! `splice` applies to the source buffer.

use crate::scan::Match;
use crate::splice::Replacement;
use regex::Regex;

/// Options controlling how a match is rendered into a klog block.
#[derive(Clone, Debug)]
pub struct RenderOpts {
    /// Level literal used for non-fail messages (default: 3 = LEVEL_INFO).
    pub level: u32,
    /// Level literal used when `fail_pattern` matches (default: 1 = LEVEL_ERROR).
    ///
    /// Historical note (paideia-as#1274 / paideia-os#704): the v0.20.1 delivery
    /// defaulted this to `5`, which corresponds to `LEVEL_TRACE` in
    /// paideia-os's `src/kernel/core/klog/level.pdx`. Because
    /// `KLOG_COMPILE_LEVEL=3` and `klog_emit_core` filters with
    /// `cmp rdi, KLOG_COMPILE_LEVEL; ja emit_skip`, every `*_fail_msg` /
    /// `*_err_msg` witness the tool emitted was silently dropped. paideia-os
    /// inline-fixed 84 sites at 463b16f; this default aligns future runs
    /// with paideia-os's actual `LEVEL_ERROR = 1`.
    pub fail_level: u32,
    /// Symbol name for the SUBSYS argument (default: "SUBSYS_BOOT").
    pub subsys: String,
    /// Regex tested against `Match::msg_symbol` to elevate to `fail_level`.
    pub fail_pattern: Regex,
}

impl RenderOpts {
    /// Construct the defaults documented in `klog-migration-helper.md`.
    ///
    /// # Panics
    ///
    /// Panics only if the default fail-pattern regex fails to compile —
    /// a static invariant that the unit test `default_regex_is_valid`
    /// guards.
    pub fn defaults() -> Self {
        Self {
            level: 3,
            fail_level: 1,
            subsys: "SUBSYS_BOOT".to_owned(),
            fail_pattern: Regex::new(r"(?i)(fail|err)").expect("default fail-pattern is valid"),
        }
    }
}

/// Render one match into its [`Replacement`].
///
/// The match's `byte_start` points at the `lea` token, meaning the source
/// buffer already contains the line's leading indent immediately before
/// `byte_start`. The replacement therefore emits its FIRST line without any
/// leading indent (the pre-existing indent stays in place) and prepends
/// `indent` to every subsequent line so all four rendered lines end up
/// column-aligned.
pub fn render_replacement(m: &Match, source: &str, opts: &RenderOpts) -> Replacement {
    let indent = indent_of_line_containing(source, m.byte_start);
    let level = if opts.fail_pattern.is_match(&m.msg_symbol) {
        opts.fail_level
    } else {
        opts.level
    };
    let new_text = format!(
        "mov rdi, {lvl};\n\
         {i}lea rsi, [rip + {sub}];\n\
         {i}lea rdx, [rip + {sym}];\n\
         {i}call klog_s1;",
        i = indent,
        lvl = level,
        sub = opts.subsys,
        sym = m.msg_symbol,
    );
    Replacement {
        byte_start: m.byte_start,
        byte_end: m.byte_end,
        new_text,
    }
}

/// Return the indent (leading whitespace) of the line containing `byte_offset`.
///
/// If `byte_offset` sits inside the line's leading whitespace (e.g. the
/// scanner's match starts at the first non-whitespace token), the returned
/// string covers everything from the line's start up to `byte_offset`
/// (which by construction is exactly the indent for that line).
fn indent_of_line_containing(source: &str, byte_offset: usize) -> String {
    let line_start = source[..byte_offset]
        .rfind('\n')
        .map(|nl| nl + 1)
        .unwrap_or(0);
    let mut end = line_start;
    for (idx, ch) in source[line_start..byte_offset].char_indices() {
        if ch == ' ' || ch == '\t' {
            end = line_start + idx + ch.len_utf8();
        } else {
            break;
        }
    }
    source[line_start..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    fn render_all(src: &str, opts: &RenderOpts) -> Vec<Replacement> {
        scan(src)
            .iter()
            .map(|m| render_replacement(m, src, opts))
            .collect()
    }

    #[test]
    fn default_regex_is_valid() {
        // The RenderOpts::defaults panic-invariant guard.
        let _ = RenderOpts::defaults();
    }

    #[test]
    fn default_level_for_ok_msg() {
        let src = "      lea rdi, [rip + banner_msg];\n      call uart_puts;\n";
        let reps = render_all(src, &RenderOpts::defaults());
        assert_eq!(reps.len(), 1);
        assert!(
            reps[0].new_text.contains("mov rdi, 3;"),
            "got: {}",
            reps[0].new_text
        );
        assert!(reps[0].new_text.contains("lea rsi, [rip + SUBSYS_BOOT];"));
        assert!(reps[0].new_text.contains("lea rdx, [rip + banner_msg];"));
        assert!(reps[0].new_text.contains("call klog_s1;"));
    }

    #[test]
    fn fail_pattern_elevates_level() {
        // paideia-as#1274 / paideia-os#704: fail_level must be 1 (LEVEL_ERROR),
        // not 5 (LEVEL_TRACE), or the message is silently dropped by
        // klog_emit_core's `cmp rdi, KLOG_COMPILE_LEVEL=3; ja skip` gate.
        let src = "      lea rdi, [rip + logp_hex_fail_msg];\n      call uart_puts;\n";
        let reps = render_all(src, &RenderOpts::defaults());
        assert!(reps[0].new_text.contains("mov rdi, 1;"));
        assert!(reps[0].new_text.contains("lea rdx, [rip + logp_hex_fail_msg];"));
    }

    #[test]
    fn err_pattern_elevates_level() {
        // paideia-as#1274 / paideia-os#704: LEVEL_ERROR = 1, not 5.
        let src = "      lea rdi, [rip + wsl_err_msg];\n      call uart_puts;\n";
        let reps = render_all(src, &RenderOpts::defaults());
        assert!(reps[0].new_text.contains("mov rdi, 1;"));
    }

    #[test]
    fn custom_subsys_is_used() {
        let src = "      lea rdi, [rip + m2_ring_ok_msg];\n      call uart_puts;\n";
        let mut opts = RenderOpts::defaults();
        opts.subsys = "SUBSYS_INT_".to_owned();
        let reps = render_all(src, &opts);
        assert!(reps[0].new_text.contains("lea rsi, [rip + SUBSYS_INT_];"));
    }

    #[test]
    fn indent_is_preserved_6_spaces() {
        // First line has NO indent (source's original indent bytes stay in
        // place because the match starts at `lea`, not at column 0).
        // Subsequent lines each carry the 6-space indent explicitly.
        let src = "      lea rdi, [rip + banner_msg];\n      call uart_puts;\n";
        let reps = render_all(src, &RenderOpts::defaults());
        let lines: Vec<&str> = reps[0].new_text.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(!lines[0].starts_with(' '), "first line must not re-indent");
        for line in &lines[1..] {
            assert!(line.starts_with("      "), "line missing indent: {line:?}");
        }
    }

    #[test]
    fn indent_is_preserved_tab() {
        let src = "\tlea rdi, [rip + banner_msg];\n\tcall uart_puts;\n";
        let reps = render_all(src, &RenderOpts::defaults());
        let lines: Vec<&str> = reps[0].new_text.lines().collect();
        assert!(!lines[0].starts_with('\t'), "first line must not re-indent");
        for line in &lines[1..] {
            assert!(line.starts_with('\t'), "line missing tab indent: {line:?}");
        }
    }

    #[test]
    fn zero_indent_produces_zero_indent_output() {
        let src = "lea rdi, [rip + banner_msg]; call uart_puts;\n";
        let reps = render_all(src, &RenderOpts::defaults());
        for line in reps[0].new_text.lines() {
            assert!(!line.starts_with(' ') && !line.starts_with('\t'));
        }
    }
}
