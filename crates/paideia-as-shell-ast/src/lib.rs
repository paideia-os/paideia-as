//! paideia-as-shell-ast — R221.M5 (unified AST) + R221.M6 (span
//! provenance across NFC) + R221.M7 (parser + Unicode fuzz corpus).
//!
//! # In one paragraph
//!
//! The semantic shell composes three sub-languages *lexically* — the
//! R221.M4 lexer stamps every token with its sub-language context
//! (Pipeline / Datalog / Lambda). This crate lifts that stream into a
//! single [`SyntaxNode`] enum, driven by a top-down recursive-descent
//! parser that dispatches to a per-context sub-parser without any
//! extra look-ahead. Downstream (R222 functor surface, R225 unified
//! HM checker, R226 Datalog evaluator) works over one AST vocabulary
//! — the design decision at `design/terminal/semantic-shell-language-
//! plan.md` §4 R221.M5.
//!
//! # Q-A5 invariant: every node carries a source span
//!
//! Every `SyntaxNode` embeds a [`NodeSpan`] with three fields:
//! * `original: (usize, usize)` — byte range into the *pre-NFC* source
//!   the user typed. Diagnostics and R229 REPL underlines resolve this
//!   so squigglies land on the user's own bytes.
//! * `nfc: (usize, usize)` — byte range into the *post-NFC* source the
//!   parser saw. Used for lookahead + inter-node comparison.
//! * `context: Context` — sub-language grammar the node was parsed
//!   under. R229's syntax colorer keys off this without re-running
//!   the parser.
//!
//! # R221.M6 span provenance
//!
//! The [`NfcMap`] type records the pre-NFC ↔ post-NFC byte-boundary
//! correspondence built alongside normalization. For an ASCII-only
//! input (the fast path), the map is identity and consumes zero extra
//! memory. For a mixed input, checkpoints are recorded at each
//! canonical-starter boundary (per UAX#15 §3.11) and interior points
//! are snapped to the surrounding checkpoint pair — a conservative,
//! never-narrower mapping that keeps LSP squigglies covering at least
//! the user's typed text.
//!
//! # R221.M7 fuzz corpus
//!
//! `tests/parser_fuzz_proptest.rs` runs ~1000 randomly-generated
//! inputs through [`parse`] and asserts two invariants:
//! 1. The parser never panics — every error is a `ParseError`.
//! 2. The result is `Ok(SyntaxNode)` XOR `Err(ParseError)`; never both.
//!
//! Coverage: malformed UTF-8 bytes (guarded via the unicode substrate),
//! orphan combining marks (malformed NFC), deep-nested `{`
//! adversarial trees, and random context-switch sequences.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod ast;
pub mod nfc_map;
pub mod parser;
pub mod pretty;
pub mod span;

pub use ast::{MatchArm, RecordField, RedirectKind, SyntaxNode};
pub use nfc_map::NfcMap;
pub use parser::{ParseError, ParseErrorKind, parse, parse_with_map};
pub use pretty::pretty_print;
pub use span::NodeSpan;

// Re-export the lexer's `Context` — many downstream consumers want it
// for pattern matches on `NodeSpan::context` and shouldn't have to
// depend on `paideia-as-shell-lex` themselves.
pub use paideia_as_shell_lex::Context;
