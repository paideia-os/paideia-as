//! paideia-as-shell-completion — R228.M1 tab-completion substrate for
//! the semantic shell.
//!
//! # What this crate does at M1
//!
//! Given the user's edit buffer plus a byte-offset cursor, tokenize the
//! pre-cursor slice via `paideia-as-shell-lex` and, based on the last
//! token's [`Context`], emit a `CompletionResponse` naming the byte
//! range the caller should overwrite and a list of `Candidate`s ranked
//! solely by ASCII case-sensitive `starts_with`.
//!
//! The M1 catalogue is intentionally small — command names and
//! known-variable names, both as flat `Vec<String>` — so this crate
//! stays isolated from `paideia-as-shell-cmd` (functor registry) and
//! `paideia-as-shell-repl` (dispatch registry). R229's REPL wire-up
//! (planned for R229.M4-onward) will seed the engine at start-up.
//!
//! # Design decisions
//!
//! * **Pure function, not an iterator.** `complete()` returns a
//!   fully-materialized `CompletionResponse` rather than an
//!   `Iterator<Item=Candidate>` because the caller (REPL / LSP) needs
//!   the count up front to lay out its popup, and the M1 catalogue is
//!   bounded (hundreds of commands, not millions).
//!
//! * **NFC-normalize the prefix before tokenizing** so identifiers
//!   typed with combining marks (`é` as U+0065 U+0301) match a
//!   catalogue keyed on the composed form (U+00E9). Byte offsets in
//!   the response are relative to the normalized prefix, which in the
//!   ASCII-only fast path (the overwhelming common case for command
//!   names) is bit-identical to the input — so REPL callers on ASCII
//!   input never have to re-map. Non-ASCII callers must NFC-normalize
//!   the source on their side before consulting the response for
//!   byte-offset math; the follow-on R221.M6-shape provenance layer
//!   (planned for R228.M4) will surface an `NfcMap` so span
//!   translation is automatic.
//!
//! * **`CandidateKind` is an open categorization, not a permission**.
//!   M1 emits `Command` / `Var` / `Keyword`; `Field` / `Path` /
//!   `Argument` are reserved for M3/M4. The enum is `#[non_exhaustive]`-
//!   free deliberately: this is a workspace-internal crate and any
//!   consumer will be updated in lockstep when a new variant lands.
//!
//! # What lands in later milestones
//!
//! * M2 — Candidate enrichment (`type_hint` for commands) plus 1-level
//!   Field completion off a `Record.<cursor>` shape. Nested lookup
//!   (`foo.bar.<cursor>`) is deferred to M3+.
//! * M3 — four-tier ranker (exact prefix > case-insensitive prefix >
//!   subsequence) with `(score desc, text asc)` total order.
//! * M4 — recency-aware ranking + usage-history buffer (see
//!   [`history::UsageHistory`] and [`record_selection`]).
//! * M5 — async / streaming candidate providers.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::HashMap;

use paideia_as_shell_lex::{Context, Lexer, Span, Token, TokenKind};
use paideia_as_unicode::nfc_normalize;

pub mod flags;
pub mod history;
pub mod matching;
pub mod path;

pub use flags::CommandFlags;
use history::UsageHistory;
use matching::score_match;
pub use path::PathProvider;

// R228.M4 will consume `paideia_as_shell_ast::SyntaxNode` to resolve
// the *nested* record type behind a chained `.` cursor. R228.M2 only
// needs a flat `records: HashMap<String, Vec<String>>` on the engine,
// which the caller (R229 REPL) seeds from the current session's
// record catalogue — no AST walk yet. Held as a no-name import here
// so the crate's Cargo.toml dep list is stable across the M1..M4
// landings; the actual `use paideia_as_shell_ast::SyntaxNode` will
// land at M4 with the nested-lookup module. The `_` binding is
// Rust's idiom for "load the crate but don't expose a name" — it
// keeps the `unused_crate_dependencies` lint quiet without polluting
// this crate's public surface.
use paideia_as_shell_ast as _;

/// Immutable snapshot of the user's edit buffer plus a byte-offset
/// cursor. Owned so the engine never borrows into an editor buffer
/// that the caller may mutate on the next tick.
#[derive(Clone, Debug)]
pub struct CompletionRequest {
    /// The full source text of the edit buffer as the user typed it
    /// (pre-NFC). The engine NFC-normalizes internally before
    /// tokenizing so `é` composed vs. decomposed spellings both hit.
    pub source: String,
    /// Byte offset into `source` where the cursor sits. Must lie on a
    /// UTF-8 code-point boundary and satisfy `<= source.len()`. Values
    /// outside `[0, source.len()]` are clamped to `source.len()`
    /// defensively — an out-of-range cursor is treated as end-of-input
    /// rather than panicking, because a REPL desync should degrade to
    /// "no completions" rather than crash.
    pub cursor_byte: usize,
}

/// Surface category the caller uses to render a candidate (icon, colour,
/// popup section grouping).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CandidateKind {
    /// A command name (dispatch-registry entry). Emitted when the
    /// cursor sits at command position in `Context::Pipeline`.
    Command,
    /// A lambda-bound or session-scoped variable. Emitted when the
    /// cursor sits in `Context::Lambda`.
    Var,
    /// A record field. Emitted from R228.M2 onward for a 1-level
    /// `Record.<cursor>` shape when the engine's flat `records` map
    /// carries the record name. Chained lookup (`foo.bar.<cursor>`)
    /// awaits R228.M4's AST-driven resolver.
    Field,
    /// A reserved word of the sub-language grammar. Emitted for the
    /// Datalog keyword catalogue in `Context::Datalog`.
    Keyword,
    /// A filesystem path element. Emitted from R228.M6 onward when
    /// the raw pre-cursor bytes form a `/`-anchored path-in-progress
    /// and the engine's [`CompletionEngine::path_provider`] has an
    /// entry for the enclosing directory. See `try_path_completion`
    /// for the detection algorithm and [`PathProvider`] for the seed
    /// shape.
    Path,
    /// An argument value the running command declared. Reserved for
    /// R228.M3. Not emitted at M1.
    Argument,
    /// A command-line flag (short `-a` or long `--all`). Emitted from
    /// R228.M5 onward when the token neighbourhood right of a command
    /// name shapes as a dash-in-progress (`-`, `--`, `-<prefix>`,
    /// `--<prefix>`) and the engine's [`CompletionEngine::command_flags`]
    /// map carries a [`CommandFlags`] entry for that command.
    Flag,
}

