//! Effect declarations and operation signatures registry.
//!
//! `EffectRegistry` records which effects exist, each with its
//! operations. Per `custom-assembler.md` §4, an effect is a set of
//! named operations with signatures (parameter types + return type).
//! Operations are looked up as dotted paths like `Io.port_read`.
//!
//! Phase-1 stores operation signatures as opaque `u32` ids — the type
//! interner from `paideia-as-types` lives in a sibling crate and we
//! avoid the cyclic dependency by accepting any external id here.

use std::collections::{HashMap, HashSet};

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};

use crate::row::EffectId;

/// Opaque external signature id (a TypeId from paideia-as-types, kept
/// crate-decoupled so paideia-as-effects doesn't depend on
/// paideia-as-types).
pub type SignatureId = u32;

/// Diagnostic code for incompatible effect re-declarations.
pub const F_REDECL_MISMATCH: u16 = 1101;

/// One operation inside an effect.
#[derive(Copy, Clone, Debug)]
pub struct Operation {
    /// Effect this operation belongs to.
    pub effect: EffectId,
    /// Interned signature id (typically a `TypeId` from the type
    /// interner).
    pub signature: SignatureId,
    /// Source span of the declaration.
    pub decl_span: Span,
}

/// Registry of effects, their operations, and dotted-path lookup.
#[derive(Default, Debug)]
pub struct EffectRegistry {
    /// Reverse lookup: effect name → interned EffectId.
    effect_ids: HashMap<String, EffectId>,
    /// Forward lookup: EffectId → declared effect name.
    effect_names: HashMap<EffectId, String>,
    /// Operations keyed by dotted path "EffectName.OpName".
    operations: HashMap<String, Operation>,
    /// For F1101 detection: known operation-name set per effect.
    effect_op_names: HashMap<EffectId, HashSet<String>>,
    /// Next free EffectId.
    next_id: u32,
}

