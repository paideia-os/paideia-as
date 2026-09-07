//! U-category diagnostic code constants for unsafe-block errors.
//! Split out of `unsafe_walker.rs` (paideia-as #1403).

use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

/// Diagnostic code for unknown mnemonic (U1605).
pub const U_UNKNOWN_MNEMONIC: u16 = 1605;

/// Diagnostic code for malformed operand (U1606).
pub const U_MALFORMED_OPERAND: u16 = 1606;

/// Diagnostic code for unexpected operands on zero-arity instruction (U1607).
pub const U_UNEXPECTED_OPERANDS: u16 = 1607;

/// Diagnostic code for unresolved field offset in unsafe block (U1608).
pub const U_UNRESOLVED_FIELD_OFFSET: u16 = 1608;

/// Diagnostic code for duplicate label declaration in unsafe block (U1609).
pub const U_DUPLICATE_LABEL: u16 = 1609;

/// Diagnostic code for unknown label reference in unsafe block (U1610).
pub const U_UNKNOWN_LABEL: u16 = 1610;

/// Diagnostic code for SymbolRef operand not supported for mnemonic (U1611).
pub const U_SYMBOLREF_NOT_SUPPORTED: u16 = 1611;

/// Diagnostic code for unsupported statement in unsafe block (U1614).
pub const U_UNSUPPORTED_STMT_IN_UNSAFE: u16 = 1614;

/// Helper: create a U-category error code.
pub(super) fn u_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, n).expect("valid U code")
}
