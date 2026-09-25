//! R229.M2 — lower a shell-ast [`SyntaxNode`] datalog block into a
//! shell-datalog [`Program`].
//!
//! # Why a separate module (not inlined into `turn.rs`)
//!
//! The R229.M1 datalog branch re-tokenized the source and asked
//! `paideia_as_shell_datalog::parser::parse_block` to re-parse it — a
//! deliberate M1 kludge because the shell-ast parser already produced a
//! [`SyntaxNode::DatalogBlock`] over the same source but the item shape
//! ([`SyntaxNode::Atom`] / [`SyntaxNode::Rule`]) is not what the R226
//! evaluator consumes ([`Program { rules, facts }`](Program)). M2's
//! typed elaborator replaces that double-parse with a direct AST →
//! Program walk. The walk lives in its own module so the executor
//! branch in `turn.rs` stays tight (three tool boundaries: lower,
//! typecheck, run) and so R225's HM-typed elaborator (M2 next step) can
//! reuse the same walk to seed its Datalog-context environment without
//! pulling in the whole turn pipeline.
//!
//! # Term dictionary
//!
//! * [`SyntaxNode::QVar`] `?X` → [`Term::Var("X")`](Term::Var).
//! * [`SyntaxNode::Ident`] `alice` → [`Term::Const(Value::Ident("alice"))`](Term::Const).
//! * [`SyntaxNode::LitStr`] `"s"` → [`Term::Const(Value::Str("s"))`](Term::Const).
//! * [`SyntaxNode::LitInt`] `42` → [`Term::Const(Value::Num(42))`](Term::Const).
//! * [`SyntaxNode::InterpVar`] `$name` → [`Term::Bound("name")`](Term::Bound).
//!   (The R226 evaluator refuses to *run* a program that mentions
//!   `Bound`, but the lowering itself is not gated — a `run` error is
//!   easier to explain than a `lower` one on this axis.)
//! * [`SyntaxNode::Var`] `x` (lambda-context variable, unusual inside a
//!   datalog block but folded in for symmetry with M2's typed
//!   elaborator) → [`Term::Var("x")`](Term::Var).
//! * Anything else — [`LowerError::IdentExpected`].
//!
//! # Clause dictionary
//!
//! Each item of [`SyntaxNode::DatalogBlock::items`] is one of:
//!
//! * [`SyntaxNode::Atom`] — a bare fact. If every term lowers to a
//!   `Term::Const(_)` it lands in [`Program::facts`]; otherwise it is
//!   treated as a rule with an empty body — but the R226 evaluator
//!   forbids that shape (a non-ground atom without a rule is
//!   range-restriction-illegal), so we surface a
//!   [`LowerError::MalformedRule`] instead of pushing a broken fact.
//! * [`SyntaxNode::Rule`] — head + body conjunction. The head must
//!   lower to a `dl::Atom`; every body element must be either a
//!   `SyntaxNode::Atom` ([`BodyGoal::Positive`]) or a
//!   `SyntaxNode::NotAtom` whose inner is an `Atom`
//!   ([`BodyGoal::Negative`]).
//! * [`SyntaxNode::NotAtom`] at top level — an isolated negated goal
//!   with no rule head has no Datalog interpretation; surfaced as
//!   [`LowerError::MalformedRule`] rather than silently dropped.
//! * Anything else — [`LowerError::UnsupportedNode`].
//!
//! # Fingerprints
//!
//! The R229.M2 corpus (`tests/turn_lowered.rs`) tags every fixture with
//! `r229m2-turn-NN` so the R220.M10 fingerprint correlator attributes
//! a lowering regression to its fixture without re-parsing the test
//! name.

use paideia_as_shell_ast::SyntaxNode;
use paideia_as_shell_datalog::{Atom, BodyGoal, Program, Rule, Term, Value};
use std::error::Error;
use std::fmt;

