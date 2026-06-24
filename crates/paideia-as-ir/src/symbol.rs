//! Symbol table for top-level bindings per `design/ir/symbol-table.md`.
//!
//! Tracks function and object definitions at module level, indexing by name
//! for efficient lookup. Special handling for `_start` entry-point.

use crate::IrNodeId;
use std::collections::HashMap;

/// Visibility level of a symbol.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum Visibility {
    /// Local symbol (STB_LOCAL in ELF).
    Local,
    /// Global symbol (STB_GLOBAL in ELF).
    Global,
}

/// Variant discriminant for a symbol.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum SymbolKind {
    /// Function binding (Lambda RHS).
    Function,
    /// Object binding (non-Lambda RHS).
    Object,
    /// Undefined (placeholder).
    Undefined,
}

/// A top-level binding symbol.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Symbol {
    /// The binding name (identifier).
    pub name: String,
    /// Discriminant: Function or Object.
    pub kind: SymbolKind,
    /// IrNodeId of the corresponding Let node.
    pub ir_node: IrNodeId,
    /// Visibility level: Local or Global.
    pub visibility: Visibility,
}

impl Symbol {
    /// Construct a new symbol with auto-global rule (PA10-013 backward compatibility).
    ///
    /// Per PA10-013: only _start and long_mode_entry are auto-global.
    /// For explicit export control, use `new_with_visibility()`.
    #[must_use]
    pub fn new(name: String, kind: SymbolKind, ir_node: IrNodeId) -> Self {
        // PA10-013: Revert PA10-009's over-broad STB_GLOBAL marking.
        // Restore local-by-default: only _start and long_mode_entry are global.
        // B2-003 (paideia-os) requires long_mode_entry to be global for cross-module ljmp.
        let visibility = if name == "_start" || name == "long_mode_entry" {
            Visibility::Global
        } else {
            Visibility::Local
        };
        Self {
            name,
            kind,
            ir_node,
            visibility,
        }
    }

    /// Construct a new symbol with explicit visibility control.
    #[must_use]
    pub fn new_with_visibility(
        name: String,
        kind: SymbolKind,
        ir_node: IrNodeId,
        visibility: Visibility,
    ) -> Self {
        Self {
            name,
            kind,
            ir_node,
            visibility,
        }
    }
}

/// Symbol table for module-level bindings.
///
/// Maintains insertion order, a by-name lookup map, and tracks the `_start`
/// entry-point (if present).
#[derive(Default, Debug, Clone)]
pub struct SymbolTable {
    /// Symbols in insertion order.
    symbols: Vec<Symbol>,
    /// Index map: name → position in symbols vec.
    by_name: HashMap<String, usize>,
    /// Index of the _start entry-point, if any.
    entry_point: Option<usize>,
}

impl SymbolTable {
    /// Construct an empty symbol table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a symbol and return its index.
    ///
    /// If a symbol with the same name already exists, it is replaced.
    /// If the symbol is named `_start`, it is registered as the entry-point.
    pub fn insert(&mut self, sym: Symbol) -> usize {
        let idx = if let Some(&existing_idx) = self.by_name.get(&sym.name) {
            // Replace existing symbol at the same position.
            self.symbols[existing_idx] = sym.clone();
            existing_idx
        } else {
            // Add new symbol.
            let idx = self.symbols.len();
            self.symbols.push(sym.clone());
            self.by_name.insert(sym.name.clone(), idx);
            idx
        };

        // Track entry-point.
        if sym.name == "_start" {
            self.entry_point = Some(idx);
        }

        idx
    }

    /// Look up a symbol by name.
    #[must_use]
    pub fn lookup_by_name(&self, name: &str) -> Option<&Symbol> {
        self.by_name
            .get(name)
            .and_then(|&idx| self.symbols.get(idx))
    }

