//! R226.M8 — session-local extensional database.
//!
//! # Position in the pipeline
//!
//! ```text
//!     Program (rules + program facts)          SessionEdb (per-turn facts)
//!            │                                          │
//!            └──────────── merged EDB ──────────────────┘
//!                                │
//!                                ▼
//!                      seminaïve fixpoint (stratified)
//!                                │
//!                                ▼
//!                             query
//! ```
//!
//! A session EDB is an additive overlay on top of a program's own
//! extensional facts: everything the user has `assert`-ed since the
//! REPL started (or since the last `clear`) is seeded into the DB
//! alongside `Program::facts` before the fixpoint runs. Session facts
//! propagate through IDB rules exactly the way program facts do — the
//! evaluator draws no distinction between the two once seeded.
//!
//! # Why an overlay rather than a mutable Program
//!
//! A `Program` mirrors what the user typed in the current `datalog
//! { … }` block or `.pds` script. Copying rewritten variants of it into
//! `Program::facts` every time a REPL turn asserts a new tuple would
//! (a) make the AST cache useless, (b) lose the provenance distinction
//! between compiled program facts and interactive session facts, and
//! (c) force the parser layer to reshape a tuple back into `Atom`
//! terms only for the seeder to unpack them again. The overlay keeps
//! the two layers cleanly separated: `Program` is *this file's
//! declarations*, `SessionEdb` is *the current shell's live state*.
//!
//! # Parser-layer scope (deliberately narrowed at M8)
//!
//! The R226.M8 issue mentions extending `parse_command` for
//! `assert p(a,b,c).` / `retract p(a,b,c).` REPL utterances. That top-
//! level command-parsing layer lives in `paideia-as-shell-cmd` (R222),
//! not in this crate: `paideia-as-shell-datalog`'s parser recognises
//! only the interior of a `datalog { … }` block. Extending it to also
//! parse REPL commands would fold the command-registry surface into
//! the Datalog surface — an inversion that would leave R222's command
//! registry with nothing to route.
//!
//! M8 therefore keeps `SessionEdb::assert` / `SessionEdb::retract` as
//! programmatic APIs the REPL command layer will call from its
//! own `assert`/`retract` command handlers. When R222 wires those
//! handlers, the code path is:
//!
//! ```text
//!   shell-cmd::parse_command   →  Command::Assert { pred, args }
//!            │                                │
//!            ▼                                ▼
//!   shell-cmd dispatch          SessionEdb::assert(&mut self, pred, args)
//! ```
//!
//! No AST or parser file in this crate changes for that landing.
//!
//! # Cross-turn survival
//!
//! A `SessionEdb` outlives a single query — the REPL owns one per
//! session, mutates it with `assert` / `retract` / `clear`, and hands
//! an immutable `&SessionEdb` to each `Evaluator::run_*_with_session`
//! call. The evaluator never mutates the session (its `&SessionEdb`
//! signature enforces it): a query is a *read* against the merged EDB.
//! `SessionEdb`'s tests (`r226m8-edb-05`) pin that three sequential
//! queries on the same session observe accumulated assertions.
//!
//! # Isolation
//!
//! Two `SessionEdb` instances share no state — no static, no lazy_static
//! backing store. `r226m8-edb-10` pins that a fact asserted in one
//! session is not visible from another. This is what makes the type
//! trivially safe to hand to two independent shell tabs, or to a REPL
//! and a scripted `.pds` runner that share the same `Program` but not
//! the same interactive history.
//!
//! # Fingerprints
//!
//! Test corpus tags `r226m8-edb-01`..`r226m8-edb-10` (10 fixtures).
//! Each panic message carries its tag so R220.M10's `@fingerprint`
//! correlator can pin a regression to a fixture without re-parsing the
//! test name.

use crate::ast::Value;
use crate::eval::Database;
use std::collections::{HashMap, HashSet};

/// Outcome of a [`SessionEdb::assert`] call.
///
/// A discriminated enum rather than a plain `bool` so the caller reads
/// left-to-right (`match ev { Added => …, AlreadyPresent => … }`)
/// without the reader having to remember which polarity means "insert
/// happened". R222's command dispatcher uses this to render the user-
/// visible message (`"asserted p(a,b)."` vs `"p(a,b) already present."`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssertResult {
    /// The tuple was not previously in the session EDB; it is now.
    Added,
    /// The tuple was already present; no change.
    AlreadyPresent,
}

/// Outcome of a [`SessionEdb::retract`] call.
///
/// Same design rationale as [`AssertResult`]: an enum keeps the two
/// outcomes named at call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RetractResult {
    /// The tuple was present and has been removed.
    Removed,
    /// The tuple was not present; nothing removed.
    NotFound,
}