/// One completion suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The exact text that will replace the source slice
    /// `[prefix_start, prefix_end)` on acceptance. NFC-normalized, so
    /// the caller inserts the composed form even if the user typed the
    /// decomposed form.
    pub text: String,
    /// The surface category the caller uses for rendering.
    pub kind: CandidateKind,
    /// Optional human-facing render — a description or disambiguating
    /// suffix. `None` means "render `text` alone".  Kept distinct from
    /// [`Candidate::type_hint`] because the REPL and the LSP treat
    /// them differently: `display` is the popup's *label*, while
    /// `type_hint` is the *aligned right-column type annotation* the
    /// popup renders in a dimmer face.
    pub display: Option<String>,
    /// Optional type-annotation string the popup renders in the
    /// aligned right-column type slot. Populated by the R228.M2
    /// enrichment pass for `Command` candidates whose name appears in
    /// [`CompletionEngine::commands_with_types`]; other candidate
    /// kinds leave this `None` until their own enrichment source
    /// (R228.M4's schema-typed record types, R228.M5's argument
    /// declarations) lands. Kept as an owned `String` so the response
    /// never borrows into the engine's catalogue — the caller may
    /// swap the engine between requests.
    pub type_hint: Option<String>,
    /// Rank score assigned by the R228.M3 matcher. Higher means better;
    /// the response's `candidates` list is guaranteed sorted by
    /// `(score desc, text asc)` at the return boundary. See
    /// [`matching::score_match`] for the tier formula. Kept as `i32`
    /// (not `u32`) so a future tier can legitimately assign a negative
    /// penalty score without an awkward type break; callers that only
    /// care about ordering do not need to interpret the numeric value.
    pub score: i32,
}

/// The response the REPL / LSP consumes.
///
/// `candidates` is empty when no completion applies; the caller then
/// beeps or does nothing rather than opening an empty popup.
/// `prefix_start`/`prefix_end` name the byte range in the (NFC-
/// normalized) source that the chosen `text` will overwrite; when
/// `prefix_start == prefix_end == cursor_byte`, the completion is an
/// insertion (nothing to overwrite).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionResponse {
    /// The candidate list. As of R228.M3 the list is guaranteed sorted
    /// by `(score desc, text asc)`; each entry's [`Candidate::score`]
    /// records its rank tier per [`matching::score_match`]. M4 will
    /// layer recency weighting on top of the score tier.
    pub candidates: Vec<Candidate>,
    /// Inclusive start byte of the range the chosen candidate overwrites.
    pub prefix_start: usize,
    /// Exclusive end byte of the range the chosen candidate overwrites.
    pub prefix_end: usize,
}

/// Stateful lookup catalogue.
///
/// Owns the flat command-name and known-variable lists that M1 needs.
/// M2 introduces a `CompletionSource` trait so the engine can pull
/// fresh names on each request (dispatch-registry additions during a
/// REPL session, freshly-bound `let` in the current lambda scope);
/// the M1 shape is deliberately concrete so the R229 wire-up can proceed
/// without a trait decision the ranker milestone will inform.
pub struct CompletionEngine {
    /// Registered command names, in the order the REPL should show
    /// them (dispatch-registry insertion order today; ranked by M3's
    /// scorer at the next milestone).
    pub commands: Vec<String>,
    /// Known variable names in the current session scope. M1 populates
    /// this from the REPL's session-wide `let`-binding table; a lambda-
    /// local shadowing pass lands with M4's scope-aware completion.
    pub known_vars: Vec<String>,
    /// Command name → rendered type-annotation string. Consulted by
    /// the R228.M2 enrichment pass to fill `Candidate::type_hint` for
    /// `Command` candidates. Kept as a flat map (not a
    /// `HashMap<String, TypeExpr>`) at M2 so this crate stays isolated
    /// from `paideia-as-shell-types`; the REPL formats the type once
    /// at engine-seed time and hands the string through.  Missing
    /// entries leave the candidate's `type_hint` as `None` rather
    /// than an empty string — the REPL renders those two cases
    /// differently.
    pub commands_with_types: HashMap<String, String>,
    /// Record name → ordered list of field names. Consulted by the
    /// M2 Field-completion branch. A 1-level map at M2: the value is
    /// a flat `Vec<String>` of field names, not a nested schema.
    /// Chained lookup (`foo.bar.<cursor>` where `bar`'s type is a
    /// record with its own fields) is deferred to R228.M4, which
    /// will introduce an AST-driven resolver on top of this map.
    pub records: HashMap<String, Vec<String>>,
    /// MRU buffer of previously-selected completion texts. Consulted
    /// by every candidate-emitting helper via
    /// [`UsageHistory::recency_boost`] so a repeat selection floats
    /// its candidate above equal-scored but unseen alternatives.
    /// Seeded empty by [`Self::empty`] / [`Self::with_lists`] at the
    /// default capacity; a caller may swap in a different-cap buffer
    /// via [`Self::with_history_capacity`]. Mutated externally through
    /// [`record_selection`] rather than a `mut` reference on `complete`
    /// so the ranker stays a pure `&engine, &req -> response` function.
    pub history: UsageHistory,
    /// Command name → [`CommandFlags`] catalogue. Consulted by the
    /// R228.M5 flag-completion branch when the token neighbourhood
    /// right of a command name shapes as a dash-in-progress. A missing
    /// entry leaves the branch inert (0 candidates); adding the entry
    /// later never changes the M1..M4 branch behaviour because flag
    /// detection short-circuits the argument-position empty branch
    /// only when the dash pattern matches. Kept flat (per-command
    /// map) rather than nested (per-subcommand tree) at M5 because
    /// the R229 REPL registers commands, not subcommands; nested
    /// subcommand catalogues are a follow-on milestone.
    pub command_flags: HashMap<String, CommandFlags>,
    /// Directory -> child-name catalogue consulted by the R228.M6
    /// path-completion branch. Seeded by the R229 REPL from a
    /// directory walk on chdir (and refreshed on mtime change);
    /// defaults to an empty provider so pre-M6 seed paths that never
    /// touched a `PathProvider` continue to emit zero path
    /// candidates without a shape churn. Missing directory keys
    /// yield zero candidates rather than an error -- see
    /// [`PathProvider`]'s module header for the normalization
    /// convention (`"/"` for the root, bare paths otherwise).
    pub path_provider: PathProvider,
}

