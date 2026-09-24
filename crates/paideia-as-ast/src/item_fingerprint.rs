//! Side-table: item-level Let binding → wire fingerprint tag registered by
//! the trailing `@fingerprint("<name>")` attribute (R220.M10, paideia-as#1424).
//!
//! Any module-level `pub let` binding may carry the trailing
//! `@fingerprint("<name>")` attribute:
//!
//! ```text
//! pub let turn_marker : u64 = 0
//!     @fingerprint("test.turn.001");
//! ```
//!
//! The parser records the fingerprint name on this side-table (keyed by the
//! Let node's `NodeId`) rather than growing every construction /
//! destructuring site of [`crate::ItemData::Let`] — the same pattern as
//! [`crate::ItemAtomicTable`] (paideia-as#1301),
//! [`crate::ItemDslParserTable`] (paideia-as#1417, R220.M3), and
//! [`crate::StructAttrTable`] (paideia-as#1373).
//!
//! At elaboration time the R220.M10 pass reads this side-table and appends
//! a `DataEntry` to `IrArena::fingerprints()` — a NUL-terminated byte string
//! (`"<name>\0"`) staged for emission into `.rodata` under the symbol
//! `fp_<name>`. This matches the anti-fabrication pattern kernel-side
//! (see `feedback_workerbee_verify_claims.md`): a debugger can find the
//! fingerprint bytes by simple substring search of the compiled ELF's
//! rodata payload, giving the hosted-DSL / REPL a per-turn wire tag it
//! cannot silently drop.
//!
//! # Name canonicalisation
//!
//! Parser-side validation restricts fingerprint names to
//! `[A-Za-z0-9._-]+`, 1..=128 bytes, 7-bit ASCII printable only (no NUL,
//! no whitespace, no control chars). Dotted (`test.turn.001`) and dashed
//! (`r220m10-fp-01`) shapes are the canonical forms; both round-trip
//! byte-identical from source to `.rodata`, so the debugger's substring
//! match against the tag byte-for-byte succeeds.
//!
//! Fingerprint tag: r220m10-fp-01.

use std::collections::HashMap;

use crate::node_id::NodeId;

/// Maps Let-item `NodeId` → registered fingerprint tag name.
///
/// Sparse: only Let bindings that carry `@fingerprint("<name>")` at parse
/// time have an entry. Absence is the common case.
#[derive(Debug, Default)]
pub struct ItemFingerprintTable {
    entries: HashMap<NodeId, String>,
}

impl ItemFingerprintTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Insert a Let-item → fingerprint name mapping (overwrites any prior entry).
    ///
    /// Duplicate `@fingerprint` on the same Let binding is a parse-time
    /// error (P0250 via the shared attribute-duplicate path); this table
    /// only stores, never validates.
    pub fn insert(&mut self, let_id: NodeId, fp_name: String) {
        self.entries.insert(let_id, fp_name);
    }

    /// Look up the registered fingerprint name for a Let item.
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

    /// Iterate over `(Let-id, fingerprint-name)` pairs.
    ///
    /// Iteration order is unspecified (HashMap-backed); callers that need
    /// a deterministic order should collect and sort by `NodeId` — the
    /// elaborator's `fingerprint_emit` pass does exactly that so the
    /// resulting `.rodata` layout is byte-stable across builds.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &str)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fingerprint tag: r220m10-fp-02
    #[test]
    fn new_is_empty() {
        let t = ItemFingerprintTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    // Fingerprint tag: r220m10-fp-03
    #[test]
    fn insert_and_get() {
        let mut t = ItemFingerprintTable::new();
        let id = NodeId::new(7).unwrap();
        t.insert(id, "test.turn.001".to_string());
        assert_eq!(t.get(id), Some("test.turn.001"));
        assert_eq!(t.len(), 1);
    }

    // Fingerprint tag: r220m10-fp-04
    #[test]
    fn get_absent_returns_none() {
        let t = ItemFingerprintTable::new();
        assert!(t.get(NodeId::new(1).unwrap()).is_none());
    }

    // Fingerprint tag: r220m10-fp-05
    #[test]
    fn insert_overwrites() {
        let mut t = ItemFingerprintTable::new();
        let id = NodeId::new(3).unwrap();
        t.insert(id, "foo".to_string());
        t.insert(id, "bar".to_string());
        assert_eq!(t.get(id), Some("bar"));
        assert_eq!(t.len(), 1);
    }

    // Fingerprint tag: r220m10-fp-06
    #[test]
    fn iter_sees_all_entries() {
        let mut t = ItemFingerprintTable::new();
        t.insert(NodeId::new(1).unwrap(), "a".to_string());
        t.insert(NodeId::new(2).unwrap(), "b".to_string());
        let mut seen: Vec<_> = t.iter().map(|(_, n)| n.to_string()).collect();
        seen.sort();
        assert_eq!(seen, vec!["a".to_string(), "b".to_string()]);
    }
}