impl EffectRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern an effect name (returns an existing id on collision).
    pub fn intern_effect(&mut self, name: &str) -> EffectId {
        if let Some(&id) = self.effect_ids.get(name) {
            return id;
        }
        self.next_id += 1;
        let id = EffectId::new(self.next_id).expect("non-zero next id");
        self.effect_ids.insert(name.to_owned(), id);
        self.effect_names.insert(id, name.to_owned());
        self.effect_op_names.insert(id, HashSet::new());
        id
    }

    /// Declare an effect with its full set of operation `(op_name,
    /// signature, decl_span)` triples.
    ///
    /// If `name` was previously declared with a different op-name set,
    /// returns one F1101 diagnostic (the new ops are still applied so
    /// the rest of the program type-checks; the diagnostic is the only
    /// surface signal).
    pub fn declare_effect(
        &mut self,
        name: &str,
        ops: &[(String, SignatureId, Span)],
        decl_span: Span,
    ) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let effect_id = self.intern_effect(name);

        // Compute the proposed op-name set.
        let new_names: HashSet<String> = ops.iter().map(|(n, _, _)| n.clone()).collect();

        // F1101: if this effect was already populated with a different
        // op-name set, flag a mismatch.
        let existing = self
            .effect_op_names
            .get(&effect_id)
            .cloned()
            .unwrap_or_default();
        if !existing.is_empty() && existing != new_names {
            let diff: Vec<_> = existing.symmetric_difference(&new_names).collect();
            let mut sorted_diff: Vec<&String> = diff.into_iter().collect();
            sorted_diff.sort();
            let message = format!(
                "effect `{name}` re-declared with a different operation set; \
                 differences: {sorted_diff:?}"
            );
            diags.push(
                Diagnostic::error(f_code(F_REDECL_MISMATCH))
                    .message(message)
                    .with_span(decl_span)
                    .finish(),
            );
        }

        // Apply the new op set (overwrites prior state for this effect).
        self.effect_op_names.insert(effect_id, new_names);
        for (op_name, sig, span) in ops {
            let path = format!("{name}.{op_name}");
            self.operations.insert(
                path,
                Operation {
                    effect: effect_id,
                    signature: *sig,
                    decl_span: *span,
                },
            );
        }
        diags
    }

    /// Look up an operation by its dotted path (e.g., `Io.port_read`).
    #[must_use]
    pub fn lookup_op(&self, path: &str) -> Option<Operation> {
        self.operations.get(path).copied()
    }

    /// Look up an effect id by its name.
    #[must_use]
    pub fn lookup_effect(&self, name: &str) -> Option<EffectId> {
        self.effect_ids.get(name).copied()
    }

    /// All effects declared so far.
    #[must_use]
    pub fn effects(&self) -> Vec<(EffectId, &str)> {
        let mut out: Vec<_> = self
            .effect_names
            .iter()
            .map(|(id, name)| (*id, name.as_str()))
            .collect();
        out.sort_by_key(|(id, _)| id.get());
        out
    }

    /// Register the three Diag / Elab / FreshName effects that macro
    /// bodies are permitted to use. Each is a simple kind-only effect
    /// with one op stub for phase-2-m9; the operations themselves are
    /// wired through their builtin call paths (Diag → ctx.emit,
    /// Elab → elab_builtin, FreshName → not yet implemented).
    ///
    /// This method is idempotent: calling it multiple times is safe and
    /// produces the same effect IDs. Per `custom-assembler.md` §5.4
    /// (macro-pure context), these are the only effects permitted inside
    /// macro bodies.
    ///
    /// Phase-2-m9: the term_eval evaluator is pure-functional plus the
    /// four builtins (kind/children/span/splice/elab). None of these emit
    /// effects outside MACRO_EFFECT_ROW today, so the enforcement is
    /// structurally vacuous. The check is wired so that when m3 / m5 add
    /// user-source effect statements inside macro bodies, the violations
    /// fire automatically.
    pub fn register_macro_effects(&mut self) {
        // Use a sentinel FileId and Span for these synthesized effect declarations.
        let sentinel_span = Span::new(
            FileId::new(u32::MAX).expect("u32::MAX is valid FileId"),
            0,
            0,
        );
        let _ = self.declare_effect("Diag", &[("emit".into(), 0, sentinel_span)], sentinel_span);
        let _ = self.declare_effect("Elab", &[("elab".into(), 0, sentinel_span)], sentinel_span);
        let _ = self.declare_effect(
            "FreshName",
            &[("fresh".into(), 0, sentinel_span)],
            sentinel_span,
        );
    }

    /// The fixed effect row permitted inside macro bodies per
    /// `custom-assembler.md` §5.4 (macro-pure context).
    ///
    /// Must only be called after `register_macro_effects` has been invoked.
    /// Returns the row `!{Diag, Elab, FreshName}` containing the three
    /// hygienic macro effects.
    ///
    /// Panics if the macro effects have not yet been registered (i.e.,
    /// if register_macro_effects was not called first).
    #[must_use]
    pub fn macro_effect_row(&self) -> crate::EffectRow {
        let diag = self
            .lookup_effect("Diag")
            .expect("Diag registered via register_macro_effects");
        let elab = self
            .lookup_effect("Elab")
            .expect("Elab registered via register_macro_effects");
        let fresh = self
            .lookup_effect("FreshName")
            .expect("FreshName registered via register_macro_effects");
        crate::EffectRow::from_ids(vec![diag, elab, fresh], None)
    }
}

