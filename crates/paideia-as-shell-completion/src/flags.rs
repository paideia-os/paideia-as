//! R228.M5 — per-command flag catalogue.
//!
//! [`CommandFlags`] is the shape the R229 REPL hands the completion
//! engine on start-up: a flat, per-command list of short (`-a`) and long
//! (`--all`) option names plus an optional human-facing description
//! keyed on the *bare* option name (no leading dashes).
//!
//! # Design decisions
//!
//! * **Bare names, dashes reintroduced by the emitter.** Callers seed
//!   `short: ["a", "l"]` and `long: ["all"]`; the completion engine's
//!   emitter prepends `-` / `--` when it builds the `Candidate::text`.
//!   Storing bare names keeps the catalogue independent of the
//!   surface-syntax dash convention and lets a future consumer that
//!   renders flags differently (a Windows-style `/a`) reuse the same
//!   map without a rewrite.
//!
//! * **`descriptions` keyed on the bare name, one map for both short
//!   and long forms.** In the common case a short flag is an alias for
//!   a long one; sharing the description keeps the catalogue small and
//!   avoids the two-forms-drifting bug the two-map alternative invites.
//!   The engine looks up `descriptions.get(name)` for both the short-
//!   and long-form candidate and populates [`crate::Candidate::type_hint`]
//!   from it.
//!
//! * **Builder-shaped API.** [`CommandFlags::new`] plus `with_short` /
//!   `with_long` chained setters mirror
//!   [`crate::CompletionEngine::with_lists`] / `with_command_types`; the
//!   descriptions map is set through direct field assignment because
//!   the M5 fixture corpus never populates it and a `with_descriptions`
//!   setter would be an unused-API surface at land time.

use std::collections::HashMap;

/// Short (`-a`) and long (`--all`) flag catalogue for one command.
///
/// See the module-level docs for the "bare name" storage convention and
/// the shared-description rationale. All three fields are `pub` because
/// the R229 REPL constructs one of these per registered command at
/// start-up and may prefer direct-field seeding over the builder chain
/// for large catalogues.
#[derive(Clone, Debug, Default)]
pub struct CommandFlags {
    /// Short-form flag names without the leading dash. `["a", "l"]`
    /// yields candidates `-a` and `-l`.
    pub short: Vec<String>,
    /// Long-form flag names without the leading double-dash.
    /// `["all", "long"]` yields candidates `--all` and `--long`.
    pub long: Vec<String>,
    /// Optional human-facing description keyed on the bare flag name
    /// (matching an entry in `short` or `long`). Populates the
    /// [`crate::Candidate::type_hint`] on emission; a missing entry
    /// leaves the type-hint as `None` rather than an empty string —
    /// the REPL renders those two cases differently.
    pub descriptions: HashMap<String, String>,
}

impl CommandFlags {
    /// Empty catalogue. Layer with [`Self::with_short`] and
    /// [`Self::with_long`] to seed the flag lists.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the short-flag list wholesale. The map keys in
    /// `descriptions` stay untouched — a rebuild through the builder
    /// carries prior descriptions forward.
    pub fn with_short(mut self, short: Vec<String>) -> Self {
        self.short = short;
        self
    }

    /// Replace the long-flag list wholesale. Same carry-forward
    /// semantics for `descriptions` as [`Self::with_short`].
    pub fn with_long(mut self, long: Vec<String>) -> Self {
        self.long = long;
        self
    }
}
