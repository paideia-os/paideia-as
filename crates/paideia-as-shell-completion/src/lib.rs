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
//! * M2 — ranking (prefix > substring > subsequence; case-fold; recency).
//! * M3 — argument-position completion for the running command.
//! * M4 — field-name completion off a schema-typed record cursor.
//! * M5 — async / streaming candidate providers.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use paideia_as_shell_lex::{Context, Lexer, Span, Token, TokenKind};
use paideia_as_unicode::nfc_normalize;

// R228.M4 will consume `paideia_as_shell_ast::SyntaxNode` to resolve
// the record type behind a `.` cursor. Held as a no-name import here
// so the crate's Cargo.toml dep list is stable across the M1..M4
// landings; the actual `use paideia_as_shell_ast::SyntaxNode` will
// land at M4 with the field-completion module. The `_` binding is
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
    /// A record field. Reserved for R228.M4 (field completion after a
    /// `.` cursor). Not emitted at M1.
    Field,
    /// A reserved word of the sub-language grammar. Emitted for the
    /// Datalog keyword catalogue in `Context::Datalog`.
    Keyword,
    /// A filesystem path element. Reserved for R228.M3 (argument-
    /// position path expansion). Not emitted at M1.
    Path,
    /// An argument value the running command declared. Reserved for
    /// R228.M3. Not emitted at M1.
    Argument,
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
    /// Optional human-facing render — a description, disambiguating
    /// suffix, or type annotation. `None` means "render `text` alone".
    /// M1 never populates this; the field is present so R228.M2's
    /// ranker can annotate ambiguous matches without a shape change.
    pub display: Option<String>,
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
    /// The candidate list. Ranking is `starts_with`-only at M1; M2
    /// introduces prefix > substring > subsequence with recency
    /// weighting.
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
    /// them (dispatch-registry insertion order today; ranked by M2's
    /// scorer at the next milestone).
    pub commands: Vec<String>,
    /// Known variable names in the current session scope. M1 populates
    /// this from the REPL's session-wide `let`-binding table; a lambda-
    /// local shadowing pass lands with M4's scope-aware completion.
    pub known_vars: Vec<String>,
}

impl CompletionEngine {
    /// Construct an engine with an empty catalogue. Useful for the
    /// zero-catalogue smoke test and for REPL startup before the
    /// dispatch registry is loaded.
    pub fn empty() -> Self {
        Self {
            commands: Vec::new(),
            known_vars: Vec::new(),
        }
    }

    /// Construct an engine pre-seeded with a command and variable list.
    /// Convenience for the M1 fixtures and for the R229 REPL boot
    /// path.
    pub fn with_lists(commands: Vec<String>, known_vars: Vec<String>) -> Self {
        Self { commands, known_vars }
    }
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
///      candidates matching the token text with `starts_with`.
///    * `Ident` in `Context::Lambda` -> Var candidates from
///      `engine.known_vars` matching `starts_with`.
///    * `Ident` in `Context::Datalog` -> Keyword candidates from
///      [`DATALOG_KEYWORDS`] matching `starts_with`.
///    * Anything else -> empty candidates, insertion span at the
///      cursor.
/// 5. `prefix_start` / `prefix_end` are the active-token span; for the
///    "no active token" and "unhandled" branches, both equal
///    `cursor_byte` (an insertion).
///
/// Ranking beyond ASCII case-sensitive `starts_with` is R228.M2's job.
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
                    let cands = keyword_candidates(name);
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

/// Filter `engine.commands` to entries starting with `prefix` (case-
/// sensitive) and wrap each as a `Command` candidate.
fn command_candidates(engine: &CompletionEngine, prefix: &str) -> Vec<Candidate> {
    engine
        .commands
        .iter()
        .filter(|c| c.starts_with(prefix))
        .map(|c| Candidate {
            text: c.clone(),
            kind: CandidateKind::Command,
            display: None,
        })
        .collect()
}

/// Filter `engine.known_vars` to entries starting with `prefix` and
/// wrap each as a `Var` candidate.
fn var_candidates(engine: &CompletionEngine, prefix: &str) -> Vec<Candidate> {
    engine
        .known_vars
        .iter()
        .filter(|v| v.starts_with(prefix))
        .map(|v| Candidate {
            text: v.clone(),
            kind: CandidateKind::Var,
            display: None,
        })
        .collect()
}

/// Filter [`DATALOG_KEYWORDS`] to entries starting with `prefix` and
/// wrap each as a `Keyword` candidate.
fn keyword_candidates(prefix: &str) -> Vec<Candidate> {
    DATALOG_KEYWORDS
        .iter()
        .filter(|kw| kw.starts_with(prefix))
        .map(|kw| Candidate {
            text: (*kw).to_owned(),
            kind: CandidateKind::Keyword,
            display: None,
        })
        .collect()
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
