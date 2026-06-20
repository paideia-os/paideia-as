//! Impl coherence check per m9-004.
//!
//! For each impl block, verify that the (trait_name, for_type_id) pair is unique.
//! If a duplicate is detected, emit T0513 ("duplicate impl for the same (trait, type) pair").
//!
//! **Phase-1 scope (m9-004):**
//! - The coherence check function is implemented and tested.
//! - Wiring into the elaborator's impl-traversal (m9 walker) is deferred to m9-005+.

use paideia_as_ast::NodeId;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use std::collections::HashMap;

/// Diagnostic code for duplicate impl for the same (trait, type) pair.
pub const T_DUPLICATE_IMPL: u16 = 513;

/// A cache tracking (trait_id, for_type_id) → impl_id to detect duplicate impls.
///
/// The key is a pair: (trait_name_id, for_type_id). For inherent impls,
/// trait_name_id is None, so each for_type can have at most one inherent impl.
#[derive(Default, Debug)]
pub struct CoherenceChecker {
    /// Map from (trait_name_id, for_type_id) to the span of the first impl.
    impls: HashMap<(Option<NodeId>, NodeId), Span>,
}

impl CoherenceChecker {
    /// Construct an empty coherence checker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check a single impl block for coherence violations.
    ///
    /// If a (trait_name, for_type) pair already exists in the map, returns
    /// one T0513 diagnostic. Otherwise, records the pair and returns an empty vector.
    #[must_use]
    pub fn check_impl(
        &mut self,
        trait_name: Option<NodeId>,
        for_type: NodeId,
        impl_span: Span,
    ) -> Vec<Diagnostic> {
        let key = (trait_name, for_type);

        if let Some(first_span) = self.impls.get(&key) {
            vec![
                Diagnostic::error(t_code(T_DUPLICATE_IMPL))
                    .message(format!(
                        "duplicate impl block for (trait: {:?}, type: {}): first impl at {:?}",
                        trait_name, for_type, first_span
                    ))
                    .with_span(impl_span)
                    .finish(),
            ]
        } else {
            self.impls.insert(key, impl_span);
            Vec::new()
        }
    }
}

fn t_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn make_span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    #[test]
    fn coherence_single_impl_succeeds() {
        let mut checker = CoherenceChecker::new();
        let trait_id = NodeId::new(1).unwrap();
        let type_id = NodeId::new(2).unwrap();
        let span = make_span(0, 10);

        let diags = checker.check_impl(Some(trait_id), type_id, span);
        assert!(diags.is_empty(), "single impl should pass coherence check");
    }

    #[test]
    fn coherence_duplicate_impl_fires_t0513() {
        let mut checker = CoherenceChecker::new();
        let trait_id = NodeId::new(1).unwrap();
        let type_id = NodeId::new(2).unwrap();
        let span1 = make_span(0, 10);
        let span2 = make_span(20, 10);

        let diags1 = checker.check_impl(Some(trait_id), type_id, span1);
        assert!(diags1.is_empty(), "first impl should pass");

        let diags2 = checker.check_impl(Some(trait_id), type_id, span2);
        assert_eq!(diags2.len(), 1, "second impl should fail coherence");
        assert_eq!(diags2[0].code().number(), T_DUPLICATE_IMPL);
    }

    #[test]
    fn coherence_different_traits_allowed() {
        let mut checker = CoherenceChecker::new();
        let trait1 = NodeId::new(1).unwrap();
        let trait2 = NodeId::new(2).unwrap();
        let type_id = NodeId::new(3).unwrap();
        let span1 = make_span(0, 10);
        let span2 = make_span(20, 10);

        let diags1 = checker.check_impl(Some(trait1), type_id, span1);
        assert!(diags1.is_empty(), "first trait impl should pass");

        let diags2 = checker.check_impl(Some(trait2), type_id, span2);
        assert!(
            diags2.is_empty(),
            "second trait impl (different trait) should pass"
        );
    }

    #[test]
    fn coherence_different_types_allowed() {
        let mut checker = CoherenceChecker::new();
        let trait_id = NodeId::new(1).unwrap();
        let type1 = NodeId::new(2).unwrap();
        let type2 = NodeId::new(3).unwrap();
        let span1 = make_span(0, 10);
        let span2 = make_span(20, 10);

        let diags1 = checker.check_impl(Some(trait_id), type1, span1);
        assert!(diags1.is_empty(), "first impl should pass");

        let diags2 = checker.check_impl(Some(trait_id), type2, span2);
        assert!(
            diags2.is_empty(),
            "second impl for different type should pass"
        );
    }

    #[test]
    fn coherence_duplicate_inherent_impl() {
        let mut checker = CoherenceChecker::new();
        let type_id = NodeId::new(1).unwrap();
        let span1 = make_span(0, 10);
        let span2 = make_span(20, 10);

        let diags1 = checker.check_impl(None, type_id, span1);
        assert!(diags1.is_empty(), "first inherent impl should pass");

        let diags2 = checker.check_impl(None, type_id, span2);
        assert_eq!(diags2.len(), 1, "second inherent impl should fail");
        assert_eq!(diags2[0].code().number(), T_DUPLICATE_IMPL);
    }
}
