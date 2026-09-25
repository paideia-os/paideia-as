//! paideia-as-shell-datalog — R226.M1 (AST + parser) + R226.M2
//! (seminaïve fixpoint evaluator) for the semantic shell's embedded
//! Datalog sub-language.
//!
//! # Position in the pipeline
//!
//! ```text
//!     source
//!       │
//!       ▼
//!   paideia-as-shell-lex ── Vec<Token>  (Context::Datalog slice)
//!       │
//!       ▼
//!   parser::parse_block  ── Program { rules, facts }
//!   parser::parse_query  ── Query   { goals }
//!       │
//!       ▼
//!   eval::Database        ── seminaïve fixpoint
//!       │
//!       ▼
//!   eval::query          ── Vec<Binding>
//! ```
//!
//! Consumes the `Context::Datalog` slice of a token stream (the caller
//! has already stripped the `datalog { … }` wrapper — the parser
//! deliberately does not know about `DatalogKw`/`LBrace`/`RBrace`, so
//! it can also drive stand-alone `.dl` script files that omit the
//! wrapper).
//!
//! # Concrete syntax accepted at M1
//!
//! * **Fact** — a ground atom terminated by `.`:
//!   `parent(alice, bob).`
//! * **Rule** — head, `=>`, comma-separated body, terminated by `.`:
//!   `ancestor(?X, ?Y) => parent(?X, ?Y).`
//!   `ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).`
//! * **Query** (parsed separately via `parse_query`) — comma-separated
//!   goal atoms, no trailing dot:
//!   `ancestor(alice, ?Z)`
//!
//! `head => body` is the shell's spelling of Prolog's `head :- body`
//! (the lexer emits `=>` as `TokenKind::FatArrow`; there is no
//! `TokenKind::Colon`, so the traditional `:-` glyph is unavailable at
//! the lexical layer). Positional convention matches `:-`: head first,
//! body after.
//!
//! # Evaluator (M2)
//!
//! Seminaïve bottom-up fixpoint per Abiteboul-Hull-Vianu §13. Each
//! iteration:
//!
//! 1. For each rule `H :- B1, B2, …, Bn`, for each body position `i`:
//!    match `B[i]` against the *previous round's delta* and every
//!    other `B[j]` against the *full database* (including that delta).
//! 2. For each substitution σ satisfying the body, add σ(H) to the
//!    round's new delta (skipping duplicates already in the DB).
//! 3. Merge the new delta into the DB; if it was empty, return —
//!    fixpoint reached.
//!
//! The seminaïve pivot ensures every derivation of round `k` uses at
//! least one round-`k-1` tuple, so no re-derivation from the base EDB
//! is repeated across rounds. On the small M1/M2 test corpus this
//! matters little; on the 1M-tuple graph queries R226.M4 targets, it
//! makes a > 100x difference before magic-set rewriting even enters
//! the picture.
//!
//! # Fingerprints
//!
//! The test corpus tags each fixture with `r226-m1-NN` (parser) or
//! `r226-m2-NN` (evaluator) so the R220.M10 `@fingerprint` correlator
//! can attribute pass/fail to a specific fixture without re-parsing
//! its name.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod aggregation;
pub mod ast;
pub mod eval;
pub mod fingerprint;
pub mod magic_sets;
pub mod parser;
pub mod session_edb;
pub mod stratification;

pub use aggregation::{AggregateResult, AggregationError};
pub use ast::{Aggregate, AggregateQuery, Atom, BodyGoal, Program, Query, Rule, Term, Value};
pub use eval::{query, Binding, Database, EvalError, Evaluator};
pub use fingerprint::{CollectingSink, FingerprintSink, NullSink, QueryId};
pub use parser::{parse_aggregate_query, parse_block, parse_query, ParseError, ParseErrorKind};
pub use session_edb::{AssertResult, RetractResult, SessionEdb};
pub use stratification::{compute_strata, StratificationError};
