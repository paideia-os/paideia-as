//! Lambda closure validation.
//!
//! Validates that a lambda closure's captures are legal given its declared or
//! inferred linearity class. Emits S-range diagnostic codes for violations.
//! See `design/toolchain/custom-assembler.md` §3.2 (closure capture discipline).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_ir::LinClass;

use crate::capture::{CaptureKind, CapturedBinding};

/// Diagnostic code for capturing a linear or ordered binding by consume
/// in a closure that is not itself declared linear or affine.
///
/// Error: "closure captures a {binding_class} binding by consume but the
/// closure itself is not declared linear or affine — it could be called
/// multiple times"
pub const S_ILLEGAL_CAPTURE: u16 = 907;

/// Construct a DiagnosticCode in the S category.
fn s_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::S, Severity::Error, n).expect("valid S code")
}

/// Validate the captures of a closure against its declared (or inferred)
/// linearity class.
///
/// # Validation rules
///
/// - **Capturing a Linear/Ordered binding by Consume** requires the closure to
///   be Linear or Affine (so it cannot be called more than once). Otherwise,
///   emits S0907.
///
/// - **Capturing by Reference** is always allowed (no consumption).
///
/// - **Capturing by Value** is always allowed for Unrestricted and Affine
///   bindings (copy semantics are "free"). Capturing a Linear binding by Value
///   implicitly consumes it (copy = consume for Linear), so it is treated as a
///   Consume capture.
///
/// # Arguments
///
/// - `captures`: the list of captured bindings (from `analyze_captures`).
/// - `closure_class`: the inferred or declared linearity class of the closure.
/// - `closure_span`: the source span of the closure for diagnostic reporting.
///
/// # Returns
///
/// A vector of diagnostics. Empty on success; contains S0907 errors for illegal captures.
#[must_use]
pub fn check_lambda(
    captures: &[CapturedBinding],
    closure_class: LinClass,
    closure_span: Span,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let closure_is_single_use = matches!(closure_class, LinClass::Linear | LinClass::Affine);

    for c in captures {
        // Capture by Consume of a Linear or Ordered binding in an unrestricted
        // closure is illegal.
        #[allow(clippy::collapsible_if)]
        if let (CaptureKind::Consume, LinClass::Linear | LinClass::Ordered) =
            (c.kind, c.binding.class)
        {
            if !closure_is_single_use {
                diags.push(s_capture_diag(closure_span, c.binding.class));
            }
        }
    }

    diags
}

fn s_capture_diag(span: Span, captured_class: LinClass) -> Diagnostic {
    Diagnostic::error(s_code(S_ILLEGAL_CAPTURE))
        .message(format!(
            "closure captures a {captured_class:?} binding by consume but the closure itself \
             is not declared linear or affine — it could be called multiple times"
        ))
        .with_span(span)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linearity_ctx::Binding;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn binding(class: LinClass) -> Binding {
        Binding {
            class,
            uses: 0,
            bind_span: span(100),
        }
    }

    fn captured(kind: CaptureKind, binding_class: LinClass, symbol: u32) -> CapturedBinding {
        CapturedBinding {
            symbol,
            kind,
            binding: binding(binding_class),
        }
    }

    /// Test 1 (AC): unrestricted closure with value capture succeeds.
    #[test]
    fn unrestricted_closure_with_value_capture_ok() {
        let captures = vec![captured(CaptureKind::Value, LinClass::Unrestricted, 1)];
        let diags = check_lambda(&captures, LinClass::Unrestricted, span(50));
        assert!(diags.is_empty());
    }

    /// Test 2 (AC): unrestricted closure consumes linear binding → S0907.
    #[test]
    fn unrestricted_closure_consumes_linear_emits_s0907() {
        let captures = vec![captured(CaptureKind::Consume, LinClass::Linear, 1)];
        let diags = check_lambda(&captures, LinClass::Unrestricted, span(50));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_ILLEGAL_CAPTURE);
    }

    /// Test 3 (AC): linear closure consumes linear binding → OK.
    #[test]
    fn linear_closure_consumes_linear_ok() {
        let captures = vec![captured(CaptureKind::Consume, LinClass::Linear, 1)];
        let diags = check_lambda(&captures, LinClass::Linear, span(50));
        assert!(diags.is_empty());
    }

    /// Test 4: affine closure consumes linear binding → OK.
    #[test]
    fn affine_closure_consumes_linear_ok() {
        let captures = vec![captured(CaptureKind::Consume, LinClass::Linear, 1)];
        let diags = check_lambda(&captures, LinClass::Affine, span(50));
        assert!(diags.is_empty());
    }

    /// Test 5: unrestricted closure references linear binding → OK.
    #[test]
    fn unrestricted_closure_references_linear_ok() {
        let captures = vec![captured(CaptureKind::Reference, LinClass::Linear, 1)];
        let diags = check_lambda(&captures, LinClass::Unrestricted, span(50));
        assert!(diags.is_empty());
    }

    /// Test 6: consume of ordered treated like linear.
    #[test]
    fn consume_of_ordered_treated_like_linear() {
        let captures = vec![captured(CaptureKind::Consume, LinClass::Ordered, 1)];
        let diags = check_lambda(&captures, LinClass::Unrestricted, span(50));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_ILLEGAL_CAPTURE);
    }

    /// Test 7: consume of unrestricted binding in unrestricted closure → OK.
    #[test]
    fn consume_of_unrestricted_no_error() {
        let captures = vec![captured(CaptureKind::Consume, LinClass::Unrestricted, 1)];
        let diags = check_lambda(&captures, LinClass::Unrestricted, span(50));
        assert!(diags.is_empty());
    }
}
