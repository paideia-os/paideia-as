//! Symbol table for top-level bindings per `design/ir/symbol-table.md`.
//!
//! Tracks function and object definitions at module level, indexing by name
//! for efficient lookup. Special handling for `_start` entry-point.

use crate::IrNodeId;
use crate::let_meta::CallingConvention;
use crate::record_layout::RecordLayout;
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
///
/// PAS-DEBT-B4-002 Slice A (#1554): the `return_record_layout` field is the
/// side-table for record-typed return values on function symbols. When a
/// function's declared return type is a record (either the anonymous
/// `record { … }` shape or a named struct in return position), the
/// elaborator's `populate_return_record_layouts` pass computes a
/// `RecordLayout` from the declaration and stamps it here so the
/// downstream emit pipeline can drive SysV/MS aggregate-return
/// classification (Slice B) and record-cons pair-unpack (Slice C).
/// `None` on every other symbol — the historical case (scalar return,
/// non-function binding, or a return type the layout computation did
/// not recognise).
///
/// The field is not part of the symbol's identity: `Hash` and `Eq` skip
/// it so redefining a symbol with the same name / kind / ir_node still
/// replaces in place inside `SymbolTable::insert`. Two symbols that
/// differ only in this field are treated as the same key.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// The binding name (identifier).
    pub name: String,
    /// Discriminant: Function or Object.
    pub kind: SymbolKind,
    /// IrNodeId of the corresponding Let node.
    pub ir_node: IrNodeId,
    /// Visibility level: Local or Global.
    pub visibility: Visibility,
    /// Calling convention annotation (if present).
    pub abi: Option<CallingConvention>,
    /// Return-value record layout when this symbol is a function whose
    /// declared return type is a record. `None` for scalar returns,
    /// non-function bindings, and record returns whose layout the
    /// elaborator could not compute (unsupported field type, unresolved
    /// name, etc.). See PAS-DEBT-B4-002 Slice A (#1554).
    pub return_record_layout: Option<RecordLayout>,
    /// PAS-DEBT-B4-002-followup Gap B (paideia-as#1559): opt-out flag
    /// gating the callee-side Slice-C splice in
    /// `emit_walker::emit_core::emit_ret`.
    ///
    /// When `true`, `emit_callee_sret_splice` early-returns before
    /// emitting its scaffold-buffer allocation and `sysv_callee_*`
    /// helper stream — the enclosing Lambda's own body is expected to
    /// have already placed the return value into the ABI-appropriate
    /// register(s) or sret buffer (e.g. a hand-written `unsafe {
    /// block: { ... } }` raw-asm body, or a synthetic Symbol standing
    /// in for a stdlib recipe whose caller-inlined instructions do
    /// the packing directly). Skipping the splice is what preserves
    /// those hand-written stores; the historical (Wave 45) behaviour
    /// would append a duplicate copy sequence over uninitialised
    /// source bytes, clobbering the intentional stores.
    ///
    /// `false` (the default on every constructor) preserves existing
    /// behaviour for every user-code record-returning callee whose
    /// body is a `RecordCons` expression the Slice-D populator can
    /// fold — the historical corpus.
    ///
    /// Populated by:
    ///   * `return_record_layout_pass::populate_return_record_layouts`
    ///     when it injects a synthetic Symbol for a record-returning
    ///     stdlib recipe (`enumerate_record_return_recipes`) — the
    ///     flag mirrors the recipe's own `skip_sret_splice` field.
    ///   * (Future) an attribute-driven pass reading a
    ///     `#[skip_sret_splice]` marker on a Let, for hand-written
    ///     raw-asm Lambda bodies. Deferred to a follow-up; today the
    ///     mechanism exists only for the recipe-injector path.
    ///
    /// Like `return_record_layout`, this field is metadata and is
    /// excluded from `Hash`/`Eq`/`PartialEq` so an insert-then-
    /// populate flow does not produce two distinct table entries for
    /// the same binding.
    pub skip_sret_splice: bool,
}

// Hand-written PartialEq / Eq / Hash — exclude `return_record_layout` so
// symbol identity stays name / kind / ir_node / visibility / abi. The
// layout is derived metadata that a later pass may fill in without
// changing what the symbol *is*; if we hashed it, an insert-then-
// populate flow would produce two distinct table entries for the same
// binding.
impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.kind == other.kind
            && self.ir_node == other.ir_node
            && self.visibility == other.visibility
            && self.abi == other.abi
    }
}

impl Eq for Symbol {}

