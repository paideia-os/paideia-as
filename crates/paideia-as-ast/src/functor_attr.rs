//! Side-table: functor-decl `NodeId` → its list of [`FunctorAttr`] entries.
//!
//! paideia-as#1389 (v0.32-M1-003, Toolkit Batch 3). Functor-level
//! attributes — the first pair being `@retain` and `@immediate` — attach
//! here rather than growing every construction/destructuring site of
//! [`crate::ItemData::Functor`]. New primitives (future functor-level
//! primitives land here) extend the table by pushing an additional
//! [`FunctorAttr`] onto the functor's entry.
//!
//! Design parallels [`crate::StructAttrTable`] (v0.28 Batch 2) — a sparse
//! per-node side-table on the [`crate::AstArena`], populated by the parser
//! and read by later phases (elaborator capability-flow pass, session-type
//! checker).
//!
//! **v0.32-M1-003 landing scope.** The M1-003 parser primitive that
//! recognises `@retain` / `@immediate` is the standalone
//! `paideia_as_parser::toolkit_attrs::parse_functor_with_attrs`, which
//! predates the parse-item integration of functor decls and therefore
//! does not yet mint the `NodeId` needed to key this table. The table
//! ships now so the AST arena surface stays symmetric with
//! [`crate::StructAttrTable`], and so the M1-004 item-parser hookup can
//! push into it without a further AST churn.

use std::collections::HashMap;

use crate::node_id::NodeId;

/// Functor-level attribute (paideia-as#1389, v0.32-M1-003).
///
/// Attached to a functor-decl node via the [`FunctorAttrTable`]
/// side-table. Sparse — most functor declarations have no entry. New
/// primitives extend this enum by appending a variant.
///
/// **Semantics.** `@retain` and `@immediate` are the two ends of a
/// per-invocation capability-flow discipline on a functor's *input*
/// module:
///
/// - `Retain` — the functor may hold a reference to its input capability
///   across calls. The elaborator does not insert a consume-on-return
///   barrier at the functor boundary.
/// - `Immediate` — the functor MUST consume its input capability by the
///   time it returns. The elaborator inserts a return-path check that
///   the input has been threaded into the result (or dropped) and
///   rejects at compile time if the discipline is violated.
///
/// The two are mutually exclusive at the declaration site — a functor
/// either carries the input across (`Retain`) or hands it back /
/// consumes it on return (`Immediate`), never both. The parser rejects
/// co-declaration in place (P0330) so no downstream phase has to
/// re-check the invariant.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FunctorAttr {
    /// `@retain` — the functor may retain its input capability across
    /// invocations. No return-path consume barrier is inserted.
    Retain,
    /// `@immediate` — the functor MUST consume its input capability on
    /// the return path. Incompatible with [`Self::Retain`].
    Immediate,
}

/// Maps functor-decl `NodeId` → its list of [`FunctorAttr`] entries.
///
/// Sparse: only functor decls that carry at least one functor-level
/// attribute at parse time have an entry. Absence is the common case
/// and is exposed as an empty slice by [`Self::get`], so callers never
/// have to distinguish "no entry" from "empty entry" — a functor with
/// no annotations reads back the same way as one whose entry has been
/// drained.
#[derive(Debug, Default)]
pub struct FunctorAttrTable {
    entries: HashMap<NodeId, Vec<FunctorAttr>>,
}

impl FunctorAttrTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Append `attr` to the functor's attribute list, creating the list
    /// on first insert. Preserves insertion order so downstream phases
    /// can rely on source order when several attributes stack on one
    /// functor.
    pub fn push(&mut self, functor_id: NodeId, attr: FunctorAttr) {
        self.entries.entry(functor_id).or_default().push(attr);
    }

    /// Look up the attribute list for a functor decl.
    ///
    /// Returns `None` when no attribute has ever been recorded for the
    /// functor — matching the [`crate::StructAttrTable::get`] convention
    /// so both side-tables read the same way at the call site. When
    /// present, the slice preserves insertion order.
    #[must_use]
    pub fn get(&self, functor_id: NodeId) -> Option<&[FunctorAttr]> {
        self.entries.get(&functor_id).map(Vec::as_slice)
    }

    /// Number of functor decls with at least one recorded attribute.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff the table has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over `(functor-id, attribute-slice)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &[FunctorAttr])> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let t = FunctorAttrTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn push_and_get_single_retain() {
        let mut t = FunctorAttrTable::new();
        let id = NodeId::new(7).unwrap();
        t.push(id, FunctorAttr::Retain);
        let attrs = t.get(id).expect("entry present");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0], FunctorAttr::Retain);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn push_and_get_single_immediate() {
        let mut t = FunctorAttrTable::new();
        let id = NodeId::new(3).unwrap();
        t.push(id, FunctorAttr::Immediate);
        let attrs = t.get(id).expect("entry present");
        assert_eq!(attrs, &[FunctorAttr::Immediate]);
    }

    #[test]
    fn get_absent_returns_none() {
        let t = FunctorAttrTable::new();
        assert!(t.get(NodeId::new(1).unwrap()).is_none());
    }

    #[test]
    fn push_preserves_insertion_order() {
        // Not a legal source-level combination (parser rejects with
        // P0330), but the table itself is neutral and preserves order
        // in case a future attribute extension needs it.
        let mut t = FunctorAttrTable::new();
        let id = NodeId::new(11).unwrap();
        t.push(id, FunctorAttr::Retain);
        t.push(id, FunctorAttr::Immediate);
        let attrs = t.get(id).expect("entry present");
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0], FunctorAttr::Retain);
        assert_eq!(attrs[1], FunctorAttr::Immediate);
        assert_eq!(t.len(), 1);
    }
}
