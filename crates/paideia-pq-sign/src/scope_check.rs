//! Delegation-scope checking for post-quantum signing.
//!
//! # Load-bearing rank-5-elaborator-reflection use case
//!
//! This module implements the scope subsumption check required by OS-requirements §4 N1
//! and pq-trust-root.md §12/§13. When a signing key signs a PAX (paideia-as artifact),
//! the key's authorized effect scope must subsume the PAX's effect set.
//!
//! # Reflection Pattern
//!
//! The signer reads `.paideia.effects` (from m4-004's PAX emitter section), reflects
//! on the elaborated effect signatures, and checks whether the key's scope is
//! authorized to sign artifacts using those effects.
//!
//! If `pax.effects ⊆ key.scope`, signing succeeds. Otherwise, emits **Q0901**
//! (signing-key scope insufficient).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_emitter_pax::effects::EffectsSection;
use std::collections::BTreeSet;

/// Diagnostic code for insufficient key scope (Q0901).
pub const Q_SCOPE_INSUFFICIENT: u16 = 901;

/// A signing key's authorized effect scope.
///
/// Represents the set of effect IDs a key is authorized to sign for.
/// For a valid signing operation, the PAX's required effect set must be
/// a subset of this scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyScope {
    /// Effect IDs the key is authorized to sign for.
    effects: BTreeSet<u32>,
}

impl KeyScope {
    /// Create a new, empty key scope.
    pub fn new() -> Self {
        Self {
            effects: BTreeSet::new(),
        }
    }

    /// Add an effect ID to the key's authorized scope.
    pub fn add(&mut self, effect_id: u32) {
        self.effects.insert(effect_id);
    }

    /// Check whether the key's scope authorizes all required effects.
    ///
    /// Returns `true` if `required ⊆ self.effects`, `false` otherwise.
    pub fn allows_all(&self, required: &BTreeSet<u32>) -> bool {
        required.is_subset(&self.effects)
    }

    /// Get a reference to the underlying effect set.
    #[doc(hidden)]
    pub fn effects(&self) -> &BTreeSet<u32> {
        &self.effects
    }
}

impl Default for KeyScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract all fixed effects from a PAX `.paideia.effects` section.
///
/// Returns a flat set of all effect IDs declared as fixed effects
/// in any of the section's entries.
pub fn pax_effects_required(effects: &EffectsSection) -> BTreeSet<u32> {
    let mut set = BTreeSet::new();
    for entry in &effects.entries {
        for eid in &entry.fixed_effects {
            set.insert(*eid);
        }
    }
    set
}