impl std::hash::Hash for Symbol {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.kind.hash(state);
        self.ir_node.hash(state);
        self.visibility.hash(state);
        self.abi.hash(state);
    }
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
            abi: None,
            return_record_layout: None,
            skip_sret_splice: false,
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
            abi: None,
            return_record_layout: None,
            skip_sret_splice: false,
        }
    }

    /// Construct a new symbol with explicit calling convention annotation.
    #[must_use]
    pub fn new_with_abi(
        name: String,
        kind: SymbolKind,
        ir_node: IrNodeId,
        abi: Option<CallingConvention>,
    ) -> Self {
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
            abi,
            return_record_layout: None,
            skip_sret_splice: false,
        }
    }

    /// Attach a return-value record layout, consuming and returning `self`.
    ///
    /// Builder for the PAS-DEBT-B4-002 Slice A (#1554) side-table field.
    /// Pass `None` to explicitly clear a previously attached layout;
    /// pass `Some(layout)` when the function's declared return type is a
    /// record and the elaborator has computed its per-field offsets and
    /// sizes. Callers that do not care about aggregate returns can
    /// ignore this builder — the constructors default the field to
    /// `None`.
    #[must_use]
    pub fn with_return_record_layout(mut self, layout: Option<RecordLayout>) -> Self {
        self.return_record_layout = layout;
        self
    }

    /// Attach the Slice-C splice-suppression flag, consuming and
    /// returning `self`.
    ///
    /// Builder for the PAS-DEBT-B4-002-followup Gap B (#1559) opt-out.
    /// Pass `true` to make `emit_walker::emit_core::emit_ret` skip its
    /// `emit_callee_sret_splice` call for the enclosing Lambda —
    /// necessary when the body has already placed the return value
    /// into the ABI-appropriate register(s) or sret buffer (a hand-
    /// written raw-asm body, or a stdlib-recipe synthetic Symbol whose
    /// caller-inlined instructions do the packing directly). Pass
    /// `false` (the default on every constructor) to preserve the
    /// historical splice-on-record-return behaviour.
    #[must_use]
    pub fn with_skip_sret_splice(mut self, skip: bool) -> Self {
        self.skip_sret_splice = skip;
        self
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

    /// Look up a symbol whose `ir_node` equals `ir_node`.
    ///
    /// PAS-DEBT-B4-002 Slice C (paideia-as#1554): `emit_ret` needs to
    /// resolve `current_function` (a Lambda IrNodeId) → owning
    /// `Symbol` so it can consult `return_record_layout` and drive
    /// the callee-side sret splice. `SymbolTable`'s name-indexed
    /// hashmap doesn't help here (only the Lambda id is in hand at
    /// `emit_ret` time — the mangled name may not be), so a linear
    /// scan is the honest fallback. Symbol counts in practice are
    /// small (typically hundreds per module), so the scan cost is
    /// negligible relative to the emission work already happening at
    /// each RET site. If this ever becomes hot, a companion
    /// `by_ir_node: HashMap<IrNodeId, usize>` index alongside
    /// `by_name` would trade a small memory increase for O(1) lookup.
    #[must_use]
    pub fn lookup_by_ir_node(&self, ir_node: IrNodeId) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.ir_node == ir_node)
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

    // ---- PAS-DEBT-B4-002 Slice A (#1554): return_record_layout field ----

    /// Every constructor defaults `return_record_layout` to `None` — a
    /// scalar-return or non-function binding must not carry an aggregate
    /// return descriptor. Slice B / C code that flips a codegen switch
    /// on `Some(_)` needs to be able to trust the absence signal.
    #[test]
    fn return_record_layout_defaults_to_none() {
        use crate::let_meta::CallingConvention;

        let node_id = test_ir_node_id();

        let s1 = Symbol::new("s1".to_string(), SymbolKind::Function, node_id);
        assert!(s1.return_record_layout.is_none());

        let s2 = Symbol::new_with_visibility(
            "s2".to_string(),
            SymbolKind::Function,
            node_id,
            Visibility::Global,
        );
        assert!(s2.return_record_layout.is_none());

        let s3 = Symbol::new_with_abi(
            "s3".to_string(),
            SymbolKind::Function,
            node_id,
            Some(CallingConvention::Sysv),
        );
        assert!(s3.return_record_layout.is_none());
    }

    /// The `with_return_record_layout` builder attaches a layout without
    /// disturbing the other symbol fields; passing `None` clears it.
    /// Test uses a 4×u32 shape (16 B / align 4) — the CpuidRegs case
    /// that drove PAS-DEBT-B4-002 Slice A.
    #[test]
    fn with_return_record_layout_attaches_and_clears() {
        use crate::record_layout::{FieldLayout, RecordLayout};

        let node_id = test_ir_node_id();
        let layout = RecordLayout::with_field_names(
            16,
            4,
            vec![
                FieldLayout { offset: 0,  size: 4, signed: false, is_float: false },
                FieldLayout { offset: 4,  size: 4, signed: false, is_float: false },
                FieldLayout { offset: 8,  size: 4, signed: false, is_float: false },
                FieldLayout { offset: 12, size: 4, signed: false, is_float: false },
            ],
            vec!["eax".to_string(), "ebx".to_string(), "ecx".to_string(), "edx".to_string()],
        );

        let sym = Symbol::new("cpuid_leaf".to_string(), SymbolKind::Function, node_id)
            .with_return_record_layout(Some(layout.clone()));

        assert_eq!(sym.name, "cpuid_leaf");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.return_record_layout, Some(layout));

        let cleared = sym.with_return_record_layout(None);
        assert!(cleared.return_record_layout.is_none());
    }

    /// Symbol identity (Eq / Hash) intentionally excludes the layout —
    /// otherwise a `SymbolTable::insert` that runs AFTER a
    /// populate-layout pass would collide with the pre-layout entry
    /// instead of replacing it. Two symbols that differ only in this
    /// field are the same key.
    #[test]
    fn return_record_layout_not_part_of_identity() {
        use crate::record_layout::{FieldLayout, RecordLayout};
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let node_id = test_ir_node_id();
        let layout = RecordLayout::new(
            8,
            4,
            vec![
                FieldLayout { offset: 0, size: 4, signed: false, is_float: false },
                FieldLayout { offset: 4, size: 4, signed: false, is_float: false },
            ],
        );

        let bare = Symbol::new("cpuid".to_string(), SymbolKind::Function, node_id);
        let with_layout = Symbol::new("cpuid".to_string(), SymbolKind::Function, node_id)
            .with_return_record_layout(Some(layout));

        assert_eq!(bare, with_layout);

        let mut h1 = DefaultHasher::new();
        bare.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        with_layout.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    // ---- PAS-DEBT-B4-002-followup Gap B (#1559): skip_sret_splice ----

    /// Every constructor defaults `skip_sret_splice` to `false` — the
    /// historical behaviour is to fire the splice for any Lambda whose
    /// Symbol carries a `return_record_layout`. Slice-C gate flips
    /// only on an explicit `true`.
    #[test]
    fn skip_sret_splice_defaults_to_false() {
        use crate::let_meta::CallingConvention;
        let node_id = test_ir_node_id();

        let s1 = Symbol::new("s1".to_string(), SymbolKind::Function, node_id);
        assert!(!s1.skip_sret_splice);

        let s2 = Symbol::new_with_visibility(
            "s2".to_string(),
            SymbolKind::Function,
            node_id,
            Visibility::Global,
        );
        assert!(!s2.skip_sret_splice);

        let s3 = Symbol::new_with_abi(
            "s3".to_string(),
            SymbolKind::Function,
            node_id,
            Some(CallingConvention::Sysv),
        );
        assert!(!s3.skip_sret_splice);
    }

    /// The `with_skip_sret_splice` builder attaches the flag without
    /// disturbing other fields; a subsequent call with the opposite
    /// value overwrites it.
    #[test]
    fn with_skip_sret_splice_attaches_and_clears() {
        let node_id = test_ir_node_id();

        let sym = Symbol::new("cpuid_leaf".to_string(), SymbolKind::Function, node_id)
            .with_skip_sret_splice(true);
        assert!(sym.skip_sret_splice);

        let cleared = sym.with_skip_sret_splice(false);
        assert!(!cleared.skip_sret_splice);
    }

    /// Like `return_record_layout`, `skip_sret_splice` is not part of
    /// the symbol's identity — an insert-then-populate flow must not
    /// produce two distinct table entries for the same binding.
    #[test]
    fn skip_sret_splice_not_part_of_identity() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let node_id = test_ir_node_id();
        let bare = Symbol::new("cpuid".to_string(), SymbolKind::Function, node_id);
        let flagged = Symbol::new("cpuid".to_string(), SymbolKind::Function, node_id)
            .with_skip_sret_splice(true);

        assert_eq!(bare, flagged);

        let mut h1 = DefaultHasher::new();
        bare.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        flagged.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    /// The two Slice-C metadata fields (`return_record_layout` and
    /// `skip_sret_splice`) compose cleanly through the builder chain:
    /// the latter attaches without clearing the former, and vice
    /// versa.
    #[test]
    fn builder_chain_layout_then_skip_preserves_both() {
        use crate::record_layout::{FieldLayout, RecordLayout};

        let node_id = test_ir_node_id();
        let layout = RecordLayout::new(
            8,
            4,
            vec![
                FieldLayout { offset: 0, size: 4, signed: false, is_float: false },
                FieldLayout { offset: 4, size: 4, signed: false, is_float: false },
            ],
        );

        let sym = Symbol::new("recipe_cpuid".to_string(), SymbolKind::Function, node_id)
            .with_return_record_layout(Some(layout.clone()))
            .with_skip_sret_splice(true);

        assert_eq!(sym.return_record_layout, Some(layout));
        assert!(sym.skip_sret_splice);
    }
}