impl CompletionEngine {
    /// Construct an engine with an empty catalogue. Useful for the
    /// zero-catalogue smoke test and for REPL startup before the
    /// dispatch registry is loaded.
    pub fn empty() -> Self {
        Self {
            commands: Vec::new(),
            known_vars: Vec::new(),
            commands_with_types: HashMap::new(),
            records: HashMap::new(),
            history: UsageHistory::default(),
            command_flags: HashMap::new(),
            path_provider: PathProvider::default(),
        }
    }

    /// Construct an engine pre-seeded with a command and variable list.
    /// The type-hint and record catalogues start empty; layer them on
    /// with [`Self::with_command_types`] and [`Self::with_records`].
    /// Convenience for the M1 fixtures and for the R229 REPL boot
    /// path.
    pub fn with_lists(commands: Vec<String>, known_vars: Vec<String>) -> Self {
        Self {
            commands,
            known_vars,
            commands_with_types: HashMap::new(),
            records: HashMap::new(),
            history: UsageHistory::default(),
            command_flags: HashMap::new(),
            path_provider: PathProvider::default(),
        }
    }

    /// Layer a command → type-hint map on top of an existing engine.
    /// Overwrites any prior map wholesale; the REPL rebuilds this map
    /// on every dispatch-registry mutation, so a partial-update
    /// primitive would be an unused surface.
    pub fn with_command_types(mut self, m: HashMap<String, String>) -> Self {
        self.commands_with_types = m;
        self
    }

    /// Layer a record → field-list map on top of an existing engine.
    /// Overwrites any prior map wholesale for the same reason as
    /// [`Self::with_command_types`].
    pub fn with_records(mut self, records: HashMap<String, Vec<String>>) -> Self {
        self.records = records;
        self
    }

    /// Swap the MRU history buffer for one of the given capacity.
    /// Discards any prior recorded selections — the R229 REPL boot
    /// path chooses the cap before recording anything, so a
    /// preserve-and-resize primitive would be an unused surface.
    /// Kept separate from [`Self::empty`] / [`Self::with_lists`] so
    /// the default construction stays a nullary call.
    pub fn with_history_capacity(mut self, cap: usize) -> Self {
        self.history = UsageHistory::new(cap);
        self
    }

    /// Layer a command → [`CommandFlags`] map on top of an existing
    /// engine. Overwrites any prior map wholesale for the same reason
    /// as [`Self::with_command_types`]. Kept as a fluent builder so
    /// the R229 REPL start-up chain reads:
    ///
    /// ```ignore
    /// CompletionEngine::with_lists(cmds, vars)
    ///     .with_command_types(types)
    ///     .with_command_flags(flags)
    /// ```
    pub fn with_command_flags(mut self, flags: HashMap<String, CommandFlags>) -> Self {
        self.command_flags = flags;
        self
    }

    /// Layer a [`PathProvider`] on top of an existing engine.
    /// Overwrites any prior provider wholesale for the same reason as
    /// [`Self::with_command_types`]. The default is an empty
    /// `PathProvider`, so pre-M6 seed paths that never call this
    /// keep behaving as they did (zero path candidates rather than
    /// a shape error). Kept as a fluent builder so the R229 REPL
    /// start-up chain reads:
    ///
    /// ```ignore
    /// CompletionEngine::with_lists(cmds, vars)
    ///     .with_command_types(types)
    ///     .with_command_flags(flags)
    ///     .with_path_provider(paths)
    /// ```
    pub fn with_path_provider(mut self, p: PathProvider) -> Self {
        self.path_provider = p;
        self
    }
}

/// Record a completion selection into the engine's MRU history buffer.
///
/// Exposed as a free function (rather than a `&mut self` method on
/// `CompletionEngine`) so the caller pattern mirrors [`complete`] —
/// the R229 REPL wire-up passes `&mut engine` on selection and
/// `&engine` on every keystroke, and having both entry points as
/// crate-level fns keeps the API surface uniform. Delegates to
/// [`UsageHistory::record`] for MRU semantics (dup-remove-then-push-
/// front, then trim to capacity).
pub fn record_selection(engine: &mut CompletionEngine, text: &str) {
    engine.history.record(text.to_string());
}

/// Datalog reserved-word catalogue. Kept as a module-level constant so
/// M2's ranker can score against it without re-allocating the list on
/// every keystroke. The catalogue is small on purpose — Datalog at
/// R226 admits `not` (negation), `?` (query-mode marker; reserved as a
/// keyword-shaped completion so a user typing `n[TAB]` in Datalog
/// context sees `not` alongside identifier completions), and `$` (the
/// pipeline-value interpolation sigil). More keywords land as the
/// Datalog surface grows.
const DATALOG_KEYWORDS: &[&str] = &["not", "?", "$"];

