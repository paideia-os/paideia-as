//! R220.M1 `Elab` effect surface — Rust-side helpers.
//!
//! `Elab` itself was registered as an effect **name** by
//! `paideia_as_effects::EffectRegistry::register_macro_effects` (a one-
//! op stub, `Elab.elab`) so macro bodies could carry the row
//! `!{Diag, Elab, FreshName}`. R220.M1 grows the operation surface to
//! the four Christiansen-Brady-shaped operations a hosted DSL needs:
//!
//! | Operation                | Signature (Rust-side sketch)             |
//! |--------------------------|------------------------------------------|
//! | `get_expected_type`      | `() -> Option<TypeId>`                   |
//! | `get_expected_effects`   | `() -> EffectRow`                        |
//! | `elab_error`             | `(msg: Str, span: Span) -> Never`        |
//! | `elab_warn`              | `(msg: Str, span: Span) -> ()`           |
//!
//! The elaborator's builtin-call path is what actually dispatches these
//! at DSL run time; this module gives the surface a stable Rust-side
//! spec + the F-code assignments (F1200..F1209) so downstream crates
//! reference symbolic names, not magic numbers.
//!
//! **Scope cap.** The elaborator wiring of these ops (the elab_builtin
//! dispatcher landing an actual `Diagnostic` for `elab_error` etc.) is
//! a subsequent softarch task tracked under R220.M3 (`@dsl_parser`).
//! This module lands the *signatures* + the *error codes* — every
//! downstream consumer already has a stable symbol to depend on.

use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};
use paideia_as_effects::{EffectRegistry, SignatureId};

/// Maximum quote / anti-quote nesting depth accepted by the R220.M1
/// parser guardrail (see `paideia_as_parser::quote`). Lifting the cap
/// is a follow-on softarch task once we have a use case; today three
/// levels of nesting covers every hosted-DSL fragment in the plan.
pub const ELAB_MAX_QUOTE_DEPTH: u32 = 3;

/// F-family code for the generic `elab_error` diagnostic.
pub const F_ELAB_ERROR: u16 = 1200;

/// F-family code for the parser's quote-depth guardrail (see
/// [`ELAB_MAX_QUOTE_DEPTH`]).
pub const F_ELAB_QUOTE_DEPTH_EXCEEDED: u16 = 1201;

/// F-family code emitted when a hosted DSL asks
/// `get_expected_type()`/`get_expected_effects()` outside an elaborator
/// context where those are known.
pub const F_ELAB_TYPE_UNAVAILABLE: u16 = 1202;

/// Construct the canonical `DiagnosticCode` for one of the reserved
/// R220.M1 F-family entries. Panics if `n` is not one of the reserved
/// codes (the reservation set is `F1200..=F1209`).
#[must_use]
pub fn elab_error_code(n: u16) -> DiagnosticCode {
    assert!(
        (1200..=1209).contains(&n),
        "elab_error_code: F{n} outside R220.M1 reservation (F1200..=F1209)"
    );
    DiagnosticCode::new(Category::F, Severity::Error, n).expect("F1200..=F1209 valid")
}

/// Discriminant naming one of the four R220.M1 `Elab` operations.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ElabOpKind {
    /// `get_expected_type() -> Option<TypeId>`.
    GetExpectedType,
    /// `get_expected_effects() -> EffectRow`.
    GetExpectedEffects,
    /// `elab_error(msg: Str, span: Span) -> Never`.
    ElabError,
    /// `elab_warn(msg: Str, span: Span) -> ()`.
    ElabWarn,
}

impl ElabOpKind {
    /// The wire-form dotted-path name (`Elab.get_expected_type` etc.).
    #[must_use]
    pub fn dotted_path(self) -> &'static str {
        match self {
            Self::GetExpectedType => "Elab.get_expected_type",
            Self::GetExpectedEffects => "Elab.get_expected_effects",
            Self::ElabError => "Elab.elab_error",
            Self::ElabWarn => "Elab.elab_warn",
        }
    }

    /// The bare operation name (the second half of the dotted path).
    #[must_use]
    pub fn op_name(self) -> &'static str {
        match self {
            Self::GetExpectedType => "get_expected_type",
            Self::GetExpectedEffects => "get_expected_effects",
            Self::ElabError => "elab_error",
            Self::ElabWarn => "elab_warn",
        }
    }

    /// All four kinds, in the order the plan documents them.
    #[must_use]
    pub fn all() -> [ElabOpKind; 4] {
        [
            Self::GetExpectedType,
            Self::GetExpectedEffects,
            Self::ElabError,
            Self::ElabWarn,
        ]
    }
}

/// The bare operation-name list for the R220.M1 `Elab` effect —
/// convenient for round-tripping into `EffectRegistry::declare_effect`.
#[must_use]
pub fn elab_operation_names() -> Vec<&'static str> {
    ElabOpKind::all().iter().map(|k| k.op_name()).collect()
}

/// A structured `elab_error` payload the elaborator's builtin-call path
/// will lift into a `Diagnostic` (planned wiring at R220.M3). Keeping
/// the type here means downstream crates can construct instances today.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElabError {
    /// Human-readable message. Passed through verbatim to the
    /// diagnostic renderer.
    pub message: String,
    /// Byte-range span the diagnostic should point at.
    pub span: paideia_as_diagnostics::Span,
}

/// A structured `elab_warn` payload (sibling of [`ElabError`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElabWarn {
    /// Human-readable message.
    pub message: String,
    /// Byte-range span the warning should point at.
    pub span: paideia_as_diagnostics::Span,
}

