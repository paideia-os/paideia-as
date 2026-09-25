//! Types: monotypes, type variables, type schemes, row types for
//! record polymorphism (R225.M2), and effect rows (R225.M3).
//!
//! The type language grew in R225.M2 to include `Record(RowType)` so
//! the shell's record surface can be checked with Rémy-style row
//! polymorphism (a single fresh row variable represents "any further
//! fields"); R225.M3 added an `EffectRow(EffectRow)` variant that
//! carries the *same* Rémy machinery for effect-row polymorphism but
//! in a disjoint namespace — record rows and effect rows never
//! accidentally unify. Everything else remains as it was in R225.M1.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::effect_row::EffectRow;
use crate::typed_value::TypedValue;

/// A fresh type variable, minted by [`crate::infer::FreshVarGen`].
///
/// `TypeVar` values are opaque monotonic identifiers — the [`u32`]
/// payload has no meaning beyond distinguishing one variable from
/// another. Comparisons are by numeric identity, not by structural
/// role.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVar(pub u32);

/// A monotype: no top-level (or nested) universal quantifiers.
///
/// The three original shapes (`Var`, `Con`, `Arrow`) match the M1 type
/// grammar; `Record` — added in M2 — carries a [`RowType`] describing
/// a (possibly row-polymorphic) record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonoType {
    /// A type variable (either free or later replaced by
    /// [`crate::subst::Substitution`]).
    Var(TypeVar),
    /// A type constant such as `Int` or `Str`. The payload is the
    /// name; M1 has no constant arity — every constant is nullary.
    Con(String),
    /// A function type `τ1 -> τ2`.
    Arrow(Box<MonoType>, Box<MonoType>),
    /// A record type over a (possibly row-polymorphic) row.
    Record(RowType),
    /// An effect row over a (possibly row-polymorphic) label set.
    ///
    /// R225.M3: a *distinct* variant from [`MonoType::Record`] so
    /// record rows and effect rows never accidentally unify. The
    /// underlying algebra is the same Rémy-style row polymorphism,
    /// but the two namespaces stay lexically disjoint at the type
    /// level — a record-vs-effect unification attempt surfaces as an
    /// ordinary [`crate::unify::UnifyError::Mismatch`], not as a
    /// row-shape failure.
    EffectRow(EffectRow),
    /// A [`TypedValue`] — a paired record value-row and effect row
    /// unified as one monotype (R225.M5). Boxed to keep [`MonoType`]
    /// non-recursive-tail-inflating; the box costs one indirection
    /// but the alternative is a wider enum discriminant for every
    /// other variant that never carries a typed value.
    ///
    /// Cross-variant unification against a bare [`MonoType::Record`]
    /// or [`MonoType::EffectRow`] is rejected as an ordinary
    /// [`crate::unify::UnifyError::Mismatch`] — a `TypedValue` is a
    /// distinct algebraic kind, not a sugar for either single-row
    /// half.
    Typed(Box<TypedValue>),
}

impl MonoType {
    /// Collect every free type variable that appears in this monotype.
    ///
    /// A monotype has no binders, so *every* [`TypeVar`] it mentions
    /// is free. The result is a set (order-insensitive); callers that
    /// need a stable ordering should sort by [`TypeVar::0`].
    pub fn free_vars(&self) -> HashSet<TypeVar> {
        let mut out = HashSet::new();
        self.collect_free_vars(&mut out);
        out
    }

    fn collect_free_vars(&self, out: &mut HashSet<TypeVar>) {
        match self {
            Self::Var(v) => {
                out.insert(*v);
            }
            Self::Con(_) => {}
            Self::Arrow(a, b) => {
                a.collect_free_vars(out);
                b.collect_free_vars(out);
            }
            Self::Record(row) => {
                for v in row.free_vars() {
                    out.insert(v);
                }
            }
            Self::EffectRow(row) => {
                // Union payload free-vars with the tail row-var (if any).
                for ty in row.present.values() {
                    ty.collect_free_vars(out);
                }
                if let Some(v) = row.tail {
                    out.insert(v);
                }
            }
            Self::Typed(tv) => {
                // Union of both sides' free vars — record row and
                // effect row share a single substitution domain, so
                // any variable free on either side is free in the
                // whole typed value.
                for v in tv.value_row.free_vars() {
                    out.insert(v);
                }
                for ty in tv.effect_row.present.values() {
                    ty.collect_free_vars(out);
                }
                if let Some(v) = tv.effect_row.tail {
                    out.insert(v);
                }
            }
        }
    }
}

