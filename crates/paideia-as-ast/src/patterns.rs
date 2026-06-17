//! Pattern-specific structured data (§8 Pattern grammar).
//!
//! [`PatternData`] is an enum carrying the semantic payload for pattern nodes.
//! Patterns are used in match arms, let bindings, and function parameters.
//! Categories: Wildcard, Ident, Literal, Tuple, Struct, EnumVariant, Or, Binding.

use crate::NodeId;

/// Structured payload for pattern nodes.
///
/// Each variant corresponds to a pattern kind as specified in §8 of the
/// syntax reference. Child `NodeId` fields point to other nodes in the arena.
#[derive(Clone, Debug)]
pub enum PatternData {
    /// `_` (wildcard).
    ///
    /// Matches any value and discards it.
    Wildcard,

    /// Named pattern (identifier).
    ///
    /// Binds a value to a name.
    Ident {
        /// Binding name (Ident node).
        name: NodeId,
        /// `true` if declared mutable (`mut name`).
        mutable: bool,
    },

    /// Literal pattern.
    ///
    /// Matches a specific literal value.
    Literal {
        /// Literal node.
        lit: NodeId,
    },

    /// Tuple pattern `(p1, p2, ...)`.
    ///
    /// Destructures a tuple.
    Tuple {
        /// Element patterns.
        elements: Vec<NodeId>,
    },

    /// Struct pattern.
    ///
    /// Destructures a struct by name and fields.
    Struct {
        /// Struct path (Path node).
        path: NodeId,
        /// Struct field patterns.
        fields: Vec<PatField>,
    },

    /// Enum variant pattern.
    ///
    /// Matches an enum variant with optional arguments.
    EnumVariant {
        /// Variant path (Path node).
        path: NodeId,
        /// Variant arguments (patterns).
        args: Vec<NodeId>,
    },

    /// Or-pattern: `p1 | p2 | ...`.
    ///
    /// Matches any of the alternative patterns.
    Or {
        /// Alternative patterns.
        alternatives: Vec<NodeId>,
    },

    /// Binding pattern: `name @ pat`.
    ///
    /// Binds a name to a pattern.
    Binding {
        /// Binding name (Ident node).
        name: NodeId,
        /// Inner pattern.
        inner: NodeId,
    },
}

/// A field in a struct pattern.
#[derive(Copy, Clone, Debug)]
pub struct PatField {
    /// Field name (Ident node).
    pub name: NodeId,
    /// Field pattern.
    pub pattern: NodeId,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nodeid(n: u32) -> NodeId {
        NodeId::new(n).unwrap()
    }

    #[test]
    fn pattern_wildcard_constructs() {
        let pat = PatternData::Wildcard;
        match pat {
            PatternData::Wildcard => {}
            _ => panic!("expected Wildcard variant"),
        }
    }

    #[test]
    fn pattern_ident_constructs() {
        let name = make_nodeid(1);
        let pat = PatternData::Ident {
            name,
            mutable: true,
        };
        match pat {
            PatternData::Ident {
                name: n,
                mutable: m,
            } => {
                assert_eq!(n, name);
                assert!(m);
            }
            _ => panic!("expected Ident variant"),
        }
    }

    #[test]
    fn pattern_literal_constructs() {
        let lit = make_nodeid(1);
        let pat = PatternData::Literal { lit };
        match pat {
            PatternData::Literal { lit: l } => {
                assert_eq!(l, lit);
            }
            _ => panic!("expected Literal variant"),
        }
    }

    #[test]
    fn pattern_tuple_constructs() {
        let elem1 = make_nodeid(1);
        let elem2 = make_nodeid(2);
        let pat = PatternData::Tuple {
            elements: vec![elem1, elem2],
        };
        match pat {
            PatternData::Tuple { elements: e } => {
                assert_eq!(e.len(), 2);
            }
            _ => panic!("expected Tuple variant"),
        }
    }

    #[test]
    fn pattern_struct_constructs() {
        let path = make_nodeid(1);
        let field_name = make_nodeid(2);
        let field_pat = make_nodeid(3);
        let field = PatField {
            name: field_name,
            pattern: field_pat,
        };
        let pat = PatternData::Struct {
            path,
            fields: vec![field],
        };
        match pat {
            PatternData::Struct { path: p, fields: f } => {
                assert_eq!(p, path);
                assert_eq!(f.len(), 1);
                assert_eq!(f[0].name, field_name);
            }
            _ => panic!("expected Struct variant"),
        }
    }

    #[test]
    fn pattern_enum_variant_constructs() {
        let path = make_nodeid(1);
        let arg1 = make_nodeid(2);
        let arg2 = make_nodeid(3);
        let pat = PatternData::EnumVariant {
            path,
            args: vec![arg1, arg2],
        };
        match pat {
            PatternData::EnumVariant { path: p, args: a } => {
                assert_eq!(p, path);
                assert_eq!(a.len(), 2);
            }
            _ => panic!("expected EnumVariant variant"),
        }
    }

    #[test]
    fn pattern_or_constructs() {
        let alt1 = make_nodeid(1);
        let alt2 = make_nodeid(2);
        let pat = PatternData::Or {
            alternatives: vec![alt1, alt2],
        };
        match pat {
            PatternData::Or { alternatives: a } => {
                assert_eq!(a.len(), 2);
            }
            _ => panic!("expected Or variant"),
        }
    }

    #[test]
    fn pattern_binding_constructs() {
        let name = make_nodeid(1);
        let inner = make_nodeid(2);
        let pat = PatternData::Binding { name, inner };
        match pat {
            PatternData::Binding { name: n, inner: i } => {
                assert_eq!(n, name);
                assert_eq!(i, inner);
            }
            _ => panic!("expected Binding variant"),
        }
    }

    #[test]
    fn pat_field_constructs() {
        let name = make_nodeid(1);
        let pat = make_nodeid(2);
        let field = PatField { name, pattern: pat };
        assert_eq!(field.name, name);
        assert_eq!(field.pattern, pat);
    }
}
