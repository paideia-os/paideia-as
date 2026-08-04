//! Terminal color detection for diagnostic rendering.
//!
//! #1257: replace the seven hard-coded `color: true` diagnostic-renderer
//! constructions with a runtime decision so ANSI codes don't leak into
//! pipes and log files.
//!
//! Precedence:
//! 1. `NO_COLOR` env var (any value) → force OFF (the de facto convention;
//!    see https://no-color.org/).
//! 2. `CLICOLOR_FORCE=1` → force ON (respects the classic bsd/less flag).
//! 3. Otherwise: stderr `is_terminal()`.
//!
//! A `--color=auto|always|never` CLI flag can layer on top of this later
//! by passing a `ColorChoice` in place of the raw bool.

use std::io::IsTerminal;

/// Decide whether stderr diagnostics should include ANSI color escapes.
#[must_use]
pub fn should_use_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("CLICOLOR_FORCE").map(|v| v == "1").unwrap_or(false) {
        return true;
    }
    std::io::stderr().is_terminal()
}
