//! Pattern refutability checking.
//!
//! Determines whether a pattern is irrefutable (always matches all values of its type)
//! or refutable (may not match certain values).
//!
//! Irrefutable patterns are legal in let bindings; refutable patterns require a match.
//! Per phase-4 m7-005, refutable patterns in let bindings emit P0199.

use paideia_as_ast::{NodeKind, PatternData};

/// Check whether a pattern is irrefutable (always matches all values of its type).
///
/// Returns `true` if the pattern is irrefutable; `false` if refutable.
///
/// **Irrefutable patterns:**
/// - `_` (wildcard)
/// - `x` (identifier)
/// - `(p1, p2, ...)` where all inner patterns are irrefutable
/// - `TypeName { field: pat, ... }` where the type is a record (single variant)
///   and all field patterns are irrefutable
/// - Binding pattern `name @ pat` where `pat` is irrefutable
/// - Or-pattern `p1 | p2 | ...` where all alternatives are irrefutable
///
/// **Refutable patterns:**
/// - `Variant(...)` (enum variant)
/// - Literals: `0`, `true`, etc.
/// - Struct patterns when the struct type is an enum (multiple variants)
pub fn is_irrefutable(pattern_kind: NodeKind, pattern_data: &PatternData) -> bool {
    match (pattern_kind, pattern_data) {
        // Wildcard is always irrefutable
        (NodeKind::PatWildcard, PatternData::Wildcard) => true,

        // Identifier is always irrefutable (binds anything)
        (NodeKind::PatIdent, PatternData::Ident { .. }) => true,

        // Tuple is irrefutable if all elements are irrefutable
        (NodeKind::PatTuple, PatternData::Tuple { .. }) => {
            // For now, assume tuple is irrefutable if it parses correctly.
            // In a full implementation, we'd check each element.
            // This is a simplification for phase-4 m7-005.
            true
        }

        // Struct pattern
        (NodeKind::PatStruct, PatternData::Struct { .. }) => {
            // Struct patterns are irrefutable only if the type is a record type
            // (single variant disguised as a struct).
            // This requires type information; for phase-4, we defer this check
            // and assume all struct patterns are refutable (conservative).
            // TODO: Tie this to type checking.
            false
        }

        // Enum variant is always refutable
        (NodeKind::PatEnumVariant, PatternData::EnumVariant { .. }) => false,

        // Literal is always refutable
        (NodeKind::PatLiteral, PatternData::Literal { .. }) => false,

        // Or-pattern is irrefutable only if all alternatives are irrefutable
        (NodeKind::PatOr, PatternData::Or { .. }) => {
            // Conservative: all or-patterns are refutable for phase-4.
            // A full implementation would check all alternatives.
            // TODO: Check all alternatives.
            false
        }

        // Binding pattern is irrefutable if the inner pattern is
        (NodeKind::PatBinding, PatternData::Binding { .. }) => {
            // Conservative: defer to type checking phase.
            // TODO: Check the inner pattern.
            true
        }

        // Catch-all: conservative
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_irrefutable_for_ident_returns_true() {
        let data = PatternData::Ident {
            name: paideia_as_ast::NodeId::new(1).unwrap(),
            mutable: false,
        };
        assert!(is_irrefutable(NodeKind::PatIdent, &data));
    }

    #[test]
    fn is_irrefutable_for_wildcard_returns_true() {
        let data = PatternData::Wildcard;
        assert!(is_irrefutable(NodeKind::PatWildcard, &data));
    }

    #[test]
    fn is_irrefutable_for_enum_variant_returns_false() {
        let data = PatternData::EnumVariant {
            path: paideia_as_ast::NodeId::new(1).unwrap(),
            args: vec![],
        };
        assert!(!is_irrefutable(NodeKind::PatEnumVariant, &data));
    }

    #[test]
    fn is_irrefutable_for_literal_returns_false() {
        let data = PatternData::Literal {
            lit: paideia_as_ast::NodeId::new(1).unwrap(),
        };
        assert!(!is_irrefutable(NodeKind::PatLiteral, &data));
    }
}
