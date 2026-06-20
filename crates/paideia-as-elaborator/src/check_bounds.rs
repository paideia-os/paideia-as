//! Trait-bound resolution at call sites.
//!
//! For each call to `T::method` where `T: SomeTrait`, verify either:
//! - T is bound to SomeTrait in the enclosing generic-parameter list.
//! - There's a concrete impl of SomeTrait for the substituted type at this call site.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use std::collections::HashMap;

/// Diagnostic code for unsatisfied trait bound.
pub const T_UNSATISFIED_BOUND: u16 = 514;

/// The result of resolving a trait bound at a call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundResolution {
    /// Bound satisfied via generic-parameter declaration.
    BoundedParam {
        /// Name of the generic parameter.
        param_name: String,
        /// Name of the trait it is bound to.
        trait_name: String,
    },
    /// Bound satisfied via a concrete impl in the coherence table.
    ConcreteImpl {
        /// Name of the type.
        type_name: String,
        /// Name of the trait being implemented.
        trait_name: String,
    },
    /// Unsatisfied — emit T0514.
    Missing {
        /// Name of the type for which no impl exists.
        type_name: String,
        /// Name of the required trait.
        trait_name: String,
    },
}

/// A cache of trait impls for coherence-checked types.
///
/// Maps (impl_type_name, trait_name) → true if a concrete impl exists.
#[derive(Default, Debug, Clone)]
pub struct BoundCache {
    /// Concrete impls: (type_name, trait_name) → present.
    impls: HashMap<(String, String), bool>,
}

impl BoundCache {
    /// Construct an empty bound cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a concrete impl.
    pub fn register_impl(&mut self, type_name: String, trait_name: String) {
        self.impls.insert((type_name, trait_name), true);
    }

    /// Check if a concrete impl exists for (type_name, trait_name).
    #[must_use]
    pub fn has_impl(&self, type_name: &str, trait_name: &str) -> bool {
        self.impls
            .get(&(type_name.to_string(), trait_name.to_string()))
            .copied()
            .unwrap_or(false)
    }
}

/// Resolve a trait bound at a call site.
///
/// # Arguments
///
/// * `bound_cache` - The cache of known concrete impls.
/// * `bounded_params` - List of (param_name, [trait_names]) for generic parameters.
/// * `target_type` - The type (or type variable) being used at this call site.
/// * `required_trait` - The trait that must be implemented.
///
/// # Returns
///
/// A `BoundResolution` indicating whether the bound is satisfied, and if so, how.
#[must_use]
pub fn resolve_bound(
    bound_cache: &BoundCache,
    bounded_params: &[(String, Vec<String>)],
    target_type: &str,
    required_trait: &str,
) -> BoundResolution {
    // Check if the target_type is a bounded parameter.
    for (param_name, trait_list) in bounded_params {
        if target_type == param_name && trait_list.contains(&required_trait.to_string()) {
            return BoundResolution::BoundedParam {
                param_name: param_name.clone(),
                trait_name: required_trait.to_string(),
            };
        }
    }

    // Check if there's a concrete impl for this (target_type, required_trait) pair.
    if bound_cache.has_impl(target_type, required_trait) {
        return BoundResolution::ConcreteImpl {
            type_name: target_type.to_string(),
            trait_name: required_trait.to_string(),
        };
    }

    // No impl found.
    BoundResolution::Missing {
        type_name: target_type.to_string(),
        trait_name: required_trait.to_string(),
    }
}

/// Emit a T0514 diagnostic for an unsatisfied bound.
#[must_use]
pub fn unsatisfied_bound_diagnostic(type_name: &str, trait_name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(t_code(T_UNSATISFIED_BOUND))
        .message(format!(
            "no impl found for trait `{0}` on type `{1}` at this call site",
            trait_name, type_name
        ))
        .with_span(span)
        .finish()
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
    fn resolve_bound_via_bounded_param_succeeds() {
        let cache = BoundCache::new();
        let bounded_params = vec![("T".to_string(), vec!["Display".to_string()])];

        let resolution = resolve_bound(&cache, &bounded_params, "T", "Display");

        assert_eq!(
            resolution,
            BoundResolution::BoundedParam {
                param_name: "T".to_string(),
                trait_name: "Display".to_string(),
            }
        );
    }

    #[test]
    fn resolve_bound_via_concrete_impl_succeeds() {
        let mut cache = BoundCache::new();
        cache.register_impl("u64".to_string(), "Display".to_string());
        let bounded_params = vec![];

        let resolution = resolve_bound(&cache, &bounded_params, "u64", "Display");

        assert_eq!(
            resolution,
            BoundResolution::ConcreteImpl {
                type_name: "u64".to_string(),
                trait_name: "Display".to_string(),
            }
        );
    }

    #[test]
    fn resolve_bound_missing_returns_missing() {
        let cache = BoundCache::new();
        let bounded_params = vec![];

        let resolution = resolve_bound(&cache, &bounded_params, "u32", "UnknownTrait");

        assert_eq!(
            resolution,
            BoundResolution::Missing {
                type_name: "u32".to_string(),
                trait_name: "UnknownTrait".to_string(),
            }
        );
    }

    #[test]
    fn t0514_diagnostic_includes_type_and_trait_names() {
        let span = make_span(100, 20);
        let diag = unsatisfied_bound_diagnostic("MyType", "MyTrait", span);

        let msg = diag.message();
        assert!(
            msg.contains("MyTrait"),
            "diagnostic should contain trait name"
        );
        assert!(
            msg.contains("MyType"),
            "diagnostic should contain type name"
        );
    }
}
