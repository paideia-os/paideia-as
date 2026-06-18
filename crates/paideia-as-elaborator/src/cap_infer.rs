//! Capability-set inference + over-declaration tolerance.
//!
//! The elaborator threads a `CapSet` alongside the effect row through
//! IR traversal. Each `perform` and `unsafe` block contributes its
//! declared capability requirements; the function's declared capability
//! set must be a superset of the inferred required set. Per
//! `custom-assembler.md` §5, **over-declaration is permissible** — a
//! function may list more capabilities than its body actually uses.
//!
//! Under-declaration (function declares `@{R}` but body needs `R` and
//! `W`) yields `C1300` ("required capability not held"), one per
//! missing capability.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_types::{CapId, CapSet};

/// Diagnostic code for missing capability.
pub const C_MISSING_CAP: u16 = 1300;

/// Check that `declared` contains every capability in `required`.
///
/// Emits one C1300 per missing capability, naming the cap id. Returns
/// the empty Vec when `declared ⊇ required` (over-declaration is fine).
#[must_use]
pub fn check_capabilities(declared: &CapSet, required: &CapSet, use_span: Span) -> Vec<Diagnostic> {
    let missing: Vec<CapId> = required.missing_caps(declared);
    let mut diags = Vec::new();
    for cap in &missing {
        diags.push(
            Diagnostic::error(c_code(C_MISSING_CAP))
                .message(format!(
                    "required capability {} not held by the enclosing function's \
                     declared capability set",
                    cap
                ))
                .with_span(use_span)
                .finish(),
        );
    }
    diags
}

/// Compose two cap sets — the inferred required set is the union of
/// every sub-expression's required set.
#[must_use]
pub fn compose_caps(a: &CapSet, b: &CapSet) -> CapSet {
    a.union(b)
}

fn c_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::C, Severity::Error, n).expect("valid C code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn cid(n: u32) -> CapId {
        CapId::new(n).unwrap()
    }

    fn caps(ns: &[u32]) -> CapSet {
        CapSet::from_ids(ns.iter().map(|n| cid(*n)).collect())
    }

    // ── AC bullets ────────────────────────────────────────────────────

    #[test]
    fn body_uses_subset_of_declared_passes() {
        // declared @{R, W}, body uses @{R} — OK.
        let diags = check_capabilities(&caps(&[1, 2]), &caps(&[1]), span());
        assert!(diags.is_empty());
    }

    #[test]
    fn body_uses_unauthorized_cap_emits_c1300() {
        // declared @{R}, body uses @{R, W} — W is missing.
        let diags = check_capabilities(&caps(&[1]), &caps(&[1, 2]), span());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1300);
    }

    #[test]
    fn body_uses_multiple_unauthorized_caps_emits_one_each() {
        let diags = check_capabilities(&caps(&[1]), &caps(&[1, 2, 3]), span());
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn over_declaration_is_permissible() {
        // declared @{R, W, X}, body uses just @{R} — fine.
        let diags = check_capabilities(&caps(&[1, 2, 3]), &caps(&[1]), span());
        assert!(diags.is_empty());
    }

    #[test]
    fn empty_required_passes_regardless() {
        let diags = check_capabilities(&caps(&[]), &caps(&[]), span());
        assert!(diags.is_empty());

        let diags = check_capabilities(&caps(&[1, 2]), &caps(&[]), span());
        assert!(diags.is_empty());
    }

    #[test]
    fn empty_declared_with_required_emits_one_each() {
        let diags = check_capabilities(&caps(&[]), &caps(&[1, 2]), span());
        assert_eq!(diags.len(), 2);
    }

    // ── compose_caps ─────────────────────────────────────────────────

    #[test]
    fn compose_caps_unions() {
        let a = caps(&[1, 2]);
        let b = caps(&[2, 3]);
        let c = compose_caps(&a, &b);
        assert_eq!(c.as_slice(), &[cid(1), cid(2), cid(3)]);
    }
}