/// Structured failure modes for [`lower_datalog`].
///
/// Kept as a small enum (four variants) rather than a boxed
/// `dyn Error`: each variant names a distinct architectural axis
/// (wrong root node, wrong term shape, empty block, malformed rule
/// body) so a diagnostic layer keys directly off the discriminant
/// without string-parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    /// The lowering entry point was handed a node that is not a
    /// [`SyntaxNode::DatalogBlock`] (or, inside a walk, a node whose
    /// role slot wants a specific variant it does not fill — e.g. an
    /// item in [`SyntaxNode::DatalogBlock::items`] that is neither an
    /// `Atom`, `Rule`, nor `NotAtom`). `kind` is the offending
    /// variant's short name (see `variant_name`).
    UnsupportedNode {
        /// Short human name of the offending variant.
        kind: String,
    },
    /// A term slot inside an atom was filled by a node whose shape has
    /// no Term projection (e.g. a nested `Atom`, a `Lambda`, a
    /// `RecordExpr`). `got` is the variant's short name.
    IdentExpected {
        /// Short human name of the offending variant.
        got: String,
    },
    /// Reserved: the datalog block had no items. Currently the walk
    /// treats an empty block as a valid empty program (matching the
    /// M1 fixture `datalog { }`); this variant is kept for the M3+
    /// caller that wants to refuse an empty block outright.
    EmptyDatalogBlock,
    /// A [`SyntaxNode::Rule`] could not be lowered because its head
    /// or body did not match the expected shape (head must lower to
    /// an atom; each body element must be an `Atom` or a `NotAtom`
    /// wrapping an `Atom`). `reason` is a short human explanation.
    MalformedRule {
        /// Why the rule failed to lower — surfaced in `Display`.
        reason: String,
    },
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::UnsupportedNode { kind } => {
                write!(f, "unsupported syntax node: {kind}")
            }
            LowerError::IdentExpected { got } => {
                write!(f, "expected a datalog term, got {got}")
            }
            LowerError::EmptyDatalogBlock => {
                write!(f, "datalog block is empty")
            }
            LowerError::MalformedRule { reason } => {
                write!(f, "malformed datalog rule: {reason}")
            }
        }
    }
}

impl Error for LowerError {}

/// Lower a [`SyntaxNode::DatalogBlock`] into a shell-datalog
/// [`Program`].
///
/// # Contract
///
/// * `node` must be a [`SyntaxNode::DatalogBlock`]; any other variant
///   yields [`LowerError::UnsupportedNode`] tagged with the actual
///   variant's short name.
/// * Every item is walked in source order. Ground atoms accumulate in
///   [`Program::facts`]; rules accumulate in [`Program::rules`].
/// * The R226 [`Rule`] shape forbids an empty body (a body-less rule
///   is stored as a fact by the R226 parser), so a `SyntaxNode::Rule`
///   with `body.is_empty()` is rejected as [`LowerError::MalformedRule`]
///   — the alternative (silently discarding the rule) would drop
///   user intent without a diagnostic.
/// * Item order is preserved within `facts` and within `rules`
///   separately; interleaved order across the two is not preserved
///   because [`Program`] does not model it (the R226 evaluator seeds
///   from `facts` in one pass, then iterates over `rules`).
pub fn lower_datalog(node: &SyntaxNode) -> Result<Program, LowerError> {
    let items = match node {
        SyntaxNode::DatalogBlock { items, .. } => items,
        other => {
            return Err(LowerError::UnsupportedNode {
                kind: variant_name(other).to_owned(),
            });
        }
    };

    let mut program = Program::empty();
    for item in items {
        lower_clause(item, &mut program)?;
    }
    Ok(program)
}

/// Lower one clause (an item inside a `DatalogBlock`) into either a
/// [`Program::facts`] entry or a [`Program::rules`] entry.
fn lower_clause(node: &SyntaxNode, program: &mut Program) -> Result<(), LowerError> {
    match node {
        SyntaxNode::Atom { .. } => {
            let atom = lower_atom(node)?;
            program.facts.push(atom);
            Ok(())
        }
        SyntaxNode::Rule { head, body, .. } => {
            let dl_head = lower_atom(head)?;
            if body.is_empty() {
                // The R226 `Rule` type forbids an empty body — such
                // shapes belong in `Program::facts` (the parser lifts
                // them there itself). A parser that produced
                // `Rule { body: vec![] }` here would be a bug; surface
                // it rather than silently reshaping.
                return Err(LowerError::MalformedRule {
                    reason: "rule body is empty".to_owned(),
                });
            }
            let mut dl_body: Vec<BodyGoal> = Vec::with_capacity(body.len());
            for goal in body {
                dl_body.push(lower_body_goal(goal)?);
            }
            program.rules.push(Rule { head: dl_head, body: dl_body });
            Ok(())
        }
        SyntaxNode::NotAtom { .. } => Err(LowerError::MalformedRule {
            reason: "top-level `not` clause has no rule head".to_owned(),
        }),
        other => Err(LowerError::UnsupportedNode {
            kind: variant_name(other).to_owned(),
        }),
    }
}