/// The M1 completion entry point.
///
/// # Algorithm
///
/// 1. NFC-normalize `source[..cursor_byte]` — the "prefix". Downstream
///    byte offsets are relative to this normalized prefix; ASCII-only
///    inputs get a bit-identical copy.
/// 2. Tokenize the prefix via `paideia-as-shell-lex`.
/// 3. Locate the "active token": the last token whose byte span
///    contains or ends at the cursor. If no such token exists (empty
///    source, or the cursor sits on trailing whitespace / punctuation
///    that produced no tokens), the response is an insertion at the
///    cursor and candidates come from the "upcoming context" — at M1
///    that means: command candidates when the last non-whitespace
///    token is either absent, a `Pipe`, or a `Semi` (all mean "cursor
///    is at command position"); empty otherwise.
/// 4. Dispatch on the active token's `(kind, context)`:
///    * `Ident` in `Context::Pipeline` at command position -> Command
///      candidates ranked by [`matching::score_match`] against the
///      token text.
///    * `Ident` in `Context::Lambda` -> Var candidates from
///      `engine.known_vars` ranked by [`matching::score_match`].
///    * `Ident` in `Context::Datalog` -> Keyword candidates from
///      [`DATALOG_KEYWORDS`] ranked by [`matching::score_match`].
///    * Anything else -> empty candidates, insertion span at the
///      cursor.
/// 5. `prefix_start` / `prefix_end` are the active-token span; for the
///    "no active token" and "unhandled" branches, both equal
///    `cursor_byte` (an insertion).
///
/// R228.M3 landed the four-tier ranker (exact prefix > case-insensitive
/// prefix > subsequence, tie-broken by candidate length then alphabetic
/// text). R228.M4 layers a recency boost on top of the tier score via
/// [`history::UsageHistory::recency_boost`]; the sort order at the
/// response boundary remains `(score desc, text asc)`.
pub fn complete(engine: &CompletionEngine, req: &CompletionRequest) -> CompletionResponse {
    // ---- 1. Slice + normalize the pre-cursor prefix. ----------------
    // Defensive clamp: an out-of-range cursor or one that lands off a
    // UTF-8 boundary degrades to "no completions" rather than panicking.
    let cursor = req.cursor_byte.min(req.source.len());
    let raw_prefix = if req.source.is_char_boundary(cursor) {
        &req.source[..cursor]
    } else {
        // Not on a code-point boundary: return empty rather than
        // attempt a heuristic realignment. The REPL should never hit
        // this because its own cursor lives at a grapheme boundary,
        // but the completion crate must not panic on adversarial
        // input from a script test.
        return empty_at(cursor);
    };
    let prefix = nfc_normalize(raw_prefix);

    // ---- 2. Tokenize the prefix. ------------------------------------
    // We intentionally drop lex errors (`Err(_)`) — a malformed byte
    // partway through the buffer should still let the user complete
    // whatever they were typing at the cursor. The last successful
    // token is what drives dispatch.
    let tokens: Vec<Token> = Lexer::new(&prefix).filter_map(Result::ok).collect();

    let cursor = prefix.len(); // post-NFC cursor sits at end of prefix.

    // ---- 3. Locate the active token. --------------------------------
    // "Active" = last token whose span ends at or contains the cursor.
    // Because we tokenized `source[..cursor]`, this is simply the last
    // token whose `span.end == cursor` (i.e. the tokenizer consumed
    // right up to the cursor with no trailing whitespace) OR whose
    // span contains it. If the tokenizer stopped short (trailing
    // whitespace after the last token), there is no active token —
    // the cursor sits on whitespace and the request is an insertion.
    let active = tokens.iter().rposition(|t| t.span.end == cursor
        || (t.span.start < cursor && cursor < t.span.end));

    match active {
        // ---- 4a. No active token: insertion at cursor. -------------
        None => {
            // "Upcoming context" at M1: if the last non-whitespace
            // token is absent, a `Pipe`, or a `Semi`, the cursor is at
            // command position -- offer commands as an insertion.
            // Anything else -> empty (M3 will fill argument-position
            // candidates).
            let at_cmd_pos = match tokens.last() {
                None => true,
                Some(t) => matches!(t.kind, TokenKind::Pipe | TokenKind::Semi),
            };
            if at_cmd_pos {
                let cands = command_candidates(engine, "");
                CompletionResponse {
                    candidates: cands,
                    prefix_start: cursor,
                    prefix_end: cursor,
                }
            } else {
                empty_at(cursor)
            }
        }
        // ---- 4b. Active token: dispatch on (kind, context). --------
        Some(idx) => {
            // R228.M5 -- flag completion also short-circuits the token-
            // kind dispatch when the trailing tokens form a dash-in-
            // progress right of a command name. Runs BEFORE field
            // completion because a `-` token cannot participate in
            // either field shape (`Ident Dot ^` / `Ident Dot Ident^`)
            // but the field helper would otherwise fall through and
            // let the argument-position branch return empty — losing
            // the flag hit.
            if let Some(resp) = try_flag_completion(engine, &tokens, idx, cursor) {
                return resp;
            }
            // R228.M6 -- path completion short-circuits when the raw
            // pre-cursor bytes form a `/`-anchored path-in-progress.
            // Runs BEFORE field completion because a `Record.<cursor>`
            // shape never carries a leading `/` (the path detector
            // requires `bytes[start] == b'/'`) and the field detector
            // never triggers on an active `Op("/")` -- so ordering is
            // only load-bearing for a hypothetical future field shape
            // that would confuse the two. Runs AFTER flag completion
            // because a `-` typed inside a path is unusual enough that
            // a `command --path/segment` cursor should still classify
            // as a flag when the immediately-active token is the dash.
            if let Some(resp) = try_path_completion(engine, &prefix, cursor) {
                return resp;
            }
            // R228.M2 -- field completion short-circuits the token-
            // kind dispatch: a `Record.<cursor>` or `Record.pre<cursor>`
            // shape is recognized purely from the token neighbourhood,
            // independent of the surrounding sub-language context. If
            // the helper claims the request, its response wins;
            // otherwise fall through to the M1 (kind, context)
            // dispatch below.
            if let Some(resp) = try_field_completion(engine, &tokens, idx, cursor) {
                return resp;
            }
            let tok = &tokens[idx];
            match (&tok.kind, tok.context) {
                (TokenKind::Ident(name), Context::Pipeline) => {
                    if is_command_position(&tokens, idx) {
                        let cands = command_candidates(engine, name);
                        response(cands, tok.span)
                    } else {
                        // Argument position -- M3's job. M1 emits no
                        // candidates but still names the token's span
                        // as the overwrite range so a future M3 wire-
                        // up can slot in without a shape change.
                        empty_at(cursor)
                    }
                }
                (TokenKind::Ident(name), Context::Lambda) => {
                    let cands = var_candidates(engine, name);
                    response(cands, tok.span)
                }
                (TokenKind::Ident(name), Context::Datalog) => {
                    let cands = keyword_candidates(engine, name);
                    response(cands, tok.span)
                }
                _ => empty_at(cursor),
            }
        }
    }
}

