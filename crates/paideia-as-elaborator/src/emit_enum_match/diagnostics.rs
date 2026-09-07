//! Typed `DiagnosticCode` factories used across the `emit_enum_match`
//! submodules. Each helper wraps `DiagnosticCode::new` with the fixed
//! category / severity / number for its diagnostic. Extracted verbatim
//! from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

/// Helper to construct T0556 diagnostic code.
pub(super) fn t0556_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 556)
        .expect("T0556 is within valid T range")
}

/// Helper to construct T0559 diagnostic code.
pub(super) fn t0559_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 559)
        .expect("T0559 is within valid T range")
}

/// Helper to construct T0560 diagnostic code.
pub(super) fn t0560_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 560)
        .expect("T0560 is within valid T range")
}

/// Helper to construct T0561 diagnostic code.
pub(super) fn t0561_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 561)
        .expect("T0561 is within valid T range")
}

/// Helper to construct T0562 diagnostic code.
pub(super) fn t0562_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 562)
        .expect("T0562 is within valid T range")
}

/// Helper to construct T0563 diagnostic code.
pub(super) fn t0563_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 563)
        .expect("T0563 is within valid T range")
}

/// Helper to construct T0566 diagnostic code.
pub(super) fn t0566_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 566)
        .expect("T0566 is within valid T range")
}

/// Helper to construct U1648 diagnostic code.
pub(super) fn u1648_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1648)
        .expect("U1648 is within valid U range")
}

/// Helper to construct U1649 diagnostic code.
pub(super) fn u1649_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1649)
        .expect("U1649 is within valid U range")
}

/// Helper to construct U1650 diagnostic code.
pub(super) fn u1650_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1650)
        .expect("U1650 is within valid U range")
}

/// Helper to construct U1651 diagnostic code.
pub(super) fn u1651_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1651)
        .expect("U1651 is within valid U range")
}

/// Helper to construct U1652 diagnostic code.
pub(super) fn u1652_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1652)
        .expect("U1652 is within valid U range")
}

/// Helper to construct U1653 diagnostic code.
pub(super) fn u1653_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1653)
        .expect("U1653 is within valid U range")
}

/// Helper to construct U1654 diagnostic code.
pub(super) fn u1654_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1654)
        .expect("U1654 is within valid U range")
}

/// Helper to construct U1655 diagnostic code.
pub(super) fn u1655_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1655)
        .expect("U1655 is within valid U range")
}

/// Helper to construct U1656 diagnostic code.
pub(super) fn u1656_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1656)
        .expect("U1656 is within valid U range")
}

/// Helper to construct U1657 diagnostic code.
pub(super) fn u1657_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1657)
        .expect("U1657 is within valid U range")
}

/// Helper to construct U1658 diagnostic code.
pub(super) fn u1658_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1658)
        .expect("U1658 is within valid U range")
}