    /// Iterate over all symbols in insertion order.
    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> + '_ {
        self.symbols.iter()
    }

    /// Get the entry-point symbol, if any.
    #[must_use]
    pub fn entry_point(&self) -> Option<&Symbol> {
        self.entry_point.and_then(|idx| self.symbols.get(idx))
    }

    /// Number of symbols in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// True if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Clear all symbols from the table.
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.by_name.clear();
        self.entry_point = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    #[allow(dead_code)]
    fn test_span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn test_ir_node_id() -> IrNodeId {
        IrNodeId::new(1).unwrap()
    }

    #[test]
    fn symbol_new_auto_flags_start_as_global() {
        let sym = Symbol::new(
            "_start".to_string(),
            SymbolKind::Function,
            test_ir_node_id(),
        );
        assert_eq!(sym.visibility, Visibility::Global);
        assert_eq!(sym.name, "_start");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    #[test]
    fn symbol_new_regular_name_not_global() {
        let sym = Symbol::new("foo".to_string(), SymbolKind::Object, test_ir_node_id());
        assert_eq!(sym.visibility, Visibility::Local);
        assert_eq!(sym.name, "foo");
    }

    #[test]
    fn symbol_table_insert_returns_index() {
        let mut st = SymbolTable::new();
        let idx = st.insert(Symbol::new(
            "foo".to_string(),
            SymbolKind::Object,
            test_ir_node_id(),
        ));
        assert_eq!(idx, 0);
    }

    #[test]
    fn symbol_table_lookup_by_name_finds_symbol() {
        let mut st = SymbolTable::new();
        let sym = Symbol::new("foo".to_string(), SymbolKind::Object, test_ir_node_id());
        st.insert(sym.clone());

        let found = st.lookup_by_name("foo");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo");
        assert_eq!(found.unwrap().kind, SymbolKind::Object);
    }

    #[test]
    fn symbol_table_lookup_by_name_not_found() {
        let st = SymbolTable::new();
        assert!(st.lookup_by_name("missing").is_none());
    }

    #[test]
    fn symbol_table_iter_preserves_insertion_order() {
        let mut st = SymbolTable::new();
        st.insert(Symbol::new(
            "first".to_string(),
            SymbolKind::Object,
            test_ir_node_id(),
        ));
        st.insert(Symbol::new(
            "second".to_string(),
            SymbolKind::Function,
            test_ir_node_id(),
        ));
        st.insert(Symbol::new(
            "third".to_string(),
            SymbolKind::Object,
            test_ir_node_id(),
        ));

        let names: Vec<_> = st.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn symbol_table_entry_point_found() {
        let mut st = SymbolTable::new();
        let start_id = IrNodeId::new(42).unwrap();
        let sym = Symbol::new("_start".to_string(), SymbolKind::Function, start_id);
        st.insert(sym);

        let ep = st.entry_point();
        assert!(ep.is_some());
        assert_eq!(ep.unwrap().name, "_start");
        assert_eq!(ep.unwrap().ir_node, start_id);
    }

    #[test]
    fn symbol_table_entry_point_not_found() {
        let mut st = SymbolTable::new();
        st.insert(Symbol::new(
            "foo".to_string(),
            SymbolKind::Object,
            test_ir_node_id(),
        ));
        assert!(st.entry_point().is_none());
    }

    #[test]
    fn symbol_table_len_and_empty() {
        let mut st = SymbolTable::new();
        assert!(st.is_empty());
        assert_eq!(st.len(), 0);

        st.insert(Symbol::new(
            "foo".to_string(),
            SymbolKind::Object,
            test_ir_node_id(),
        ));
        assert!(!st.is_empty());
        assert_eq!(st.len(), 1);
    }

    // Acceptance criteria test 1: let foo : u64 = 42 → one Object symbol
    #[test]
    fn ac_test_object_binding() {
        let mut st = SymbolTable::new();
        let node_id = IrNodeId::new(1).unwrap();
        let sym = Symbol::new("foo".to_string(), SymbolKind::Object, node_id);
        st.insert(sym);

        assert_eq!(st.len(), 1);
        let found = st.lookup_by_name("foo").unwrap();
        assert_eq!(found.kind, SymbolKind::Object);
        assert_eq!(found.visibility, Visibility::Local);
    }

    // Acceptance criteria test 2: let add_one : (u64) -> u64 = fn ... → one Function symbol
    // PA10-013: Function symbols are local by default (unless explicitly exported via 'pub').
    #[test]
    fn ac_test_function_binding() {
        let mut st = SymbolTable::new();
        let node_id = IrNodeId::new(2).unwrap();
        let sym = Symbol::new("add_one".to_string(), SymbolKind::Function, node_id);
        st.insert(sym);

        assert_eq!(st.len(), 1);
        let found = st.lookup_by_name("add_one").unwrap();
        assert_eq!(found.kind, SymbolKind::Function);
        assert_eq!(found.visibility, Visibility::Local); // PA10-013: functions are local by default
    }

    // Acceptance criteria test 3: let _start : () -> () = fn () -> ... → marked as entry-point
    #[test]
    fn ac_test_start_entry_point() {
        let mut st = SymbolTable::new();
        let node_id = IrNodeId::new(3).unwrap();
        let sym = Symbol::new("_start".to_string(), SymbolKind::Function, node_id);
        st.insert(sym);

        assert_eq!(st.len(), 1);
        let found = st.lookup_by_name("_start").unwrap();
        assert_eq!(found.kind, SymbolKind::Function);
        assert_eq!(found.visibility, Visibility::Global); // Auto-flagged as global

        // Entry-point lookup
        let ep = st.entry_point().unwrap();
        assert_eq!(ep.name, "_start");
        assert_eq!(ep.visibility, Visibility::Global);
    }
}
