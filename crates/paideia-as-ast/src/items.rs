//! Item-specific structured data.
//!
//! [`ItemData`] is an enum that carries the semantic payload for item nodes
//! (Module, Signature, Let, Effect, etc.). Each variant holds `NodeId`
//! references to child nodes that will be filled in by the parser.

use crate::NodeId;

/// Structured payload for item nodes.
///
/// Each variant corresponds to a top-level item kind (Module, Let, Effect, etc.)
/// as specified in §8 of the syntax reference. Child `NodeId` fields point to
/// other nodes in the arena; those nodes' concrete kinds (Expr, Type, Pattern)
/// are introduced by later PRs.
///
/// Fields named `name` always point to an `Ident` node. Fields named `doc`
/// hold an optional `StringLit` node for documentation comments.
#[derive(Clone, Debug)]
pub enum ItemData {
    /// Module declaration: `module Name (: Sig)? = Body`
    Module {
        /// Name of the module (Ident node).
        name: NodeId,
        /// Optional signature ascription.
        sig: Option<NodeId>,
        /// Module body (Structure or Functor node).
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Signature declaration: analogous to Structure but introduces a signature.
    Signature {
        /// Name of the signature (Ident node).
        name: NodeId,
        /// Signature body (Structure node with type declarations).
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Structure: `struct { ItemDecl* }`
    Structure {
        /// Item declarations in this structure.
        items: Vec<NodeId>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Functor: `functor (Param)+ -> struct { ItemDecl* }`
    Functor {
        /// Functor parameters (FunctorParam nodes).
        params: Vec<NodeId>,
        /// Functor body (Structure node).
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Functor parameter: `Name : Sig`
    FunctorParam {
        /// Name of the parameter (Ident node).
        name: NodeId,
        /// Signature ascription (SignatureRef node).
        sig: NodeId,
    },

    /// Effect declaration: `effect Name { OpSig+ }`
    Effect {
        /// Name of the effect (Ident node).
        name: NodeId,
        /// Operation signatures (OpSig nodes).
        ops: Vec<NodeId>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Operation signature: `op Name : Type (!{ EffectSet })?`
    OpSig {
        /// Name of the operation (Ident node).
        name: NodeId,
        /// Type signature (Type node).
        ty: NodeId,
        /// Optional effect set constraint.
        effect_set: Option<NodeId>,
    },

    /// Capability declaration.
    Capability {
        /// Name of the capability (Ident node).
        name: NodeId,
        /// Capability body.
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Let binding: `let Name (: Type)? = Expr`
    Let {
        /// Name of the binding (Ident node).
        name: NodeId,
        /// Optional type annotation (Type node).
        ty: Option<NodeId>,
        /// Value expression (Expr node).
        value: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Struct type declaration.
    Struct {
        /// Name of the struct (Ident node).
        name: NodeId,
        /// Struct fields.
        fields: Vec<NodeId>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Enum type declaration.
    Enum {
        /// Name of the enum (Ident node).
        name: NodeId,
        /// Enum variants.
        variants: Vec<NodeId>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Unsafe block: `unsafe { effects: {...} capabilities: {...} justification: "..." block: {...} }`
    UnsafeBlock {
        /// Effects declared in the block.
        effects: Vec<NodeId>,
        /// Capabilities declared in the block.
        capabilities: Vec<NodeId>,
        /// Justification (StringLit node).
        justification: NodeId,
        /// Body statements.
        block: Vec<NodeId>,
    },

    /// Placeholder for non-item nodes (expressions, types, patterns, statements).
    /// Used by later PRs.
    NonItem,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_data_module_variant_constructs() {
        let name = NodeId::new(1).unwrap();
        let body = NodeId::new(2).unwrap();
        let item = ItemData::Module {
            name,
            sig: None,
            body,
            doc: None,
        };
        match item {
            ItemData::Module {
                name: n,
                sig: s,
                body: b,
                doc: d,
            } => {
                assert_eq!(n, name);
                assert!(s.is_none());
                assert_eq!(b, body);
                assert!(d.is_none());
            }
            _ => panic!("expected Module variant"),
        }
    }

    #[test]
    fn item_data_let_with_type_constructs() {
        let name = NodeId::new(1).unwrap();
        let ty = NodeId::new(2).unwrap();
        let value = NodeId::new(3).unwrap();
        let item = ItemData::Let {
            name,
            ty: Some(ty),
            value,
            doc: None,
        };
        match item {
            ItemData::Let {
                name: n,
                ty: t,
                value: v,
                doc: d,
            } => {
                assert_eq!(n, name);
                assert_eq!(t, Some(ty));
                assert_eq!(v, value);
                assert!(d.is_none());
            }
            _ => panic!("expected Let variant"),
        }
    }

    #[test]
    fn item_data_structure_with_items_constructs() {
        let item1 = NodeId::new(1).unwrap();
        let item2 = NodeId::new(2).unwrap();
        let item = ItemData::Structure {
            items: vec![item1, item2],
            doc: None,
        };
        match item {
            ItemData::Structure { items: its, doc: d } => {
                assert_eq!(its.len(), 2);
                assert_eq!(its[0], item1);
                assert_eq!(its[1], item2);
                assert!(d.is_none());
            }
            _ => panic!("expected Structure variant"),
        }
    }
}