fn f_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::F, Severity::Error, n).expect("valid F code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    #[test]
    fn declare_and_lookup_op() {
        let mut reg = EffectRegistry::new();
        let diags = reg.declare_effect("Io", &[("port_read".to_string(), 42, span(0))], span(10));
        assert!(diags.is_empty());
        let op = reg.lookup_op("Io.port_read").unwrap();
        assert_eq!(op.signature, 42);
    }

    #[test]
    fn lookup_unknown_returns_none() {
        let reg = EffectRegistry::new();
        assert!(reg.lookup_op("Io.port_read").is_none());
    }

    #[test]
    fn redeclaration_with_different_op_set_emits_f1101() {
        let mut reg = EffectRegistry::new();
        let _ = reg.declare_effect("Io", &[("a".into(), 1, span(0))], span(0));
        let diags = reg.declare_effect(
            "Io",
            &[("a".into(), 1, span(0)), ("b".into(), 2, span(0))],
            span(20),
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1101);
        assert_eq!(diags[0].code().category(), Category::F);
        // The new op is registered despite the mismatch.
        assert!(reg.lookup_op("Io.b").is_some());
    }

    #[test]
    fn redeclaration_with_same_op_set_no_diagnostic() {
        let mut reg = EffectRegistry::new();
        let _ = reg.declare_effect("Io", &[("a".into(), 1, span(0))], span(0));
        let diags = reg.declare_effect("Io", &[("a".into(), 1, span(0))], span(20));
        assert!(diags.is_empty());
    }

    #[test]
    fn empty_op_list_does_not_panic() {
        // Safety net for parser-rejected inputs that still reach the
        // registry.
        let mut reg = EffectRegistry::new();
        let diags = reg.declare_effect("Empty", &[], span(0));
        assert!(diags.is_empty());
        assert!(reg.lookup_effect("Empty").is_some());
    }

    #[test]
    fn intern_effect_is_idempotent() {
        let mut reg = EffectRegistry::new();
        let id1 = reg.intern_effect("Io");
        let id2 = reg.intern_effect("Io");
        assert_eq!(id1, id2);
        let id3 = reg.intern_effect("Mmio");
        assert_ne!(id1, id3);
    }

    #[test]
    fn effects_iter_is_id_sorted() {
        let mut reg = EffectRegistry::new();
        reg.intern_effect("Io");
        reg.intern_effect("Mmio");
        reg.intern_effect("Net");
        let effects = reg.effects();
        assert_eq!(effects.len(), 3);
        assert!(effects[0].0.get() < effects[1].0.get());
        assert!(effects[1].0.get() < effects[2].0.get());
    }

    #[test]
    fn register_macro_effects_idempotent() {
        let mut reg = EffectRegistry::new();
        reg.register_macro_effects();
        let diag1 = reg.lookup_effect("Diag").unwrap();
        let elab1 = reg.lookup_effect("Elab").unwrap();
        let fresh1 = reg.lookup_effect("FreshName").unwrap();

        // Call again
        reg.register_macro_effects();
        let diag2 = reg.lookup_effect("Diag").unwrap();
        let elab2 = reg.lookup_effect("Elab").unwrap();
        let fresh2 = reg.lookup_effect("FreshName").unwrap();

        // Same IDs (idempotent)
        assert_eq!(diag1, diag2);
        assert_eq!(elab1, elab2);
        assert_eq!(fresh1, fresh2);

        // Only 3 effects in the registry
        let all_effects = reg.effects();
        assert_eq!(all_effects.len(), 3);
    }

    #[test]
    fn macro_effect_row_contains_diag_elab_freshname() {
        let mut reg = EffectRegistry::new();
        reg.register_macro_effects();
        let row = reg.macro_effect_row();

        // The row should contain exactly three effects
        assert_eq!(row.fixed.len(), 3);

        // The row should not have a tail variable
        assert!(row.tail.is_none());

        // Verify it's the correct three effects by checking they can be looked up
        let diag = reg.lookup_effect("Diag").unwrap();
        let elab = reg.lookup_effect("Elab").unwrap();
        let fresh = reg.lookup_effect("FreshName").unwrap();

        assert!(row.fixed.contains(&diag));
        assert!(row.fixed.contains(&elab));
        assert!(row.fixed.contains(&fresh));
    }
}
