//! Handler well-typedness check.
//!
//! A handler value of type `Handler<E>` must provide an implementation
//! function for every operation in effect `E`. Per
//! `custom-assembler.md` §4.4, the handler is a record `{ op1 = impl1,
//! op2 = impl2 }`. Each `impl_i`'s return type must match the
//! corresponding `op_i`'s declared return type.
//!
//! Phase-1 simplification: signatures are opaque `u32` ids (the same
//! [`SignatureId`] used by `EffectRegistry`). Matching is by
//! equality. Real subtype/return-type matching arrives once
//! `paideia-as-types` is wired through this pass.

use std::collections::{HashMap, HashSet};

use crate::effect_infer::handle_row;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_effects::{EffectId, EffectRow, SignatureId};

/// Diagnostic code for handler op-set mismatches.
///
/// Re-uses `F1101` ("effect-related declaration mismatch") since both
/// the effect-redeclaration check (PR-41) and the handler-completeness
/// check share the same root cause: a declared effect set has been
/// violated.
pub const F_HANDLER_MISMATCH: u16 = 1101;

/// One handler implementation entry.
#[derive(Clone, Debug)]
pub struct HandlerImpl {
    /// Operation name within the handled effect.
    pub op_name: String,
    /// Implementation signature (TypeId as a SignatureId u32).
    pub signature: SignatureId,
    /// Source span of the implementation.
    pub span: Span,
}

/// Validate a handler value against the effect it's handling.
///
/// `declared_ops` is the effect's full op set as `(op_name, signature)`
/// pairs — the caller (which holds an `EffectRegistry`) pulls them out
/// for this function. Phase-1: pass them in directly to keep this
/// module decoupled from the registry's internal layout.
///
/// Rules:
/// - Every op in `declared_ops` must be present in `impls` — otherwise
///   one F1101 per missing op.
/// - Every impl in `impls` whose op name is unknown to the effect is an
///   F1101 ("handler provides unknown op").
/// - Every impl whose signature doesn't equal the declared op signature
///   is an F1101 ("op `Foo.bar` impl returns wrong type").
///
/// Diagnostics are sorted by impl span byte_start for deterministic
/// output.
#[must_use]
pub fn check_handler(
    effect_name: &str,
    declared_ops: &[(String, SignatureId)],
    impls: &[HandlerImpl],
    handler_span: Span,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Build the set of declared op names + the lookup from name to
    // signature.
    let mut declared: HashMap<String, SignatureId> = HashMap::new();
    for (name, sig) in declared_ops {
        declared.insert(name.clone(), *sig);
    }

    // Implementations actually present, by name.
    let provided: HashSet<String> = impls.iter().map(|i| i.op_name.clone()).collect();
    let declared_set: HashSet<String> = declared.keys().cloned().collect();

    // 1) Missing implementations.
    let mut missing: Vec<&String> = declared_set.difference(&provided).collect();
    missing.sort();
    for op_name in missing {
        diags.push(
            Diagnostic::error(f_code(F_HANDLER_MISMATCH))
                .message(format!(
                    "handler for effect `{effect_name}` is missing implementation of op `{op_name}`"
                ))
                .with_span(handler_span)
                .finish(),
        );
    }

    // 2) Extra implementations (unknown op) + 3) signature mismatches.
    let mut by_span: Vec<&HandlerImpl> = impls.iter().collect();
    by_span.sort_by_key(|i| i.span.byte_start());
    for i in by_span {
        match declared.get(&i.op_name) {
            None => {
                diags.push(
                    Diagnostic::error(f_code(F_HANDLER_MISMATCH))
                        .message(format!(
                            "handler for effect `{effect_name}` provides implementation of \
                             unknown op `{}`",
                            i.op_name
                        ))
                        .with_span(i.span)
                        .finish(),
                );
            }
            Some(&decl_sig) if decl_sig != i.signature => {
                diags.push(
                    Diagnostic::error(f_code(F_HANDLER_MISMATCH))
                        .message(format!(
                            "impl of op `{effect_name}.{}` has signature {} but op declares {decl_sig}",
                            i.op_name, i.signature
                        ))
                        .with_span(i.span)
                        .finish(),
                );
            }
            _ => {}
        }
    }

    diags
}

