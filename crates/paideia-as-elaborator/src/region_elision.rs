//! Region (lifetime) elision rules.
//!
//! Rule 1 (input elision): if a function has exactly one input borrow
//! whose lifetime isn't explicitly stated, assign it a fresh region.
//!
//! Rule 2 (output elision): if there is exactly one input lifetime
//! (whether explicit or elided) AND an output borrow with no explicit
//! lifetime, the output gets the same lifetime as the input.
//!
//! Rule 3 (method elision): if a function has &self or &mut self,
//! all output borrows take the receiver's lifetime.
//!
//! These mirror Rust's rules. Phase-4-m5-005 minimum: encode the
//! decision; activate when the elaborator's region-inference walker
//! runs over function signatures.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_types::RegionId;

/// Diagnostic code for ambiguous lifetime elision.
///
/// Fires when elision rules can't pick a unique lifetime for an elided output borrow.
/// Category: lint (L).
pub const L_AMBIGUOUS_LIFETIME_ELISION: u16 = 2001;

fn l_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::L, Severity::Warning, n).expect("valid L code")
}

/// Result of applying elision rules to a function signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElisionResult {
    /// All output borrows got fixed lifetimes.
    Elided {
        /// The resolved RegionIds for output borrows in order.
        resolved_outputs: Vec<RegionId>,
    },
    /// Ambiguous — multiple inputs with no obvious receiver. Fire diagnostic.
    Ambiguous {
        /// Human-readable explanation of the ambiguity.
        reason: String,
        /// Span for the diagnostic.
        span: Span,
    },
    /// No elision needed (no output borrows or all explicit).
    NotNeeded,
}

/// Apply lifetime elision rules to a function signature.
///
/// # Arguments
///
/// * `input_lifetimes` — `None` = needs elision; `Some(rid)` = explicit or already-assigned region.
/// * `output_lifetimes` — `None` = needs elision; `Some(rid)` = explicit region.
/// * `has_self_receiver` — whether the function has `&self` or `&mut self`.
/// * `self_receiver_lifetime` — the lifetime of `&self` or `&mut self` if present.
/// * `fn_span` — span for diagnostics (used if ambiguous).
///
/// # Rules
///
/// **Rule 1 (input elision):** If exactly one input borrow lacks an explicit lifetime,
/// assign it a fresh region.
///
/// **Rule 2 (output elision):** If exactly one input lifetime exists and an output
/// borrow has no explicit lifetime, the output takes the input's lifetime.
///
/// **Rule 3 (method elision):** If `&self` or `&mut self` is present, all output
/// borrows take the receiver's lifetime.
#[must_use]
pub fn elide_lifetimes(
    input_lifetimes: &[Option<RegionId>],
    output_lifetimes: &[Option<RegionId>],
    has_self_receiver: bool,
    self_receiver_lifetime: Option<RegionId>,
    fn_span: Span,
) -> ElisionResult {
    // Filter to find outputs needing elision.
    let outputs_needing_elision: Vec<usize> = output_lifetimes
        .iter()
        .enumerate()
        .filter_map(|(idx, lt)| if lt.is_none() { Some(idx) } else { None })
        .collect();

    // If no outputs need elision, we're done.
    if outputs_needing_elision.is_empty() {
        return ElisionResult::NotNeeded;
    }

    // Rule 3: method elision (receiver lifetime applies to all outputs).
    if has_self_receiver {
        if let Some(receiver_lifetime) = self_receiver_lifetime {
            let mut resolved = output_lifetimes.to_vec();
            for &idx in &outputs_needing_elision {
                resolved[idx] = Some(receiver_lifetime);
            }
            return ElisionResult::Elided {
                resolved_outputs: resolved.into_iter().flatten().collect(),
            };
        }
    }

    // For non-method functions, count input lifetimes (explicit or already assigned).
    let input_lifetimes_for_rule2: Vec<RegionId> =
        input_lifetimes.iter().filter_map(|&lt| lt).collect();

    // Rule 2: output elision (single input lifetime).
    if input_lifetimes_for_rule2.len() == 1 {
        let input_lifetime = input_lifetimes_for_rule2[0];
        let mut resolved = output_lifetimes.to_vec();
        for &idx in &outputs_needing_elision {
            resolved[idx] = Some(input_lifetime);
        }
        return ElisionResult::Elided {
            resolved_outputs: resolved.into_iter().flatten().collect(),
        };
    }

    // Ambiguous case: multiple input lifetimes (or zero) + elided outputs.
    let reason = if input_lifetimes_for_rule2.is_empty() {
        "no input borrows; cannot infer output lifetime".to_string()
    } else {
        format!(
            "multiple input lifetimes ({} inputs); cannot determine which to apply to output",
            input_lifetimes_for_rule2.len()
        )
    };

    ElisionResult::Ambiguous {
        reason,
        span: fn_span,
    }
}