/// Whether the token at `idx` sits at "command position" — the first
/// non-whitespace token of its pipeline stage. In the R221.M4 token
/// stream, whitespace is not emitted, so command position is: the
/// token is the first token overall, OR the immediately-preceding
/// token is a `Pipe`, `Semi`, or `Newline`.
fn is_command_position(tokens: &[Token], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    matches!(
        tokens[idx - 1].kind,
        TokenKind::Pipe | TokenKind::Semi | TokenKind::Newline
    )
}

/// Score-filter `engine.commands` via [`score_match`] and wrap each hit
/// as a `Command` candidate. Type-hint enrichment (R228.M2): if
/// `engine.commands_with_types` carries an entry for the command name,
/// its value populates the candidate's `type_hint`; otherwise
/// `type_hint` stays `None`. Sorted by `(score desc, text asc)` at
/// return so the caller can concatenate multiple candidate pools
/// without a re-sort.
fn command_candidates(engine: &CompletionEngine, prefix: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = engine
        .commands
        .iter()
        .filter_map(|c| {
            score_match(prefix, c).map(|base_score| Candidate {
                text: c.clone(),
                kind: CandidateKind::Command,
                display: None,
                type_hint: engine.commands_with_types.get(c).cloned(),
                // R228.M4 -- recency boost layered on top of the M3
                // tier score. See `history::UsageHistory::recency_boost`
                // for the formula; 0 for entries not in the buffer, so
                // an empty history leaves M3 ordering untouched.
                score: base_score + engine.history.recency_boost(c),
            })
        })
        .collect();
    sort_by_score_then_text(&mut out);
    out
}

/// Score-filter `engine.known_vars` via [`score_match`] and wrap each
/// hit as a `Var` candidate. Var type-hint enrichment lands with
/// R228.M4's scope-aware pass; M3 leaves `type_hint` as `None`.
fn var_candidates(engine: &CompletionEngine, prefix: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = engine
        .known_vars
        .iter()
        .filter_map(|v| {
            score_match(prefix, v).map(|base_score| Candidate {
                text: v.clone(),
                kind: CandidateKind::Var,
                display: None,
                type_hint: None,
                score: base_score + engine.history.recency_boost(v),
            })
        })
        .collect();
    sort_by_score_then_text(&mut out);
    out
}

/// Score-filter [`DATALOG_KEYWORDS`] via [`score_match`] and wrap each
/// hit as a `Keyword` candidate.
fn keyword_candidates(engine: &CompletionEngine, prefix: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = DATALOG_KEYWORDS
        .iter()
        .filter_map(|kw| {
            score_match(prefix, kw).map(|base_score| Candidate {
                text: (*kw).to_owned(),
                kind: CandidateKind::Keyword,
                display: None,
                type_hint: None,
                score: base_score + engine.history.recency_boost(kw),
            })
        })
        .collect();
    sort_by_score_then_text(&mut out);
    out
}

/// Score-filter `engine.records[rec_name]` via [`score_match`] and wrap
/// each hit as a `Field` candidate. Returns an empty vector when the
/// record name is not in the map (the caller may still return a
/// positioned response).
fn field_candidates(engine: &CompletionEngine, rec_name: &str, prefix: &str) -> Vec<Candidate> {
    let Some(fields) = engine.records.get(rec_name) else {
        return Vec::new();
    };
    let mut out: Vec<Candidate> = fields
        .iter()
        .filter_map(|f| {
            score_match(prefix, f).map(|base_score| Candidate {
                text: f.clone(),
                kind: CandidateKind::Field,
                display: None,
                type_hint: None,
                score: base_score + engine.history.recency_boost(f),
            })
        })
        .collect();
    sort_by_score_then_text(&mut out);
    out
}

/// Sort a candidate vector in place by `(score desc, text asc)`.
///
/// This is the R228.M3 total order guaranteed at every
/// [`CompletionResponse`] boundary. Extracted so every candidate-
/// emitting helper (command/var/keyword/field) sorts through one
/// implementation; if a future tier introduces a secondary sort key
/// (recency at M4+), this is the sole place to change.
fn sort_by_score_then_text(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.text.cmp(&b.text)));
}

