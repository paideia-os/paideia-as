//! Type-specific structured data (§8 Type grammar).
//!
//! [`TypeData`] is an enum carrying the semantic payload for type nodes.
//! Type categories: TypeName, Arrow, Tuple, LinearClass, EffectRowType.

use crate::NodeId;

/// Structured payload for type nodes.
///
/// Each variant corresponds to a type kind as specified in §8 of the
/// syntax reference. Child `NodeId` fields point to other nodes in the arena.
#[derive(Clone, Debug)]
pub enum TypeData {
    /// `TypeName` or `TypeName(args)`.
    ///
    /// A named type, optionally with type arguments.
    Name {
        /// Type name (Ident node).
        name: NodeId,
        /// Type arguments.
        args: Vec<NodeId>,
    },

    /// `(T1, T2, ...) -> T !{...} @{...}`.
    ///
    /// Function type: parameter types, return type, optional effect set, and
    /// optional capability set.
    Arrow {
        /// Parameter types.
        params: Vec<NodeId>,
        /// Return type.
        ret: NodeId,
        /// Optional effect row.
        effects: Option<NodeId>,
        /// Optional capability set.
        capabilities: Option<NodeId>,
    },

    /// `(T1, T2, ...)`.
    ///
    /// Tuple type.
    Tuple {
        /// Element types.
        elements: Vec<NodeId>,
    },

    /// `<LinClass> T`.
    ///
    /// Substructural class annotation (linear, ordered, affine, unrestricted).
    LinearClass {
        /// Substructural class.
        class: LinClass,
        /// Inner type.
        inner: NodeId,
    },

    /// `eff1, eff2 | rest` or `ε`.
    ///
    /// Effect row: a list of effects with optional tail variable.
    EffectRow {
        /// Effect items.
        items: Vec<NodeId>,
        /// Optional rest variable.
        rest: Option<NodeId>,
    },

    /// `*T`.
    ///
    /// Pointer type: a type prefixed by `*`.
    Ptr {
        /// Pointed-to type.
        pointee: NodeId,
    },

    /// `record { field1: T1, field2: T2, ... }`.
    ///
    /// Record type: a named product type with labeled fields.
    Record {
        /// Record fields: each is (field_name_node, field_type_node).
        fields: Vec<(NodeId, NodeId)>,
    },

    /// `enum { Variant1, Variant2(T1, T2), Variant3 { f1: T1, f2: T2 }, ... }`.
    ///
    /// Enum type: a sum type with variants that can be unit, tuple-payload, or record-payload.
    Enum {
        /// Enum variants: each can be unit-shaped, tuple-shaped, or record-shaped.
        variants: Vec<EnumVariant>,
    },

    /// `Self::Item` — Self-qualified type reference.
    ///
    /// Represents a path to an associated type on Self within a trait context.
    /// In phase 4, this is parse-only; the resolver will validate that `item`
    /// refers to a valid associated type on the trait.
    SelfQualifiedPath {
        /// Name of the associated type (Ident node).
        item: NodeId,
    },
}

/// One variant of an enum type.
///
/// Variants come in three shapes: unit (no payload), tuple (positional payload),
/// or record (labeled payload).
#[derive(Clone, Debug)]
pub enum EnumVariant {
    /// Unit variant with no payload.
    Unit {
        /// Variant name (Ident node).
        name: NodeId,
    },
    /// Tuple variant with positional payload.
    Tuple {
        /// Variant name (Ident node).
        name: NodeId,
        /// Payload type nodes.
        payload: Vec<NodeId>,
    },
    /// Record variant with labeled payload.
    Record {
        /// Variant name (Ident node).
        name: NodeId,
        /// Payload fields: each is (field_name_node, field_type_node).
        fields: Vec<(NodeId, NodeId)>,
    },
}

/// Substructural type class.
///
/// Qualifies a type with a linearity/affinity constraint. The `*Mark` variants
/// are shorthand: `LinearMark` (↓ / $) for linear, `AffineMark` (~) for affine.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LinClass {
    /// Ordered (most permissive within linear types).
    Ordered,
    /// Linear (consume exactly once).
    Linear,
    /// Affine (consume at most once).
    Affine,
    /// Unrestricted (no linearity constraint).
    Unrestricted,
    /// Shorthand for linear: `↓` (U+2193) / `$`.
    LinearMark,
    /// Shorthand for affine: `~` (U+007E).
    AffineMark,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nodeid(n: u32) -> NodeId {
        NodeId::new(n).unwrap()
    }

    #[test]
    fn type_name_constructs() {
        let name = make_nodeid(1);
        let arg1 = make_nodeid(2);
        let arg2 = make_nodeid(3);
        let ty = TypeData::Name {
            name,
            args: vec![arg1, arg2],
        };
        match ty {
            TypeData::Name { name: n, args: a } => {
                assert_eq!(n, name);
                assert_eq!(a.len(), 2);
            }
            _ => panic!("expected Name variant"),
        }
    }

    #[test]
    fn type_arrow_constructs() {
        let param1 = make_nodeid(1);
        let param2 = make_nodeid(2);
        let ret = make_nodeid(3);
        let eff = make_nodeid(4);
        let ty = TypeData::Arrow {
            params: vec![param1, param2],
            ret,
            effects: Some(eff),
            capabilities: None,
        };
        match ty {
            TypeData::Arrow {
                params: p,
                ret: r,
                effects: e,
                capabilities: c,
            } => {
                assert_eq!(p.len(), 2);
                assert_eq!(r, ret);
                assert_eq!(e, Some(eff));
                assert!(c.is_none());
            }
            _ => panic!("expected Arrow variant"),
        }
    }

    #[test]
    fn type_tuple_constructs() {
        let elem1 = make_nodeid(1);
        let elem2 = make_nodeid(2);
        let ty = TypeData::Tuple {
            elements: vec![elem1, elem2],
        };
        match ty {
            TypeData::Tuple { elements: e } => {
                assert_eq!(e.len(), 2);
            }
            _ => panic!("expected Tuple variant"),
        }
    }

    #[test]
    fn type_linear_class_constructs() {
        let inner = make_nodeid(1);
        let ty = TypeData::LinearClass {
            class: LinClass::Linear,
            inner,
        };
        match ty {
            TypeData::LinearClass { class: c, inner: i } => {
                assert_eq!(c, LinClass::Linear);
                assert_eq!(i, inner);
            }
            _ => panic!("expected LinearClass variant"),
        }
    }

    #[test]
    fn type_effect_row_constructs() {
        let eff1 = make_nodeid(1);
        let eff2 = make_nodeid(2);
        let rest = make_nodeid(3);
        let ty = TypeData::EffectRow {
            items: vec![eff1, eff2],
            rest: Some(rest),
        };
        match ty {
            TypeData::EffectRow { items: it, rest: r } => {
                assert_eq!(it.len(), 2);
                assert_eq!(r, Some(rest));
            }
            _ => panic!("expected EffectRow variant"),
        }
    }

    #[test]
    fn linclass_variants_exist() {
        let _ = LinClass::Ordered;
        let _ = LinClass::Linear;
        let _ = LinClass::Affine;
        let _ = LinClass::Unrestricted;
        let _ = LinClass::LinearMark;
        let _ = LinClass::AffineMark;
    }
}