/// Check that a `resume e` expression's value type matches the
/// handled operation's return type.
///
/// Phase-1 uses `SignatureId` equality as a stand-in for "matches the
/// operation's return". Returns one `F1101` on mismatch.
#[must_use]
pub fn check_resume(
    op_return_sig: SignatureId,
    resume_value_sig: SignatureId,
    span: Span,
) -> Vec<Diagnostic> {
    if op_return_sig != resume_value_sig {
        vec![
            Diagnostic::error(f_code(F_HANDLER_MISMATCH))
                .message(format!(
                    "resume value type {resume_value_sig} does not match operation's \
                     declared return type {op_return_sig}"
                ))
                .with_span(span)
                .finish(),
        ]
    } else {
        Vec::new()
    }
}

/// Full handler-installation check under row polymorphism.
///
/// Combines:
/// 1. `check_handler` — verifies the handler's impls match the effect's
///    declared ops (F1101).
/// 2. `handle_row` — subtracts the handled effect from the body row
///    (m1-006's primitive). If the body row was open `!{... | r}`,
///    the result remains open with the tail preserved — that's the
///    row-polymorphism story.
///
/// Returns `(post_handler_body_row, F1101_diagnostics)`.
///
/// # Example
/// ```ignore
/// // Handler for Net; body row is {Net | r}
/// let (post_row, diags) = check_handler_installation_polymorphic(
///     "Net",
///     &net_ops,
///     &impls,
///     handler_span,
///     &body_row,
///     net_effect_id,
/// );
/// // post_row is now { | r} (Net subtracted, tail preserved)
/// // diags contains any F1101 op-set mismatches
/// ```
#[must_use]
pub fn check_handler_installation_polymorphic(
    effect_name: &str,
    declared_ops: &[(String, SignatureId)],
    impls: &[HandlerImpl],
    handler_span: Span,
    body_row: &EffectRow,
    handled_effect: EffectId,
) -> (EffectRow, Vec<Diagnostic>) {
    let diags = check_handler(effect_name, declared_ops, impls, handler_span);
    let post_row = handle_row(body_row, handled_effect);
    (post_row, diags)
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

    fn impl_(name: &str, sig: SignatureId, byte_start: u32) -> HandlerImpl {
        HandlerImpl {
            op_name: name.to_string(),
            signature: sig,
            span: span(byte_start),
        }
    }

    fn io_ops() -> Vec<(String, SignatureId)> {
        vec![
            ("port_read".to_string(), 101),
            ("port_write".to_string(), 102),
        ]
    }

    // ── AC bullet 1: complete handler typechecks ─────────────────────

    #[test]
    fn complete_handler_passes() {
        let impls = vec![impl_("port_read", 101, 0), impl_("port_write", 102, 10)];
        let diags = check_handler("Io", &io_ops(), &impls, span(30));
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    // ── AC bullet 2: missing op emits F1101 ──────────────────────────

    #[test]
    fn missing_op_emits_f1101() {
        let impls = vec![impl_("port_read", 101, 0)];
        let diags = check_handler("Io", &io_ops(), &impls, span(30));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1101);
        assert_eq!(diags[0].code().category(), Category::F);
        assert!(diags[0].message().contains("port_write"));
    }

    // ── AC bullet 3: wrong return-type signature emits F1101 ─────────

    #[test]
    fn signature_mismatch_emits_f1101() {
        let impls = vec![
            impl_("port_read", 999, 0), // wrong sig
            impl_("port_write", 102, 10),
        ];
        let diags = check_handler("Io", &io_ops(), &impls, span(30));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1101);
        assert!(diags[0].message().contains("port_read"));
    }

    // ── AC bullet 4: resume return-type check ────────────────────────

    #[test]
    fn resume_matches_op_return_clean() {
        let diags = check_resume(101, 101, span(0));
        assert!(diags.is_empty());
    }

    #[test]
    fn resume_mismatch_emits_f1101() {
        let diags = check_resume(101, 102, span(0));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1101);
    }

    // ── Misc ─────────────────────────────────────────────────────────

    #[test]
    fn extra_impl_emits_f1101() {
        let impls = vec![
            impl_("port_read", 101, 0),
            impl_("port_write", 102, 10),
            impl_("not_a_real_op", 999, 20),
        ];
        let diags = check_handler("Io", &io_ops(), &impls, span(30));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("not_a_real_op"));
    }

    #[test]
    fn empty_handler_for_non_empty_effect_emits_one_per_op() {
        let diags = check_handler("Io", &io_ops(), &[], span(30));
        assert_eq!(diags.len(), 2);
        for d in &diags {
            assert_eq!(d.code().number(), 1101);
        }
    }

    // ── Row-polymorphic handler installation tests ──────────────────

    /// Helper: construct an EffectId from a positive integer.
    fn eff(n: u32) -> EffectId {
        EffectId::new(n).expect("valid effect id")
    }

    /// Helper: construct a RowVarId from a positive integer.
    fn row_var(n: u32) -> paideia_as_effects::RowVarId {
        paideia_as_effects::RowVarId::new(n).expect("valid row var id")
    }

    /// Test 1: handler for Net against row-polymorphic caller.
    /// Body row is {Net | r}; handler matches.
    /// - post_row should be { | r} (Net subtracted, tail preserved).
    /// - No F1101 diagnostics.
    #[test]
    fn installation_polymorphic_handler_for_net_against_polymorphic_caller() {
        let net_effect = eff(1);
        let declared_ops_net = vec![("send".to_string(), 101)];
        let impls = vec![impl_("send", 101, 0)];
        let handler_span = span(30);

        // Body row is {Net | r}
        let body_row = EffectRow::from_ids(vec![net_effect], Some(row_var(1)));

        let (post_row, diags) = check_handler_installation_polymorphic(
            "Net",
            &declared_ops_net,
            &impls,
            handler_span,
            &body_row,
            net_effect,
        );

        // Expect no F1101 diagnostics.
        assert!(diags.is_empty(), "got {:?}", diags);

        // Expect post_row to be { | r} (empty fixed, tail preserved).
        assert!(post_row.fixed.is_empty());
        assert_eq!(post_row.tail, Some(row_var(1)));
    }

    /// Test 2: handler for Net missing op, against row-polymorphic caller.
    /// Body row is {Net | r}; handler missing "send" op.
    /// - Expect F1101.
    /// - post_row is still computed: { | r} (don't fail-fast).
    #[test]
    fn installation_polymorphic_handler_missing_op_emits_f1101() {
        let net_effect = eff(1);
        let declared_ops_net = vec![("send".to_string(), 101)];
        let impls = vec![]; // missing implementation
        let handler_span = span(30);

        // Body row is {Net | r}
        let body_row = EffectRow::from_ids(vec![net_effect], Some(row_var(1)));

        let (post_row, diags) = check_handler_installation_polymorphic(
            "Net",
            &declared_ops_net,
            &impls,
            handler_span,
            &body_row,
            net_effect,
        );

        // Expect one F1101 for missing op.
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1101);
        assert!(diags[0].message().contains("send"));

        // post_row is still computed: { | r}
        assert!(post_row.fixed.is_empty());
        assert_eq!(post_row.tail, Some(row_var(1)));
    }

    /// Test 3: handler for Net against closed caller.
    /// Body row is {Net} (closed, no tail); handler matches.
    /// - post_row should be {} (closed empty).
    /// - No F1101 diagnostics.
    #[test]
    fn installation_polymorphic_handler_against_closed_caller() {
        let net_effect = eff(1);
        let declared_ops_net = vec![("send".to_string(), 101)];
        let impls = vec![impl_("send", 101, 0)];
        let handler_span = span(30);

        // Body row is {Net} (closed)
        let body_row = EffectRow::from_ids(vec![net_effect], None);

        let (post_row, diags) = check_handler_installation_polymorphic(
            "Net",
            &declared_ops_net,
            &impls,
            handler_span,
            &body_row,
            net_effect,
        );

        // Expect no F1101 diagnostics.
        assert!(diags.is_empty(), "got {:?}", diags);

        // Expect post_row to be {} (closed empty).
        assert!(post_row.fixed.is_empty());
        assert!(post_row.tail.is_none());
    }
}
