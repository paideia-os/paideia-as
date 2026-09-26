//! Function/callsite capability side-tables.
//!
//! PAS-DEBT-B3-002 (#1515): tightens the tail-call pass with a capability
//! boundary guard. Two sparse tables:
//!
//! - [`FnDeclaredCapsTable`] — per top-level function symbol → the cap set
//!   the function is declared to hold in its signature.
//! - [`CallSiteRequiredCapsTable`] — per Call IrNodeId → the cap set the
//!   callee requires at that site.
//!
//! The tail-call pass reads both. If either is absent, no cap evidence exists
//! and the check is silent. If both are present and disagree, TCO is refused
//! — eliding the callee's frame across a cap boundary is unsound.
//!
//! Both use sorted `BTreeSet<String>` payloads so equality is order-stable.
//!
//! Populated by the elaborator (call-site) and the type checker (declaration).
//! Both hooks land as follow-up B3-002b — the pass survives an unpopulated
//! table today (returns None → no blocker).
//!
//! Pattern mirrors [`crate::instr_owner::InstrOwnerTable`].

use crate::node::IrNodeId;
use std::collections::{BTreeSet, HashMap};

/// Sparse map: top-level function symbol name → declared capability set.
///
/// An absent entry means the pass has no declaration evidence for that
/// function; the tail-call pass then relies on other checks only.
#[derive(Default, Debug, Clone)]
pub struct FnDeclaredCapsTable {
    entries: HashMap<String, BTreeSet<String>>,
}

impl FnDeclaredCapsTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the declared cap set for a function.
    pub fn insert(&mut self, owner: String, caps: BTreeSet<String>) {
        self.entries.insert(owner, caps);
    }

    /// Declared cap set for `owner`, if recorded.
    #[must_use]
    pub fn get(&self, owner: &str) -> Option<&BTreeSet<String>> {
        self.entries.get(owner)
    }

    /// True iff no entries have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drop all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Sparse map: Call IrNodeId → cap set the callee requires at that site.
///
/// Indexed by the Call site rather than by callee name so that a
/// polymorphic-callee mismatch can be surfaced at the specific invocation.
#[derive(Default, Debug, Clone)]
pub struct CallSiteRequiredCapsTable {
    entries: HashMap<IrNodeId, BTreeSet<String>>,
}

impl CallSiteRequiredCapsTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the required cap set for a Call site.
    pub fn insert(&mut self, call_id: IrNodeId, caps: BTreeSet<String>) {
        self.entries.insert(call_id, caps);
    }

    /// Required cap set for `call_id`, if recorded.
    #[must_use]
    pub fn get(&self, call_id: IrNodeId) -> Option<&BTreeSet<String>> {
        self.entries.get(&call_id)
    }

    /// True iff no entries have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drop all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fn_declared_caps_insert_and_get() {
        let mut t = FnDeclaredCapsTable::new();
        let mut caps = BTreeSet::new();
        caps.insert("Fs".to_string());
        caps.insert("Net".to_string());
        t.insert("worker".to_string(), caps.clone());
        assert_eq!(t.get("worker"), Some(&caps));
        assert_eq!(t.get("unknown"), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn call_site_required_caps_insert_and_get() {
        let mut t = CallSiteRequiredCapsTable::new();
        let call_id = IrNodeId::new(7).unwrap();
        let mut caps = BTreeSet::new();
        caps.insert("Fs".to_string());
        t.insert(call_id, caps.clone());
        assert_eq!(t.get(call_id), Some(&caps));
        assert_eq!(t.get(IrNodeId::new(1).unwrap()), None);
    }

    #[test]
    fn clear_empties_both_tables() {
        let mut a = FnDeclaredCapsTable::new();
        a.insert("f".to_string(), BTreeSet::new());
        a.clear();
        assert!(a.is_empty());

        let mut b = CallSiteRequiredCapsTable::new();
        b.insert(IrNodeId::new(1).unwrap(), BTreeSet::new());
        b.clear();
        assert!(b.is_empty());
    }

    #[test]
    fn set_equality_is_order_independent() {
        // BTreeSet iteration is sorted, so two sets built in different
        // insertion orders compare equal.
        let mut a = BTreeSet::new();
        a.insert("A".to_string());
        a.insert("B".to_string());
        let mut b = BTreeSet::new();
        b.insert("B".to_string());
        b.insert("A".to_string());
        assert_eq!(a, b);
    }
}
