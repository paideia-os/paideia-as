//! Side-table: item-level Let binding → DSL parser name registered by
//! the trailing `@dsl_parser("<name>")` attribute (R220.M3, paideia-as#1417).
//!
//! Module-level `pub let` bindings whose value is a `Syntax -> Syntax`
//! function may carry the trailing `@dsl_parser("<name>")` attribute:
//!
//! ```text
//! pub let parse_num : (Syntax) -> Syntax
//!     = fn(body: Syntax) -> body
//!     @dsl_parser("num");
//! ```
//!
//! The parser records the DSL name on this side-table (keyed by the Let
//! node's `NodeId`) rather than growing every construction/destructuring
//! site of [`crate::ItemData::Let`] — the same pattern as
//! [`crate::ItemAtomicTable`] (paideia-as#1301) and
//! [`crate::StructAttrTable`] (paideia-as#1373). The elaborator's
//! `dsl_parser_registry` pass reads back the entry when it builds the
//! per-module `DslParserRegistry` used to dispatch hosted-DSL bodies
//! through `expand_reflective_hygienic` (R220.M2).
//!
//! Fingerprint tag: r220m3-dsl-01.

use std::collections::HashMap;

use crate::node_id::NodeId;

/// Maps Let-item `NodeId` → registered DSL parser name.
///
/// Sparse: only Let bindings that carry `@dsl_parser("<name>")` at parse
/// time have an entry. Absence is the common case.
///
/// **Name canonicalisation.** Parser-side validation restricts DSL names
/// to `[A-Za-z_][A-Za-z0-9_]*` (identifier-shaped, ASCII only) so the
/// registry's lookup key is byte-identical to the invocation-site head
/// identifier — no NFC / case-folding surprises. Length cap: 64 chars,
/// per `design/terminal/semantic-shell-language-plan.md` §4 R220.M3.
#[derive(Debug, Default)]
pub struct ItemDslParserTable {
    entries: HashMap<NodeId, String>,
}

impl ItemDslParserTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Insert a Let-item → DSL name mapping (overwrites any prior entry).
    ///
    /// Duplicate-name diagnostics (two distinct Let bindings registering
    /// the same DSL name) are the elaborator's responsibility — this
    /// side-table only stores, never validates.
    pub fn insert(&mut self, let_id: NodeId, dsl_name: String) {
        self.entries.insert(let_id, dsl_name);
    }

    /// Look up the registered DSL name for a Let item.
    #[must_use]
    pub fn get(&self, let_id: NodeId) -> Option<&str> {
        self.entries.get(&let_id).map(String::as_str)
    }

    /// Number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over `(Let-id, dsl-name)` pairs.
    ///
    /// Iteration order is unspecified (HashMap-backed); callers that
    /// need a deterministic order should collect and sort by `NodeId`.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &str)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fingerprint tag: r220m3-dsl-01
    #[test]
    fn new_is_empty() {
        let t = ItemDslParserTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    // Fingerprint tag: r220m3-dsl-02
    #[test]
    fn insert_and_get() {
        let mut t = ItemDslParserTable::new();
        let id = NodeId::new(7).unwrap();
        t.insert(id, "num".to_string());
        assert_eq!(t.get(id), Some("num"));
        assert_eq!(t.len(), 1);
    }

    // Fingerprint tag: r220m3-dsl-03
    #[test]
    fn get_absent_returns_none() {
        let t = ItemDslParserTable::new();
        assert!(t.get(NodeId::new(1).unwrap()).is_none());
    }

    // Fingerprint tag: r220m3-dsl-04
    #[test]
    fn insert_overwrites() {
        let mut t = ItemDslParserTable::new();
        let id = NodeId::new(3).unwrap();
        t.insert(id, "foo".to_string());
        t.insert(id, "bar".to_string());
        assert_eq!(t.get(id), Some("bar"));
        assert_eq!(t.len(), 1);
    }

    // Fingerprint tag: r220m3-dsl-05
    #[test]
    fn iter_sees_all_entries() {
        let mut t = ItemDslParserTable::new();
        t.insert(NodeId::new(1).unwrap(), "a".to_string());
        t.insert(NodeId::new(2).unwrap(), "b".to_string());
        let mut seen: Vec<_> = t.iter().map(|(_, n)| n.to_string()).collect();
        seen.sort();
        assert_eq!(seen, vec!["a".to_string(), "b".to_string()]);
    }
}
