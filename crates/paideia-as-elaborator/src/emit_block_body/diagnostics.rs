//! Typed `DiagnosticCode` factories used across the `emit_block_body`
//! submodules. Each helper wraps `DiagnosticCode::new` with the fixed
//! category / severity / number for its diagnostic. Extracted verbatim
//! from the pre-split `emit_block_body.rs` (issue #1410).

use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

/// Helper to construct T0527 diagnostic code.
pub(super) fn t0527_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 527)
        .expect("T0527 is within valid T range")
}

/// Helper to construct U1621 diagnostic code (Branch shape invariant).
/// Shared code with emit_control_flow::visit_branch — slice A1 minted; A3 reuses.
pub(super) fn u1621_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1621)
        .expect("U1621 is within valid U range")
}

/// Helper to construct U1642 diagnostic code (RawInstruction payload invariant).
/// Slice A3 mint; reclassifies former T0526 emissions in emit_block_body.
pub(super) fn u1642_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1642)
        .expect("U1642 is within valid U range")
}

/// Helper to construct U1659 diagnostic code (Unhandled Let-RHS kind).
/// Part of #1209/#1207 hardening — detects gaps in Let-RHS dispatcher.
pub(super) fn u1659_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1659)
        .expect("U1659 is within valid U range")
}

/// Helper to construct T0559 diagnostic code (Pattern shape violation).
/// #1213: Used when EnumCons payload child is neither Literal nor Var.
pub(super) fn t0559_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 559)
        .expect("T0559 is within valid T range")
}