/// R228.M2 -- attempt Field completion off the token neighbourhood.
///
/// Recognized shapes (`^` marks the cursor):
///
/// * `Ident Dot ^`       — insertion at cursor, all fields of the
///                          record named by the leading `Ident`.
/// * `Ident Dot Ident^`  — overwrite the trailing `Ident`'s span,
///                          filter fields by its text.
///
/// The helper deliberately keeps its shape-recognition detached from
/// the [`Context`] tag: `.` is a field-access glyph in `Pipeline` and
/// a fact terminator in `Datalog`, but at R228.M2 the caller only
/// asks for field completion when the record catalogue is non-empty
/// and the token neighbourhood matches — the Datalog surface never
/// hands the engine a records map today, so the crossover is inert
/// in practice.
///
/// Nested lookup (`Ident Dot Ident Dot ^` or
/// `Ident Dot Ident Dot Ident^`) is deferred to R228.M4: the helper
/// still *claims* the request (returning an empty response) so a
/// caller does not fall through to the argument-position empty
/// branch and produce misleading candidates in the interim.
///
/// Returns `None` when the neighbourhood does not match a field
/// shape at all; then the caller continues the M1 token-kind
/// dispatch.
fn try_field_completion(
    engine: &CompletionEngine,
    tokens: &[Token],
    active_idx: usize,
    cursor: usize,
) -> Option<CompletionResponse> {
    // Case A: `Ident Dot ^` -- active token is the Dot.
    if matches!(tokens[active_idx].kind, TokenKind::Dot) {
        if active_idx == 0 {
            return None;
        }
        let TokenKind::Ident(rec_name) = &tokens[active_idx - 1].kind else {
            return None;
        };
        // Nested guard: the Ident we would consult is itself a field
        // access (`prev.rec.<cursor>`). M4's resolver will pick this
        // up; M2 returns an empty insertion so the caller does not
        // fall through to a wrong dispatch branch.
        if active_idx >= 2 && matches!(tokens[active_idx - 2].kind, TokenKind::Dot) {
            return Some(empty_at(cursor));
        }
        let cands = field_candidates(engine, rec_name, "");
        return Some(CompletionResponse {
            candidates: cands,
            prefix_start: cursor,
            prefix_end: cursor,
        });
    }
    // Case B: `Ident Dot Ident^` -- active token is the trailing
    // Ident. Requires at least two preceding tokens.
    if let TokenKind::Ident(field_prefix) = &tokens[active_idx].kind {
        if active_idx >= 2 && matches!(tokens[active_idx - 1].kind, TokenKind::Dot) {
            let TokenKind::Ident(rec_name) = &tokens[active_idx - 2].kind else {
                return None;
            };
            // Nested guard: the record ident is itself a field access.
            if active_idx >= 3 && matches!(tokens[active_idx - 3].kind, TokenKind::Dot) {
                return Some(empty_at(cursor));
            }
            let cands = field_candidates(engine, rec_name, field_prefix);
            return Some(response(cands, tokens[active_idx].span));
        }
    }
    None
}

/// R228.M6 -- attempt Path completion off the raw pre-cursor bytes.
///
/// # Detection
///
/// Walk `prefix` backwards from `cursor`, consuming characters that
/// can legally appear inside a filesystem path segment (ASCII
/// alphanumerics plus `/ . - _ ~`). Stop at the first non-path char
/// (typically whitespace or a shell metacharacter) or at the buffer
/// start. If the resulting range `prefix[start..cursor]` is non-
/// empty AND begins with `/`, it is a path-in-progress and this
/// helper claims the request.
///
/// The byte-scan is deliberately independent of the token stream:
/// the shell lexer emits `/` as `Op("/")` and splits `/usr/local`
/// into `Op("/"), Ident("usr"), Op("/"), Ident("local")`, so a
/// token-neighbourhood match would need one arm per shape (leading
/// slash, trailing slash, mid-segment). The raw-byte scan captures
/// all four M6 fixture shapes in one pass and stays right when the
/// tokenizer's segmentation rules evolve.
///
/// # Path split + lookup
///
/// The path text is split on its last `/`: everything up to (and
/// including) that slash names the directory; everything after is
/// the name prefix the popup filters by. The directory string is
/// then normalized -- the root stays `"/"`, every other directory
/// drops its trailing slash -- and passed to
/// [`PathProvider::list`]. The provider's returned names are score-
/// filtered via [`score_match`] against the name prefix (with the
/// same recency boost every other candidate emitter applies), then
/// wrapped as [`CandidateKind::Path`].
///
/// # Missing directory
///
/// A path prefix whose directory is absent from the provider still
/// causes the helper to claim the request -- it returns
/// `Some(response)` with an empty candidate list. Falling through
/// to the M1 argument-position empty branch would yield the same
/// empty result today, but claiming here keeps the "cursor is
/// inside a path" signal available to a future
/// argument-position enrichment pass that might otherwise misroute.
///
/// # Overwrite span
///
/// `prefix_start` names the byte immediately after the last `/` in
/// the path text (so the completion overwrites just the name
/// prefix, never the directory portion the user already typed);
/// `prefix_end` is the cursor. For a bare trailing slash
/// (`/usr/^`), `prefix_start == prefix_end == cursor` -- the
/// completion is an insertion.
///
/// Returns `None` when the pre-cursor bytes do not shape as a
/// `/`-anchored path; the caller then falls through to the M2 field
/// dispatch.
fn try_path_completion(
    engine: &CompletionEngine,
    prefix: &str,
    cursor: usize,
) -> Option<CompletionResponse> {
    // Walk backwards through the raw bytes; every byte we accept is
    // ASCII by construction (the path-legal set is a strict subset of
    // ASCII), so a byte-level cursor never lands mid-code-point.
    let bytes = prefix.as_bytes();
    let mut start = cursor;
    while start > 0 && is_path_char(bytes[start - 1]) {
        start -= 1;
    }
    // Path-in-progress must be non-empty AND anchored at `/`.
    if start >= cursor || bytes[start] != b'/' {
        return None;
    }
    let path_text = &prefix[start..cursor];

    // Split on the last `/`. `path_text` starts with `/`, so `rfind`
    // always succeeds.
    let last_slash = path_text.rfind('/').expect("path text starts with `/`");
    let raw_dir = &path_text[..=last_slash]; // includes trailing `/`
    let name_prefix = &path_text[last_slash + 1..];

    // Normalize directory: the root stays `"/"`; every other path
    // drops its trailing slash so the seed shape and query shape
    // agree with [`PathProvider`]'s stored key convention.
    let dir = if raw_dir == "/" {
        "/"
    } else {
        &raw_dir[..raw_dir.len() - 1]
    };

    let entries = engine.path_provider.list(dir);
    let mut cands: Vec<Candidate> = entries
        .iter()
        .filter_map(|name| {
            score_match(name_prefix, name).map(|base_score| Candidate {
                text: name.clone(),
                kind: CandidateKind::Path,
                display: None,
                type_hint: None,
                // Recency boost is keyed on the bare child name, not
                // the joined full path -- the M6 REPL wire-up records
                // selection texts as the emitter shipped them, and the
                // emitter ships bare names.
                score: base_score + engine.history.recency_boost(name),
            })
        })
        .collect();
    sort_by_score_then_text(&mut cands);
    Some(CompletionResponse {
        candidates: cands,
        prefix_start: start + last_slash + 1,
        prefix_end: cursor,
    })
}

