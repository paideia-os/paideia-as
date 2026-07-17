//! Address-of pre-emit helpers: variable-name extraction and addend range check.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Together these back the PA-R17-003 (#988) pass in `run` that resolves
//! `&fn_name` operands into the `AddrOfSideTable`.

use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap, VecSink,
};
use paideia_as_ir::IrNodeId;

use super::identifier::is_valid_identifier;

/// Policy governing which resolved SymbolKinds are accepted as address-of targets.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(super) enum AddrOfPolicy {
    /// Top-level `let x = &sym` — LHS declared as fn-ptr; reject non-Function with T0536.
    FunctionOnly,
    /// RecordCons field `S { f: &sym, … }` — LHS field type may be `*T` or fn-ptr; accept
    /// Function or Object. Undefined names still fall through to the cross-file path.
    FunctionOrObject,
}

/// PA-R17-003 / #988: Extract variable name from a Borrow operand.
///
/// Given a Borrow node's operand (expected to be a Var), extract the identifier text
/// from the source span, validate it, and check if it refers to a symbol matching the
/// provided policy (FunctionOnly or FunctionOrObject).
/// Returns `Some(var_name)` on success, or `None` if validation fails (with diagnostic
/// emitted to sink).
pub(super) fn extract_var_name_from_operand(
    operand_id: IrNodeId,
    lowering: &paideia_as_elaborator::LoweringResult,
    source_map: &SourceMap,
    file: paideia_as_diagnostics::FileId,
    policy: AddrOfPolicy,
    sink: &mut VecSink,
) -> Option<String> {
    let operand_node = lowering.ir.get(operand_id)?;

    // Reject if operand kind is not Var.
    if operand_node.kind != paideia_as_ir::IrKind::Var {
        let diag = Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 532).expect("T0532 is valid"),
        )
        .message("address-of operand is not an identifier")
        .with_span(operand_node.span)
        .finish();
        let _ = sink.emit(diag);
        return None;
    }

    // Extract source text of the Var.
    let span = operand_node.span;
    let start = span.byte_start() as usize;
    let len = span.byte_len() as usize;
    let content_ref = source_map.content(file);
    if start + len > content_ref.len() {
        // Malformed span, skip
        return None;
    }
    let var_name = content_ref[start..start + len].to_string();

    // Look up name in SymbolTable.
    let sym_kind = lowering.ir.symbols().lookup_by_name(&var_name).map(|s| s.kind);

    if let Some(sym_kind) = sym_kind {
        // Found locally. Check that symbol matches the policy.
        let ok = match policy {
            AddrOfPolicy::FunctionOnly => sym_kind == paideia_as_ir::SymbolKind::Function,
            AddrOfPolicy::FunctionOrObject => {
                matches!(sym_kind, paideia_as_ir::SymbolKind::Function | paideia_as_ir::SymbolKind::Object)
            }
        };
        if !ok {
            let diag = Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 536).expect("T0536 is valid"),
            )
            .message(match policy {
                AddrOfPolicy::FunctionOnly => {
                    "address-of target is not a function; data-symbol addr-of not supported in v0.17".to_string()
                }
                AddrOfPolicy::FunctionOrObject => {
                    format!("address-of target must be a function or object; got {:?}", sym_kind)
                }
            })
            .with_span(operand_node.span)
            .finish();
            let _ = sink.emit(diag);
            return None;
        }
        // On success, return the name
        Some(var_name)
    } else {
        // Name not found locally.
        // For well-formed identifiers, DO NOT reject here.
        // The writer will synthesize an undefined symbol.
        // Only reject if the name is malformed.
        if !is_valid_identifier(&var_name) {
            let diag = Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 534).expect("T0534 is valid"),
            )
            .message("unresolved identifier in address-of expression")
            .with_span(operand_node.span)
            .finish();
            let _ = sink.emit(diag);
            return None;
        }
        // Well-formed name not found locally → allow cross-file reference
        Some(var_name)
    }
}

/// PA-R17-014 / #992: Check that an address-of addend fits in i32 for rel32 relocations.
///
/// When a lea instruction uses RIP-relative addressing with an addend (e.g., `lea r64, [rip + sym + addend]`),
/// the addend must fit in a signed 32-bit integer. This function validates that constraint and emits
/// a T0536 diagnostic on overflow.
///
/// NOTE: Currently this helper is future-proofing. The elaborator's try_extract_symbol_sum
/// in unsafe_walker.rs performs early validation (lines 1271-1275), so addend overflow is
/// already caught at elaboration time. This helper will be wired in when field-offset-derived
/// addends are computed (issue #1043-1046 postprocessing).
///
/// Returns `Some(addend as i32)` on success, or `None` after emitting T0536 to the sink.
#[allow(dead_code)]
pub(super) fn check_addend_i32(
    addend: i64,
    span: paideia_as_diagnostics::Span,
    sink: &mut dyn paideia_as_diagnostics::DiagnosticSink,
) -> Option<i32> {
    if (i32::MIN as i64..=i32::MAX as i64).contains(&addend) {
        Some(addend as i32)
    } else {
        // Emit T0536 diagnostic for addend overflow
        let diag = Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 536).expect("T0536 is valid"),
        )
        .message(format!(
            "lea rel32 addend exceeds i32 range: {} (min: {}, max: {})",
            addend,
            i32::MIN as i64,
            i32::MAX as i64
        ))
        .with_span(span)
        .finish();
        let _ = sink.emit(diag);
        None
    }
}
