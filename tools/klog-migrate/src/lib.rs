//! klog-migrate — tokenizer-driven `.pdx` rewriter for the direct-UART →
//! `klog_s1` migration.
//!
//! See `design/toolchain/klog-migration-helper.md` for the full spec and
//! `paideia-as#1272` (mirrors `paideia-os#717`) for the rationale.
//!
//! # Overview
//!
//! Consumes a `.pdx` source file, tokenises it with `paideia_as_lexer`, and
//! rewrites every occurrence of the pattern
//!
//! ```text
//! lea rdi, [rip + <MSG>]; call uart_puts;
//! ```
//!
//! Both `;` terminators are optional (`.pdx` accepts newline-terminated
//! statements too) — see `scan::try_match_at` and paideia-as#1273.
//!
//! into the structured-log 4-instruction block
//!
//! ```text
//! mov rdi, <LEVEL>;
//! lea rsi, [rip + <SUBSYS>];
//! lea rdx, [rip + <MSG>];
//! call klog_s1;
//! ```
//!
//! Because the paideia-as lexer strips comments and lexes strings as a single
//! opaque `StringLit` token, pattern matches inside comments or string
//! literals are impossible by construction.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod render;
pub mod scan;
pub mod splice;

pub use render::{RenderOpts, render_replacement};
pub use scan::{Match, SkipReason, scan};
pub use splice::{SpliceError, apply_replacements};

use regex::Regex;

/// A migration-time note the caller can surface to the operator.
///
/// Today only [`Warning::TrailingCommentDropped`] is emitted — the migration
/// replaces a byte range that includes a trailing `//` comment on the source
/// side of the rewrite. The rewrite is still correct code, but the comment
/// is silently gone; the caller should review those sites.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Warning {
    /// A rewritten byte range contained a `//` comment that will not appear
    /// in the migrated source. `line` is 1-based.
    TrailingCommentDropped {
        /// 1-based line number of the match start (the `lea` token).
        line: usize,
        /// The msg symbol the match captured.
        msg_symbol: String,
    },
}

/// Outcome of an end-to-end [`migrate`] call.
///
/// Splits total matched sites into two disjoint buckets:
///
/// - `rewritten`: sites the migrator actually replaced.
/// - `skipped_by_annotation`: sites carrying a `// klog-migrate: skip`
///   opt-out (#1275) — left in the source verbatim; still reported so the
///   operator can audit them.
///
/// `warnings` covers only rewritten sites (skipped sites cannot drop a
/// trailing comment because they are not replaced).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrateReport {
    /// The migrated source buffer. Equals the input if `rewritten == 0`.
    pub source: String,
    /// Number of sites the migrator rewrote in place.
    pub rewritten: usize,
    /// Number of pattern hits skipped because of a `// klog-migrate: skip`
    /// opt-out on one of the site's lines (#1275).
    pub skipped_by_annotation: usize,
    /// Advisory warnings the caller should surface (e.g. dropped trailing
    /// comment on a rewritten line).
    pub warnings: Vec<Warning>,
}

/// End-to-end migration of a single source buffer.
///
/// Returns a [`MigrateReport`] with the migrated text, the number of sites
/// rewritten, the number of sites explicitly skipped via
/// `// klog-migrate: skip` (#1275), and any [`Warning`]s the caller should
/// surface.
///
/// # Errors
///
/// Returns [`SpliceError`] if two matches overlap (a bug in the scanner —
/// the invariant is that the scanner produces disjoint, source-ordered
/// matches).
pub fn migrate(source: &str, opts: &RenderOpts) -> Result<MigrateReport, SpliceError> {
    let matches = scan(source);
    if matches.is_empty() {
        return Ok(MigrateReport {
            source: source.to_owned(),
            rewritten: 0,
            skipped_by_annotation: 0,
            warnings: Vec::new(),
        });
    }
    let skipped_by_annotation = matches
        .iter()
        .filter(|m| m.skip == Some(SkipReason::Annotation))
        .count();
    let to_rewrite: Vec<&Match> = matches.iter().filter(|m| m.skip.is_none()).collect();
    let warnings = collect_warnings(source, &to_rewrite);
    if to_rewrite.is_empty() {
        return Ok(MigrateReport {
            source: source.to_owned(),
            rewritten: 0,
            skipped_by_annotation,
            warnings,
        });
    }
    let replacements: Vec<_> = to_rewrite
        .iter()
        .map(|m| render_replacement(m, source, opts))
        .collect();
    let out = apply_replacements(source, &replacements)?;
    Ok(MigrateReport {
        source: out,
        rewritten: to_rewrite.len(),
        skipped_by_annotation,
        warnings,
    })
}

fn collect_warnings(source: &str, matches: &[&Match]) -> Vec<Warning> {
    let mut out = Vec::new();
    for m in matches {
        let slice = &source[m.byte_start..m.byte_end];
        if slice.contains("//") || slice.contains("/*") {
            let line = source[..m.byte_start].bytes().filter(|&b| b == b'\n').count() + 1;
            out.push(Warning::TrailingCommentDropped {
                line,
                msg_symbol: m.msg_symbol.clone(),
            });
        }
    }
    out
}

/// Compile a `--fail-pattern` regex once for reuse across scans.
///
/// # Errors
///
/// Returns the `regex::Error` if the pattern is not a valid regular
/// expression.
pub fn compile_fail_pattern(pat: &str) -> Result<Regex, regex::Error> {
    Regex::new(pat)
}