/// Session-local extensional database — the per-turn tuple overlay a
/// REPL merges on top of `Program::facts` when running a query.
///
/// Internal shape mirrors [`Database`]'s: a `HashMap` keyed by
/// `(predicate name, arity)` to a `HashSet<Vec<Value>>`. `(name, arity)`
/// distinguishes overloads (`p/2` and `p/3` are separate relations, per
/// the R229 schema-registry contract). Duplicate assertions deduplicate
/// naturally through the `HashSet`.
#[derive(Clone, Debug, Default)]
pub struct SessionEdb {
    facts: HashMap<(String, usize), HashSet<Vec<Value>>>,
}

impl SessionEdb {
    /// Fresh, empty session — no assertions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Assert `pred(args…)` into the session EDB.
    ///
    /// Returns [`AssertResult::Added`] when the tuple was newly
    /// inserted, or [`AssertResult::AlreadyPresent`] when the identical
    /// tuple was already there. Arity is implicit in `args.len()` — the
    /// caller does not name it separately.
    pub fn assert(&mut self, pred: &str, args: Vec<Value>) -> AssertResult {
        let key = (pred.to_owned(), args.len());
        let set = self.facts.entry(key).or_default();
        if set.insert(args) {
            AssertResult::Added
        } else {
            AssertResult::AlreadyPresent
        }
    }

    /// Retract `pred(args…)` from the session EDB.
    ///
    /// Returns [`RetractResult::Removed`] when a matching tuple was
    /// found and removed, or [`RetractResult::NotFound`] otherwise.
    /// Retracting a tuple that was never asserted is not an error — a
    /// REPL rendering the outcome may still say so.
    pub fn retract(&mut self, pred: &str, args: &[Value]) -> RetractResult {
        let key = (pred.to_owned(), args.len());
        if let Some(set) = self.facts.get_mut(&key) {
            if set.remove(args) {
                return RetractResult::Removed;
            }
        }
        RetractResult::NotFound
    }

    /// Drop every assertion in this session, restoring it to the
    /// as-constructed state. Predicate/arity keys with no remaining
    /// tuples are removed too (so a `snapshot()` after `clear()` is
    /// indistinguishable from a `snapshot()` on a fresh session).
    pub fn clear(&mut self) {
        self.facts.clear();
    }

    /// True when no assertions are present. Convenience for callers
    /// (`if session.is_empty() { … skip merge … }`) that want to short-
    /// circuit the overlay cost when the session is fresh; the
    /// evaluator's `with_session` entry points always merge, so this
    /// is a hint, not a semantic gate.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.facts.values().all(|s| s.is_empty())
    }

    /// Sum of tuple counts across every `(predicate, arity)` key in
    /// this session. Mirrors [`Database::total_tuple_count`] — a
    /// debugger fixture that compares session state before and after
    /// a query uses this without enumerating every predicate name.
    pub fn total_tuple_count(&self) -> usize {
        self.facts.values().map(|s| s.len()).sum()
    }

    /// Materialize the session EDB as a standalone [`Database`].
    ///
    /// Contains only the session's own tuples — no program facts, no
    /// IDB derivations. Useful for a REPL command that dumps the
    /// session state (`.session-dump`) or for a fixture that compares
    /// the overlay against a merged DB.
    pub fn snapshot(&self) -> Database {
        let mut db = Database::default();
        for ((pred, _arity), set) in &self.facts {
            for tuple in set {
                db.insert(pred.clone(), tuple.clone());
            }
        }
        db
    }

    /// Return a new [`Database`] that is `base` with this session's
    /// tuples layered on top.
    ///
    /// Session facts are *additive*: identical tuples deduplicate
    /// through the underlying `HashSet<Vec<Value>>`, so a session
    /// asserting a tuple already present in `base` produces the same
    /// merged DB as one that did not. `base` is not mutated (it is
    /// cloned) so the caller can reuse it for the next turn.
    pub fn merge_into(&self, base: &Database) -> Database {
        let mut merged = base.clone();
        for ((pred, _arity), set) in &self.facts {
            for tuple in set {
                merged.insert(pred.clone(), tuple.clone());
            }
        }
        merged
    }

    /// Crate-internal reader used by the evaluator's `_with_session`
    /// entry points to seed the DB directly from the overlay without
    /// paying for a [`Database`] clone (unlike [`Self::merge_into`]).
    ///
    /// Kept `pub(crate)` deliberately: outside callers use
    /// [`Self::snapshot`] or [`Self::merge_into`], both of which
    /// return owned data. Exposing the internal map publicly would let
    /// a caller reason about the storage layout, which we reserve the
    /// right to change (an indexed variant is on the R226.M10 radar).
    #[inline]
    pub(crate) fn facts_ref(&self) -> &HashMap<(String, usize), HashSet<Vec<Value>>> {
        &self.facts
    }
}