/// Whether an ASCII byte can legally appear inside a filesystem path
/// segment for M6's detection purposes. Includes the separator `/`
/// itself so the scan crosses segment boundaries; a caller reading
/// `raw` after this has to re-split on `/` to recover segments.
///
/// The set is deliberately conservative -- shell metacharacters
/// (`$`, `*`, `?`, `[`, `~` after position 0) that a real filesystem
/// accepts inside a filename are excluded so the detector does not
/// grab a glob or a pipeline-value interpolation whose bytes happen
/// to abut a path-like prefix. A shell-metacharacter-aware split
/// belongs at the R228.M7+ argument-position layer.
fn is_path_char(b: u8) -> bool {
    matches!(
        b,
        b'/' | b'.'
            | b'-'
            | b'_'
            | b'~'
            | b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
    )
}

/// R228.M5 -- attempt Flag completion off the token neighbourhood.
///
/// Recognized shapes (`^` marks the cursor, `-` is the [`TokenKind::Op`]
/// glyph the shell-lex layer emits one dash at a time):
///
/// | Shape                        | Mode  | Prefix (bare) | Overwrite span            |
/// |------------------------------|-------|---------------|---------------------------|
/// | `Op("-") ^`                  | short | `""`          | Op span                   |
/// | `Op("-") Op("-") ^`          | long  | `""`          | first Op start .. cursor  |
/// | `Op("-") Ident(name)^`       | short | `name`        | Op start .. Ident end     |
/// | `Op("-") Op("-") Ident^`     | long  | `name`        | first Op start .. Ident end |
///
/// The helper requires a resolvable command name to the left of the
/// dash pattern (an [`Ident`] at command position — first token, or
/// preceded by [`TokenKind::Pipe`] / [`TokenKind::Semi`] /
/// [`TokenKind::Newline`]) AND that name to carry a
/// [`CommandFlags`] entry in [`CompletionEngine::command_flags`]. When
/// either lookup fails the helper still *claims* the request with an
/// empty response — the alternative (falling through to the M1 argument-
/// position empty branch) is identical in candidate count today but
/// would misroute a future argument-position handler through a token
/// stream whose active token is clearly a flag.
///
/// The dash-first shape is checked with the raw token kinds rather
/// than by re-slicing `req.source` because the tokenizer already
/// normalized the byte range (NFC + boundary check) and re-slicing
/// would duplicate that work.
///
/// Returns `None` when the neighbourhood does not shape as a dash-in-
/// progress at all; then the caller continues the M1/M2 dispatch.
fn try_flag_completion(
    engine: &CompletionEngine,
    tokens: &[Token],
    active_idx: usize,
    cursor: usize,
) -> Option<CompletionResponse> {
    // Classify the trailing pattern; `cmd_scan_upto` names the token
    // index BEFORE the flag pattern so [`find_command_at_position`]
    // can walk back from there.
    let shape = classify_flag_shape(tokens, active_idx)?;

    // Command must resolve, else there is no per-command catalogue to
    // consult. A missing catalogue is still a claim (empty response)
    // per the doc-header rationale, but a missing command name is a
    // no-claim (the bare `-` at start-of-line reads as a subtraction
    // operator the future arithmetic-expression branch might want).
    let cmd_name = find_command_at_position(tokens, shape.cmd_scan_upto)?;

    let cands = match engine.command_flags.get(cmd_name) {
        Some(cf) => flag_candidates(engine, cf, shape.is_long, &shape.bare_prefix),
        None => Vec::new(),
    };
    Some(CompletionResponse {
        candidates: cands,
        prefix_start: shape.overwrite_start,
        prefix_end: cursor,
    })
}

/// Tuple result of [`classify_flag_shape`]. Kept as a named struct so a
/// future case ("cursor sits mid-flag with an escape glyph") can grow
/// a field without every call-site's tuple destructure churning.
struct FlagShape {
    is_long: bool,
    bare_prefix: String,
    overwrite_start: usize,
    cmd_scan_upto: usize,
}