/// Generate diagnostic for ambiguous elision.
///
/// Returns a diagnostic with code L2001 (ambiguous lifetime elision).
pub fn ambiguous_elision_diagnostic(result: &ElisionResult) -> Option<Diagnostic> {
    match result {
        ElisionResult::Ambiguous { reason, span } => Some(
            Diagnostic::warning(l_code(L_AMBIGUOUS_LIFETIME_ELISION))
                .message(format!("ambiguous lifetime elision: {}", reason))
                .with_span(*span)
                .finish(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn elision_rule1_input_single_borrow_succeeds() {
        // Function with a single input borrow needing elision, no outputs.
        // Rule 1: assign fresh region to the input.
        let input_lifetimes = vec![None]; // One input, needs elision
        let output_lifetimes = vec![];
        let result = elide_lifetimes(&input_lifetimes, &output_lifetimes, false, None, span());
        assert_eq!(result, ElisionResult::NotNeeded);
    }

    #[test]
    fn elision_rule2_output_inherits_single_input() {
        // Function with one explicit input lifetime and one elided output.
        // Rule 2: output inherits input lifetime.
        let input_rid = RegionId(1);
        let input_lifetimes = vec![Some(input_rid)];
        let output_lifetimes = vec![None]; // Output needs elision
        let result = elide_lifetimes(&input_lifetimes, &output_lifetimes, false, None, span());

        match result {
            ElisionResult::Elided { resolved_outputs } => {
                assert_eq!(resolved_outputs, vec![input_rid]);
            }
            _ => panic!("expected Elided, got {:?}", result),
        }
    }

    #[test]
    fn elision_rule3_method_receiver_assigns_outputs() {
        // Method with &self and elided output.
        // Rule 3: output takes receiver lifetime.
        let receiver_rid = RegionId(1);
        let input_lifetimes = vec![Some(receiver_rid)]; // &self
        let output_lifetimes = vec![None]; // Output needs elision
        let result = elide_lifetimes(
            &input_lifetimes,
            &output_lifetimes,
            true,
            Some(receiver_rid),
            span(),
        );

        match result {
            ElisionResult::Elided { resolved_outputs } => {
                assert_eq!(resolved_outputs, vec![receiver_rid]);
            }
            _ => panic!("expected Elided, got {:?}", result),
        }
    }

    #[test]
    fn elision_fires_l2001_on_ambiguous_two_inputs_no_receiver() {
        // Function with two input lifetimes and elided output.
        // Ambiguous: cannot determine which input lifetime to assign.
        let rid1 = RegionId(1);
        let rid2 = RegionId(2);
        let input_lifetimes = vec![Some(rid1), Some(rid2)];
        let output_lifetimes = vec![None];
        let result = elide_lifetimes(&input_lifetimes, &output_lifetimes, false, None, span());

        match result {
            ElisionResult::Ambiguous { .. } => {
                let diag = ambiguous_elision_diagnostic(&result);
                assert!(diag.is_some());
                let d = diag.unwrap();
                assert_eq!(d.code().category(), Category::L);
                assert_eq!(d.code().number(), L_AMBIGUOUS_LIFETIME_ELISION);
            }
            _ => panic!("expected Ambiguous, got {:?}", result),
        }
    }

    #[test]
    fn elision_not_needed_when_all_explicit() {
        // Function with all lifetimes explicit.
        let rid1 = RegionId(1);
        let rid2 = RegionId(2);
        let input_lifetimes = vec![Some(rid1)];
        let output_lifetimes = vec![Some(rid2)];
        let result = elide_lifetimes(&input_lifetimes, &output_lifetimes, false, None, span());
        assert_eq!(result, ElisionResult::NotNeeded);
    }

    #[test]
    fn elision_returns_not_needed_when_no_outputs() {
        // Function with no output borrows.
        let input_lifetimes = vec![None];
        let output_lifetimes = vec![];
        let result = elide_lifetimes(&input_lifetimes, &output_lifetimes, false, None, span());
        assert_eq!(result, ElisionResult::NotNeeded);
    }
}
