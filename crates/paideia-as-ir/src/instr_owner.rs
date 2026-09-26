//! Instruction-owner side-table: maps each emitted instruction to the
//! symbol name of the enclosing top-level function.
//!
//! PAS-DEBT-B3-001: enables the tail-call pass to identify self-recursion
//! (call target == owning function) without threading elaborator state
//! into the opt catalog. Populate from the elaborator's `instr_to_lambda`
//! + `SymbolTable` reverse lookup; consume in `opt::tailcall`.
//!
//! Pattern follows `binding_name::BindingNameTable`.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Sparse mapping: instruction IrNodeId → owning function symbol name.
///
/// An absent entry means the pass has no ownership evidence for the
/// instruction; passes that need self-owner proof (tail-call) must
/// treat "absent" as "cannot prove" and refuse to rewrite.
#[derive(Default, Debug, Clone)]
pub struct InstrOwnerTable {
    entries: HashMap<IrNodeId, String>,
}

impl InstrOwnerTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the owning function symbol name for an instruction.
    pub fn insert(&mut self, instr_id: IrNodeId, owner: String) {
        self.entries.insert(instr_id, owner);
    }

    /// Owner name for `instr_id`, if recorded.
    #[must_use]
    pub fn get(&self, instr_id: IrNodeId) -> Option<&str> {
        self.entries.get(&instr_id).map(String::as_str)
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
    fn insert_and_get() {
        let mut t = InstrOwnerTable::new();
        let id = IrNodeId::new(7).unwrap();
        t.insert(id, "factorial".to_string());
        assert_eq!(t.get(id), Some("factorial"));
    }

    #[test]
    fn missing_returns_none() {
        let t = InstrOwnerTable::new();
        assert_eq!(t.get(IrNodeId::new(1).unwrap()), None);
    }

    #[test]
    fn len_and_clear() {
        let mut t = InstrOwnerTable::new();
        assert!(t.is_empty());
        t.insert(IrNodeId::new(1).unwrap(), "f".to_string());
        t.insert(IrNodeId::new(2).unwrap(), "g".to_string());
        assert_eq!(t.len(), 2);
        t.clear();
        assert!(t.is_empty());
    }
}