/// Container for the four R220.M1 operation-signature IDs — a stable
/// artifact hosted-DSL implementors and the elaborator's builtin-call
/// dispatcher both consult.
///
/// Signature IDs are opaque `SignatureId`s (u32s) minted by the type
/// interner in `paideia-as-types`; at R220.M1 the interner does not
/// have a stable public entrypoint the reflection crate can call, so
/// [`elab_op_signatures`] returns a container filled with the sentinel
/// `SignatureId` `0` (matching the sentinel `register_macro_effects`
/// uses for the `Elab.elab` stub). R220.M3 will replace the sentinel
/// with real interned signatures.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ElabOpSignatures {
    /// Signature id for `Elab.get_expected_type`.
    pub get_expected_type: SignatureId,
    /// Signature id for `Elab.get_expected_effects`.
    pub get_expected_effects: SignatureId,
    /// Signature id for `Elab.elab_error`.
    pub elab_error: SignatureId,
    /// Signature id for `Elab.elab_warn`.
    pub elab_warn: SignatureId,
}

impl ElabOpSignatures {
    /// Look up the `SignatureId` for a given [`ElabOpKind`].
    #[must_use]
    pub fn lookup(self, kind: ElabOpKind) -> SignatureId {
        match kind {
            ElabOpKind::GetExpectedType => self.get_expected_type,
            ElabOpKind::GetExpectedEffects => self.get_expected_effects,
            ElabOpKind::ElabError => self.elab_error,
            ElabOpKind::ElabWarn => self.elab_warn,
        }
    }
}

/// Build a placeholder [`ElabOpSignatures`] with every field set to the
/// sentinel signature ID `0`.
///
/// R220.M3 will replace this with a real interner call once
/// `paideia-as-types` grows a stable public entrypoint for R220-shaped
/// signatures. Consumers should treat the sentinel as "unresolved."
#[must_use]
pub fn elab_op_signatures() -> ElabOpSignatures {
    ElabOpSignatures {
        get_expected_type: 0,
        get_expected_effects: 0,
        elab_error: 0,
        elab_warn: 0,
    }
}

/// Register the four R220.M1 `Elab` operations on the supplied registry.
///
/// Idempotent when the registry already carries the R220.M1 op-set:
/// re-declaring the same op-name set emits no diagnostic (see
/// `paideia_as_effects::EffectRegistry::declare_effect`).
///
/// The `register_macro_effects` predecessor left `Elab` at one stub op
/// (`Elab.elab`); calling this on top of that upgrades the op set to the
/// four R220.M1 ops and emits one `F1101` (effect-redeclared) — which
/// callers can suppress since the change is intentional. Callers that
/// prefer a clean landing should call this *instead of*
/// `register_macro_effects` for the `Elab` slot and register `Diag` /
/// `FreshName` separately.
///
/// Returns the assigned [`ElabOpSignatures`] (sentinel-filled today).
pub fn register_elab_ops(
    registry: &mut EffectRegistry,
    sentinel_span: paideia_as_diagnostics::Span,
) -> ElabOpSignatures {
    let ops: Vec<(String, SignatureId, paideia_as_diagnostics::Span)> = ElabOpKind::all()
        .into_iter()
        .map(|kind| (kind.op_name().to_owned(), 0u32, sentinel_span))
        .collect();
    let _ = registry.declare_effect("Elab", &ops, sentinel_span);
    elab_op_signatures()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn sentinel_span() -> Span {
        Span::new(FileId::new(u32::MAX).unwrap(), 0, 0)
    }

    #[test]
    fn all_op_kinds_have_unique_dotted_paths() {
        let mut seen = std::collections::HashSet::new();
        for kind in ElabOpKind::all() {
            assert!(seen.insert(kind.dotted_path()), "duplicate path");
        }
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn elab_error_code_within_reservation() {
        let code = elab_error_code(F_ELAB_ERROR);
        assert_eq!(code.category(), Category::F);
        assert_eq!(code.number(), 1200);
    }

    #[test]
    #[should_panic(expected = "outside R220.M1 reservation")]
    fn elab_error_code_rejects_out_of_range() {
        let _ = elab_error_code(1300); // outside 1200..=1209
    }

    #[test]
    fn register_elab_ops_populates_all_four_operations() {
        let mut reg = EffectRegistry::new();
        let _sigs = register_elab_ops(&mut reg, sentinel_span());
        for kind in ElabOpKind::all() {
            assert!(
                reg.lookup_op(kind.dotted_path()).is_some(),
                "op {} not registered",
                kind.dotted_path()
            );
        }
    }

    #[test]
    fn register_elab_ops_is_idempotent_on_same_op_set() {
        let mut reg = EffectRegistry::new();
        let _ = register_elab_ops(&mut reg, sentinel_span());
        // Second call with the same op-name set must not diverge.
        let _ = register_elab_ops(&mut reg, sentinel_span());
        for kind in ElabOpKind::all() {
            assert!(reg.lookup_op(kind.dotted_path()).is_some());
        }
    }

    #[test]
    fn elab_op_signatures_sentinel_is_zero() {
        let sigs = elab_op_signatures();
        for kind in ElabOpKind::all() {
            assert_eq!(sigs.lookup(kind), 0, "R220.M1 signatures are sentinel");
        }
    }

    #[test]
    fn elab_operation_names_covers_all_four() {
        let names = elab_operation_names();
        assert_eq!(names.len(), 4);
        assert!(names.contains(&"get_expected_type"));
        assert!(names.contains(&"get_expected_effects"));
        assert!(names.contains(&"elab_error"));
        assert!(names.contains(&"elab_warn"));
    }

    #[test]
    fn max_quote_depth_is_three() {
        assert_eq!(ELAB_MAX_QUOTE_DEPTH, 3);
    }
}