/// Check that a signing key's scope subsumes the PAX's required effects.
///
/// If the check passes, returns `true` and emits no diagnostic.
/// If the check fails (key scope is insufficient), returns `false` and emits
/// a Q0901 error diagnostic with details about missing effects.
///
/// # Arguments
///
/// * `key_scope` - The signing key's authorized effect scope
/// * `effects` - The PAX's `.paideia.effects` section (read via rank-5 reflection)
/// * `diags` - Mutable diagnostic sink for error emission
///
/// # Returns
///
/// `true` if `pax.effects ⊆ key.scope`, `false` otherwise.
pub fn check_delegation_scope(
    key_scope: &KeyScope,
    effects: &EffectsSection,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let required = pax_effects_required(effects);

    if key_scope.allows_all(&required) {
        true
    } else {
        let missing: Vec<u32> = required.difference(&key_scope.effects).copied().collect();

        let code = DiagnosticCode::new(Category::Q, Severity::Error, Q_SCOPE_INSUFFICIENT)
            .expect("Q0901 is within valid range");

        diags.push(
            Diagnostic::error(code)
                .message(format!(
                    "signing-key scope insufficient: artifact requires effects {:?}, \
                     key authorizes {:?}, missing {:?}",
                    required, key_scope.effects, missing,
                ))
                .finish(),
        );

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_emitter_pax::effects::EffectRowEntry;

    /// Test 1: Empty required effect set always passes.
    #[test]
    fn empty_required_passes() {
        let key_scope = KeyScope::new();
        let effects = EffectsSection::new();
        let mut diags = Vec::new();

        assert!(check_delegation_scope(&key_scope, &effects, &mut diags));
        assert_eq!(
            diags.len(),
            0,
            "should emit no diagnostic for empty required set"
        );
    }

    /// Test 2: Key scope subsumes required effects → success.
    #[test]
    fn key_scope_subsumes_required_succeeds() {
        let mut key_scope = KeyScope::new();
        key_scope.add(1); // Net
        key_scope.add(2); // Sched

        let mut effects = EffectsSection::new();
        let entry = EffectRowEntry::new(100, vec![1, 2], None);
        effects.push(entry);

        let mut diags = Vec::new();

        assert!(check_delegation_scope(&key_scope, &effects, &mut diags));
        assert_eq!(
            diags.len(),
            0,
            "should emit no diagnostic when scope subsumes"
        );
    }

    /// Test 3: Key scope insufficient → Q0901 emitted.
    /// AC 1: key {Net, Sched}; required {Net, Sched, FS} → false; diag mentions missing FS.
    #[test]
    fn key_scope_insufficient_emits_q0901() {
        let mut key_scope = KeyScope::new();
        key_scope.add(1); // Net
        key_scope.add(2); // Sched

        let mut effects = EffectsSection::new();
        let entry = EffectRowEntry::new(200, vec![1, 2, 3], None); // Requires Net, Sched, FS
        effects.push(entry);

        let mut diags = Vec::new();

        assert!(!check_delegation_scope(&key_scope, &effects, &mut diags));
        assert_eq!(diags.len(), 1, "should emit exactly one diagnostic");

        let diag = &diags[0];
        assert_eq!(diag.code().number(), Q_SCOPE_INSUFFICIENT);
        assert_eq!(diag.code().category(), Category::Q);

        let msg = diag.message();
        assert!(
            msg.contains("missing"),
            "message must mention missing effects"
        );
        assert!(
            msg.contains("3"),
            "message must mention missing effect ID 3"
        );
    }

    /// Test 4: pax_effects_required unions all effect entries.
    #[test]
    fn pax_effects_required_unions_all_function_rows() {
        let mut effects = EffectsSection::new();

        let e1 = EffectRowEntry::new(111, vec![1, 2], None);
        let e2 = EffectRowEntry::new(222, vec![2, 3], None);
        let e3 = EffectRowEntry::new(333, vec![3, 4], None);

        effects.push(e1);
        effects.push(e2);
        effects.push(e3);

        let required = pax_effects_required(&effects);

        // Union of {1,2}, {2,3}, {3,4} = {1,2,3,4}
        assert_eq!(required, [1, 2, 3, 4].iter().copied().collect());
    }

    /// Test 5: End-to-end check replicating AC 1.
    #[test]
    fn check_delegation_scope_end_to_end_matches_ac() {
        // Setup: key scope {Net=1, Sched=2}
        let mut key_scope = KeyScope::new();
        key_scope.add(1);
        key_scope.add(2);

        // Setup: PAX requires {Net=1, Sched=2, FS=3}
        let mut effects = EffectsSection::new();
        effects.push(EffectRowEntry::new(1000, vec![1, 2, 3], None));

        // Act: check scope
        let mut diags = Vec::new();
        let result = check_delegation_scope(&key_scope, &effects, &mut diags);

        // Assert: check fails, Q0901 emitted
        assert!(!result, "check should fail due to insufficient scope");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), Q_SCOPE_INSUFFICIENT);
        assert_eq!(diags[0].code().category(), Category::Q);

        // Assert: diagnostic message mentions the missing effect
        let msg = diags[0].message();
        assert!(
            msg.contains("missing") && msg.contains("3"),
            "message must mention missing effect ID: {}",
            msg
        );
    }
}