impl fmt::Display for MonoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Variables print as lower-case letters `a`, `b`, ... for
            // the first 26, then `t<n>` after that. This keeps the
            // common small-tree case readable in test diagnostics
            // without pretending variables are alpha-normalised (they
            // are not — the id is the raw fresh counter).
            Self::Var(TypeVar(n)) => {
                if (*n as usize) < 26 {
                    let c = char::from(b'a' + *n as u8);
                    write!(f, "{c}")
                } else {
                    write!(f, "t{n}")
                }
            }
            Self::Con(name) => f.write_str(name),
            // Right-associative arrows: `a -> b -> c` renders as
            // `(a -> (b -> c))` — we always parenthesise so a reader
            // does not have to remember precedence when reading a
            // test failure message.
            Self::Arrow(a, b) => write!(f, "({a} -> {b})"),
            Self::Record(row) => {
                // Sort field names for deterministic Display in test
                // diagnostics, then render `{a: T, b: U | rowVar}` or
                // `{a: T}` (or `{}` / `{| rowVar}` at the degenerate
                // ends).
                let (fields, tail) = row.to_map();
                let mut sorted: Vec<(String, MonoType)> = fields.into_iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                f.write_str("{")?;
                for (i, (name, ty)) in sorted.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {ty}")?;
                }
                if let Some(v) = tail {
                    if sorted.is_empty() {
                        f.write_str("| ")?;
                    } else {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{}", MonoType::Var(v))?;
                }
                f.write_str("}")
            }
            // Effect rows print with a leading `!` so a reader can tell
            // them apart from records at a glance in a test failure
            // diagnostic. Payload types are elided when they are the
            // conventional `Unit` tag so `!{io, fs}` reads as intended.
            Self::EffectRow(row) => {
                f.write_str("!{")?;
                let unit_tag = MonoType::Con("Unit".to_owned());
                let mut first = true;
                for (name, ty) in &row.present {
                    if !first {
                        f.write_str(", ")?;
                    }
                    first = false;
                    if *ty == unit_tag {
                        f.write_str(name)?;
                    } else {
                        write!(f, "{name}: {ty}")?;
                    }
                }
                if let Some(v) = row.tail {
                    if row.present.is_empty() {
                        f.write_str("| ")?;
                    } else {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{}", MonoType::Var(v))?;
                }
                f.write_str("}")
            }
            // Typed values render as `{value_row ! effect_row}`, a
            // single brace pair with the two sides separated by `!`.
            // The record half reuses the sorted-fields walker; the
            // effect half reuses the elide-Unit convention so a
            // typical R225.M5 fixture reads as `{name: Str ! io, fs}`.
            Self::Typed(tv) => {
                f.write_str("{")?;

                // -- Value-row half -----------------------------
                let (fields, v_tail) = tv.value_row.to_map();
                let mut sorted: Vec<(String, MonoType)> = fields.into_iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                for (i, (name, ty)) in sorted.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {ty}")?;
                }
                if let Some(v) = v_tail {
                    if sorted.is_empty() {
                        f.write_str("| ")?;
                    } else {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{}", MonoType::Var(v))?;
                }

                // -- Separator ---------------------------------
                f.write_str(" ! ")?;

                // -- Effect-row half ---------------------------
                let unit_tag = MonoType::Con("Unit".to_owned());
                let mut first = true;
                for (name, ty) in &tv.effect_row.present {
                    if !first {
                        f.write_str(", ")?;
                    }
                    first = false;
                    if *ty == unit_tag {
                        f.write_str(name)?;
                    } else {
                        write!(f, "{name}: {ty}")?;
                    }
                }
                if let Some(v) = tv.effect_row.tail {
                    if tv.effect_row.present.is_empty() {
                        f.write_str("| ")?;
                    } else {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{}", MonoType::Var(v))?;
                }

                f.write_str("}")
            }
        }
    }
}

/// A row type: an unordered set of field-name → monotype pairings,
/// optionally tailed by a row variable.
///
/// R225.M2 uses Rémy-style row polymorphism: a `RowVar` in the tail
/// position stands for "any further fields". Unification splits both
/// sides into (shared, only-left, only-right) and either constrains
/// the tail row variables to absorb the extras (row-polymorphic) or
/// rejects with [`crate::unify::UnifyError::MissingField`] when a
/// closed row lacks a demanded field.
///
/// Rows are structurally deterministic modulo tail identity: the
/// [`RowType::from_map`] constructor canonicalises the field
/// sequence by sorting on field name so `Display` output is stable
/// across runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowType {
    /// A closed row with no fields — the empty record shape `{}`.
    Empty,
    /// A row variable — a `TypeVar` in the row's tail position that
    /// stands for "any further fields".
    RowVar(TypeVar),
    /// A field extension: prepend `field: ty` onto `rest`. Chains of
    /// `Extend` ending in [`RowType::Empty`] or [`RowType::RowVar`]
    /// encode the whole row.
    Extend {
        /// The field name being added at this position.
        field: String,
        /// The monotype at this field.
        ty: Box<MonoType>,
        /// The remainder of the row (further fields, plus optional
        /// tail row variable).
        rest: Box<RowType>,
    },
}

