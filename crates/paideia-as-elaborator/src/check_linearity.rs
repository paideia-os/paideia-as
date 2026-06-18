//! Linearity validation and diagnostic emission.
//!
//! Validates that bindings in a closed scope satisfy the substructural
//! lattice constraints defined in `design/toolchain/custom-assembler.md` §3.1.
//! Emits S-range diagnostic codes for violations.

use std::collections::HashMap;

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_ir::LinClass;

use crate::env::Symbol;
use crate::linearity_ctx::Binding;

/// Diagnostic code for a Linear or Ordered binding that is never used (use-count = 0).
pub const S_NEVER_USED: u16 = 900;

/// Diagnostic code for a binding that violates its use-count constraint.
pub const S_OVERUSED: u16 = 901;

/// Construct a DiagnosticCode in the S category.
fn s_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::S, Severity::Error, n).expect("valid S code")
}

/// Inspect a closed scope and produce diagnostics per the substructural lattice.
///
/// The validation rules are:
///
/// - **Linear**: must be used exactly once.
///   - Use-count 0 → S0900 (never used).
///   - Use-count > 1 → S0901 (overused).
///
/// - **Ordered**: same constraint as Linear (used exactly once).
///   - Use-count 0 → S0900.
///   - Use-count > 1 → S0901.
///
/// - **Affine**: may be used at most once.
///   - Use-count 0 → OK.
///   - Use-count > 1 → S0901 (overused).
///
/// - **Unrestricted**: no constraints.
///   - Any use-count is valid.
///
/// The returned diagnostics are sorted by source span for determinism.
pub fn validate_scope(scope: &HashMap<Symbol, Binding>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for (_sym, b) in scope.iter() {
        match (b.class, b.uses) {
            (LinClass::Linear | LinClass::Ordered, 0) => {
                diags.push(
                    Diagnostic::error(s_code(S_NEVER_USED))
                        .message(format!(
                            "{:?} binding is never used; substructural lattice requires exactly one use",
                            b.class
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            (LinClass::Linear | LinClass::Ordered, n) if n > 1 => {
                diags.push(
                    Diagnostic::error(s_code(S_OVERUSED))
                        .message(format!(
                            "{:?} binding used {n} times; substructural lattice permits exactly one use",
                            b.class
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            (LinClass::Affine, n) if n > 1 => {
                diags.push(
                    Diagnostic::error(s_code(S_OVERUSED))
                        .message(format!(
                            "affine binding used {n} times; affine permits at most one use"
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            _ => {}
        }
    }

    diags.sort_by_key(|d| d.primary_span().map(|s| s.byte_start()).unwrap_or(0));
    diags
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn binding(class: LinClass, uses: u32, bind_span: Span) -> Binding {
        Binding {
            class,
            uses,
            bind_span,
        }
    }

    #[test]
    fn linear_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn linear_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn linear_unused_emits_s0900() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 0, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn affine_unused_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 0, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn affine_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn affine_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn ordered_unused_emits_s0900() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 0, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn ordered_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn ordered_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn unrestricted_used_arbitrarily_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Unrestricted, 5, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn unrestricted_unused_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Unrestricted, 0, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn multiple_bindings_in_scope() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 1, span(100)));
        scope.insert(20, binding(LinClass::Linear, 0, span(110))); // unused

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn multiple_violations_sorted_by_span() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 0, span(100))); // S0900 at span 100
        scope.insert(20, binding(LinClass::Linear, 2, span(50))); // S0901 at span 50

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 2);
        // sorted by span: 50 < 100
        assert_eq!(diags[0].primary_span().map(|s| s.byte_start()), Some(50));
        assert_eq!(diags[1].primary_span().map(|s| s.byte_start()), Some(100));
    }

    use paideia_as_diagnostics::Span;
}