/// Lower one body element of a rule into a [`BodyGoal`].
///
/// Positive: a bare `SyntaxNode::Atom`.
/// Negative: a `SyntaxNode::NotAtom { inner }` whose `inner` is an
/// `Atom`. Any other shape is [`LowerError::MalformedRule`] — the
/// R226 evaluator refuses to run programs with malformed body shapes
/// anyway, so a `lower` diagnostic is strictly more helpful than a
/// downstream `run` panic.
fn lower_body_goal(node: &SyntaxNode) -> Result<BodyGoal, LowerError> {
    match node {
        SyntaxNode::Atom { .. } => Ok(BodyGoal::Positive(lower_atom(node)?)),
        SyntaxNode::NotAtom { inner, .. } => match inner.as_ref() {
            SyntaxNode::Atom { .. } => Ok(BodyGoal::Negative(lower_atom(inner)?)),
            other => Err(LowerError::MalformedRule {
                reason: format!("`not` wraps {} (want an atom)", variant_name(other)),
            }),
        },
        other => Err(LowerError::MalformedRule {
            reason: format!(
                "body goal must be an atom or negated atom, got {}",
                variant_name(other)
            ),
        }),
    }
}

/// Lower a `SyntaxNode::Atom` into a shell-datalog [`Atom`]. Any other
/// variant is [`LowerError::UnsupportedNode`] — the caller (a rule
/// head, a body goal slot, a fact slot) always has an `Atom` in mind.
fn lower_atom(node: &SyntaxNode) -> Result<Atom, LowerError> {
    let (pred, args) = match node {
        SyntaxNode::Atom { pred, args, .. } => (pred.clone(), args),
        other => {
            return Err(LowerError::UnsupportedNode {
                kind: variant_name(other).to_owned(),
            });
        }
    };
    let mut terms: Vec<Term> = Vec::with_capacity(args.len());
    for a in args {
        terms.push(lower_term(a)?);
    }
    Ok(Atom { predicate: pred, terms })
}

/// Lower one term slot into a shell-datalog [`Term`]. See the term
/// dictionary in the module doc for the mapping.
fn lower_term(node: &SyntaxNode) -> Result<Term, LowerError> {
    match node {
        SyntaxNode::QVar { name, .. } => Ok(Term::Var(name.clone())),
        SyntaxNode::Var { name, .. } => Ok(Term::Var(name.clone())),
        SyntaxNode::Ident { name, .. } => Ok(Term::Const(Value::Ident(name.clone()))),
        SyntaxNode::LitStr { value, .. } => Ok(Term::Const(Value::Str(value.clone()))),
        SyntaxNode::LitInt { value, .. } => Ok(Term::Const(Value::Num(*value))),
        SyntaxNode::InterpVar { name, .. } => Ok(Term::Bound(name.clone())),
        other => Err(LowerError::IdentExpected {
            got: variant_name(other).to_owned(),
        }),
    }
}

/// Short, one-word name for a `SyntaxNode` variant. Duplicated from
/// `turn::variant_name` (that copy is `pub(crate)`-adjacent through
/// its module boundary) to keep `lower` free of a turn-module import
/// — the two walks are architecturally independent and shouldn't
/// share a private helper.
fn variant_name(n: &SyntaxNode) -> &'static str {
    match n {
        SyntaxNode::Cmd { .. } => "cmd",
        SyntaxNode::Pipe { .. } => "pipe",
        SyntaxNode::Seq { .. } => "seq",
        SyntaxNode::Redirect { .. } => "redirect",
        SyntaxNode::Background { .. } => "background",
        SyntaxNode::Group { .. } => "group",
        SyntaxNode::DatalogBlock { .. } => "datalog-block",
        SyntaxNode::Atom { .. } => "atom",
        SyntaxNode::Rule { .. } => "rule",
        SyntaxNode::QVar { .. } => "qvar",
        SyntaxNode::InterpVar { .. } => "interp-var",
        SyntaxNode::NotAtom { .. } => "not-atom",
        SyntaxNode::Lambda { .. } => "lambda",
        SyntaxNode::App { .. } => "app",
        SyntaxNode::Var { .. } => "var",
        SyntaxNode::Let { .. } => "let",
        SyntaxNode::Match { .. } => "match",
        SyntaxNode::BinOp { .. } => "binop",
        SyntaxNode::UnaryOp { .. } => "unaryop",
        SyntaxNode::FieldAccess { .. } => "field-access",
        SyntaxNode::RecordExpr { .. } => "record",
        SyntaxNode::LitStr { .. } => "lit-str",
        SyntaxNode::LitInt { .. } => "lit-int",
        SyntaxNode::LitBool { .. } => "lit-bool",
        SyntaxNode::Ident { .. } => "ident",
    }
}
