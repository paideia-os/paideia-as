//! paideia-as-reflection — R220.M1 elaborator reflection surface.
//!
//! This crate lands the typed `Syntax` value, the `HygienicId` substrate,
//! the structural walker API (`WalkAction` / `SyntaxWalker`), and the
//! `Elab` effect's operation signatures — the substrate hosted DSLs use
//! to hook themselves into the elaborator per Christiansen & Brady 2016
//! ("Elaborator Reflection: Extending Idris in Idris"). It supersedes the
//! `MP-D5` DEFERRED gate in `design/toolchain/macros-phase1.md` and gates
//! R220.M2 (hygiene wiring), R220.M3 (`@dsl_parser`), R220.M9 (LSP embed),
//! R220.M10 (`@fingerprint`) plus semantic-shell rounds R221..R227.
//!
//! # What this crate is (and is not)
//!
//! `Syntax` is a thin **opaque** wrapper around an existing surface-AST
//! `Term` handle (see `paideia_as_ast::reflect::Term`). Rather than
//! duplicate the AST, the reflection surface reuses the arena the parser
//! already produces. Constructors (`literal`, `var`, `app`, `let_`,
//! `lambda`, `match_`) synthesize new nodes in a supplied arena;
//! deconstruction accessors (`head_kind`, `children`) read them back.
//! Every constructed node carries a span (real or synthesized) and,
//! optionally, a `HygienicId` — the alpha-rename seat R220.M2 will grow
//! into the Ullrich 2020 hygiene pass.
//!
//! `Elab` is a paideia-as **effect row entry** (already registered by
//! `paideia_as_effects::EffectRegistry::register_macro_effects` for the
//! macro-body contract; this crate extends it with the four R220.M1
//! operations and provides the Rust-side error/warn/spec helpers a
//! hosted DSL implementor consumes).
//!
//! # Scope cap (per issue #1415)
//!
//! This is deliberately the **minimal usable surface**. Structural
//! pattern-matching on `Syntax`, in-place walker transformation
//! (`WalkAction::Replace` beyond the trivial identity path), quote/anti-
//! quote of types and effect rows (as opposed to just expressions), and
//! macro-generated modules are all FIXME'd for follow-on rounds.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod elab_effect;
pub mod hygiene;
pub mod syntax;
pub mod walker;

pub use elab_effect::{
    ELAB_MAX_QUOTE_DEPTH, ElabError, ElabOpKind, ElabOpSignatures, ElabWarn,
    F_ELAB_ERROR, F_ELAB_QUOTE_DEPTH_EXCEEDED, F_ELAB_TYPE_UNAVAILABLE,
    elab_error_code, elab_op_signatures, elab_operation_names,
};
pub use hygiene::{HYGIENIC_ID_UNTAGGED, HygienicId, fresh_hygienic_id};
pub use syntax::{Syntax, SyntaxHead, SyntaxKind, quote_depth};
pub use walker::{SyntaxWalker, WalkAction, walk_syntax};
