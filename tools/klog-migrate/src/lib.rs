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
pub use scan::{Match, scan};
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

/// End-to-end migration of a single source buffer.
///
/// Returns the migrated source (or the input unchanged if no matches were
/// found), the number of sites rewritten, and any [`Warning`]s the caller
/// should surface.
///
/// # Errors
///
/// Returns [`SpliceError`] if two matches overlap (a bug in the scanner —
/// the invariant is that the scanner produces disjoint, source-ordered
/// matches).
pub fn migrate(
    source: &str,
    opts: &RenderOpts,
) -> Result<(String, usize, Vec<Warning>), SpliceError> {
    let matches = scan(source);
    if matches.is_empty() {
        return Ok((source.to_owned(), 0, Vec::new()));
    }
    let warnings = collect_warnings(source, &matches);
    let replacements: Vec<_> = matches
        .iter()
        .map(|m| render_replacement(m, source, opts))
        .collect();
    let out = apply_replacements(source, &replacements)?;
    Ok((out, matches.len(), warnings))
}

fn collect_warnings(source: &str, matches: &[Match]) -> Vec<Warning> {
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