impl RowType {
    /// Collect every free [`TypeVar`] mentioned by this row —
    /// including the tail row variable, if any, and every free
    /// variable of every field type.
    pub fn free_vars(&self) -> HashSet<TypeVar> {
        let mut out = HashSet::new();
        self.collect_free_vars(&mut out);
        out
    }

    fn collect_free_vars(&self, out: &mut HashSet<TypeVar>) {
        match self {
            Self::Empty => {}
            Self::RowVar(v) => {
                out.insert(*v);
            }
            Self::Extend { ty, rest, .. } => {
                for v in ty.free_vars() {
                    out.insert(v);
                }
                rest.collect_free_vars(out);
            }
        }
    }

    /// Flatten a row into `(fields, optional tail row variable)`.
    ///
    /// Walks the `Extend` chain accumulating fields into a
    /// [`HashMap`]; on encountering [`RowType::Empty`] the tail is
    /// [`None`], on [`RowType::RowVar`] the tail is `Some(v)`.
    ///
    /// If the same field appears more than once (which
    /// [`RowType::from_map`] never produces, but hand-built rows in
    /// tests might), the later occurrence wins — the row is treated
    /// as an unordered map of the outermost binding per name.
    pub fn to_map(&self) -> (HashMap<String, MonoType>, Option<TypeVar>) {
        let mut fields = HashMap::new();
        let mut cur = self;
        loop {
            match cur {
                RowType::Empty => return (fields, None),
                RowType::RowVar(v) => return (fields, Some(*v)),
                RowType::Extend { field, ty, rest } => {
                    // Insert only if not already present: the *outer*
                    // binding shadows the inner one in an ordered
                    // row-extension reading, so we keep the first.
                    fields.entry(field.clone()).or_insert_with(|| (**ty).clone());
                    cur = rest;
                }
            }
        }
    }

    /// Build a canonicalised row from an unordered map of fields and
    /// an optional tail row variable.
    ///
    /// Fields are emitted in name-sorted order so `Display` output is
    /// deterministic across runs (a plain [`HashMap`] iteration order
    /// would drift with hash-map salt). The resulting `Extend` chain
    /// terminates in [`RowType::Empty`] when `tail` is [`None`] and
    /// [`RowType::RowVar`] otherwise.
    pub fn from_map(fields: HashMap<String, MonoType>, tail: Option<TypeVar>) -> RowType {
        let mut sorted: Vec<(String, MonoType)> = fields.into_iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let base = match tail {
            Some(v) => RowType::RowVar(v),
            None => RowType::Empty,
        };
        // Fold from the right so the alphabetically-first field ends
        // up as the outermost Extend — that matches the sort order
        // the Display walker also uses.
        sorted
            .into_iter()
            .rev()
            .fold(base, |acc, (name, ty)| RowType::Extend {
                field: name,
                ty: Box::new(ty),
                rest: Box::new(acc),
            })
    }
}

/// A rank-1 type scheme: a monotype body prefixed by an outermost
/// sequence of universal quantifiers.
///
/// A scheme with an empty `quantified` list is a monotype dressed as a
/// scheme — the two are equivalent under [`crate::infer::instantiate`]
/// and [`crate::infer::generalize`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeScheme {
    /// The universally quantified variables — the `α` in `∀α. τ`.
    ///
    /// Duplicates are not enforced against; the algorithm never
    /// produces them. Order is preserved for deterministic pretty
    /// printing but has no semantic weight.
    pub quantified: Vec<TypeVar>,
    /// The scheme body.
    pub body: MonoType,
}

impl TypeScheme {
    /// Free type variables of the scheme: those free in the body but
    /// not in the [`Self::quantified`] list.
    ///
    /// This is the definition [`crate::infer::generalize`] uses to
    /// decide which variables of a monotype are eligible for
    /// quantification.
    pub fn free_vars(&self) -> HashSet<TypeVar> {
        let mut out = self.body.free_vars();
        for q in &self.quantified {
            out.remove(q);
        }
        out
    }
}

impl fmt::Display for TypeScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.quantified.is_empty() {
            write!(f, "{}", self.body)
        } else {
            f.write_str("forall")?;
            for q in &self.quantified {
                write!(f, " {}", MonoType::Var(*q))?;
            }
            write!(f, ". {}", self.body)
        }
    }
}