/// Match the trailing token pattern against the four flag shapes.
/// Returns `None` when no shape applies.
fn classify_flag_shape(tokens: &[Token], active_idx: usize) -> Option<FlagShape> {
    let active = &tokens[active_idx];

    // Case A/B -- active is `Op("-")`. Distinguish short vs. long by
    // whether the immediately-preceding token is another `Op("-")`.
    if is_dash_op(&active.kind) {
        // A single `-` at token index 0 could be a subtraction operator
        // typed at command position; the "command name to the left"
        // requirement below then fails and no flag candidates fire,
        // which is the correct fallback. So we do not gate on
        // `active_idx > 0` here.
        let prev_dash = active_idx > 0 && is_dash_op(&tokens[active_idx - 1].kind);
        if prev_dash {
            // Long, empty prefix: first Op is at active_idx - 1.
            let first_dash = &tokens[active_idx - 1];
            return Some(FlagShape {
                is_long: true,
                bare_prefix: String::new(),
                overwrite_start: first_dash.span.start,
                cmd_scan_upto: active_idx - 1,
            });
        }
        // Short, empty prefix.
        return Some(FlagShape {
            is_long: false,
            bare_prefix: String::new(),
            overwrite_start: active.span.start,
            cmd_scan_upto: active_idx,
        });
    }

    // Case C/D -- active is `Ident(name)` preceded by one or two dash
    // Ops. Long mode requires TWO preceding dashes; short mode requires
    // exactly one (and the token before that is NOT another dash — a
    // triple-dash typed by mistake reads as long-mode empty and drops
    // the ident, which we defer to the caller's argument-position
    // handler).
    if let TokenKind::Ident(name) = &active.kind {
        if active_idx >= 2
            && is_dash_op(&tokens[active_idx - 1].kind)
            && is_dash_op(&tokens[active_idx - 2].kind)
        {
            let first_dash = &tokens[active_idx - 2];
            // Guard against `Op Op Op Ident` (triple-dash + ident): the
            // three-dash prefix is ill-formed so we do not claim it.
            if active_idx >= 3 && is_dash_op(&tokens[active_idx - 3].kind) {
                return None;
            }
            return Some(FlagShape {
                is_long: true,
                bare_prefix: name.clone(),
                overwrite_start: first_dash.span.start,
                cmd_scan_upto: active_idx - 2,
            });
        }
        if active_idx >= 1 && is_dash_op(&tokens[active_idx - 1].kind) {
            // Ensure we are not the middle of a long-mode shape whose
            // second dash was actually the same token as ours (impossible
            // by kind, but a guard against a future `Op("--")` glued
            // variant). Ident-with-one-preceding-dash is short mode.
            let first_dash = &tokens[active_idx - 1];
            return Some(FlagShape {
                is_long: false,
                bare_prefix: name.clone(),
                overwrite_start: first_dash.span.start,
                cmd_scan_upto: active_idx - 1,
            });
        }
    }
    None
}

/// Whether a token kind is a single-dash `Op("-")`. Kept as its own
/// helper so a future shell-lex glue that emits `Op("--")` (two-char
/// dash) can be added at one site rather than open-coded across the
/// flag helpers.
fn is_dash_op(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Op(s) if s == "-")
}

/// Walk backwards through `tokens[..upto]` and return the text of the
/// first [`TokenKind::Ident`] that sits at command position (per
/// [`is_command_position`]). Returns `None` when no such ident exists —
/// e.g. a bare `-` typed at the very start of a fresh REPL line.
fn find_command_at_position(tokens: &[Token], upto: usize) -> Option<&str> {
    for i in (0..upto).rev() {
        if let TokenKind::Ident(name) = &tokens[i].kind {
            if is_command_position(tokens, i) {
                return Some(name.as_str());
            }
        }
    }
    None
}

/// Score-filter a [`CommandFlags`] against `bare_prefix` and wrap each
/// hit as a `Flag` candidate. Only the flag list matching `is_long` is
/// consulted; the emitter reintroduces the leading dash(es) so the
/// candidate `text` is the on-screen form (`-a`, `--long`).
///
/// Score comes from [`score_match`] applied to `(bare_prefix, bare_name)`
/// — matching on the bare name (not the dash-prefixed form) so the
/// score reflects the ranker's tier judgement about the letters the
/// user actually typed after the dashes. The candidate's `score`
/// still receives the standard recency boost keyed on the final
/// `text` (`-a` / `--long`), so a repeat selection floats.
fn flag_candidates(
    engine: &CompletionEngine,
    cf: &CommandFlags,
    is_long: bool,
    bare_prefix: &str,
) -> Vec<Candidate> {
    let (source_list, prefix_glyph) = if is_long {
        (&cf.long, "--")
    } else {
        (&cf.short, "-")
    };
    let mut out: Vec<Candidate> = source_list
        .iter()
        .filter_map(|name| {
            score_match(bare_prefix, name).map(|base_score| {
                let text = format!("{prefix_glyph}{name}");
                let recency = engine.history.recency_boost(&text);
                Candidate {
                    text,
                    kind: CandidateKind::Flag,
                    display: None,
                    type_hint: cf.descriptions.get(name).cloned(),
                    score: base_score + recency,
                }
            })
        })
        .collect();
    sort_by_score_then_text(&mut out);
    out
}

/// Build a `CompletionResponse` whose overwrite span is `tok_span`.
fn response(candidates: Vec<Candidate>, tok_span: Span) -> CompletionResponse {
    CompletionResponse {
        candidates,
        prefix_start: tok_span.start,
        prefix_end: tok_span.end,
    }
}

/// Build an empty `CompletionResponse` positioned as an insertion at
/// `cursor`. Used for the "no active token" and "unhandled dispatch"
/// branches so the REPL can still uniformly consume the response
/// shape.
fn empty_at(cursor: usize) -> CompletionResponse {
    CompletionResponse {
        candidates: Vec::new(),
        prefix_start: cursor,
        prefix_end: cursor,
    }
}
